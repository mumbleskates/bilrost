use crate::buf::ReverseBuf;
use crate::encoding::value_traits::{DistinguishedMapping, Mapping};
use crate::encoding::{
    decoding_modes, encode_varint, encoded_len_varint, encoding_implemented_via_value_encoding,
    prepend_varint, Canonicity, Capped, DecodeContext, DecodeError,
    DistinguishedValueBorrowDecoder, DistinguishedValueDecoder, ForOverwrite,
    RestrictedDecodeContext, ValueBorrowDecoder, ValueDecoder, ValueEncoder, WireType, Wiretyped,
};
use crate::DecodeErrorKind::Truncated;
use bytes::{Buf, BufMut};

pub struct Map<KE, VE>(KE, VE);

encoding_implemented_via_value_encoding!(
    Map<KE, VE>,
    with where clause (T: Mapping),
    with generics (KE, VE),
);

/// Maps are always length delimited.
impl<T, KE, VE> Wiretyped<Map<KE, VE>> for T {
    const WIRE_TYPE: WireType = WireType::LengthDelimited;
}

const fn combined_fixed_size(a: WireType, b: WireType) -> Option<usize> {
    match (a.fixed_size(), b.fixed_size()) {
        (Some(a), Some(b)) => Some(a + b),
        _ => None,
    }
}

fn map_encoded_length<M, KE, VE>(value: &M) -> usize
where
    M: Mapping,
    M::Key: ValueEncoder<KE>,
    M::Value: ValueEncoder<VE>,
{
    combined_fixed_size(
        <M::Key as Wiretyped<KE>>::WIRE_TYPE,
        <M::Value as Wiretyped<VE>>::WIRE_TYPE,
    )
    .map_or_else(
        || {
            value
                .iter()
                .map(|(k, v)| {
                    ValueEncoder::<KE>::value_encoded_len(k)
                        + ValueEncoder::<VE>::value_encoded_len(v)
                })
                .sum()
        },
        |fixed_size| value.len() * fixed_size, // Both key and value are constant length; shortcut
    )
}

impl<M, K, V, KE, VE> ValueEncoder<Map<KE, VE>> for M
where
    M: Mapping<Key = K, Value = V>,
    K: ForOverwrite + ValueEncoder<KE>,
    V: ForOverwrite + ValueEncoder<VE>,
{
    fn encode_value<B: BufMut + ?Sized>(value: &M, buf: &mut B) {
        encode_varint(map_encoded_length::<M, KE, VE>(value) as u64, buf);
        for (key, val) in value.iter() {
            ValueEncoder::<KE>::encode_value(key, buf);
            ValueEncoder::<VE>::encode_value(val, buf);
        }
    }

    fn prepend_value<B: ReverseBuf + ?Sized>(value: &M, buf: &mut B) {
        let end = buf.remaining();
        for (key, val) in value.reversed() {
            ValueEncoder::<VE>::prepend_value(val, buf);
            ValueEncoder::<KE>::prepend_value(key, buf);
        }
        prepend_varint((buf.remaining() - end) as u64, buf);
    }

    fn value_encoded_len(value: &M) -> usize {
        let inner_len = map_encoded_length::<M, KE, VE>(value);
        encoded_len_varint(inner_len as u64) + inner_len
    }
}

macro_rules! impl_decoders {
    (
        mode: $mode:ident,
        relaxed: $relaxed:ident::$relaxed_method:ident,
        relaxed_value: $relaxed_value:ident::$relaxed_value_method:ident,
        relaxed_field: $relaxed_field:ident::$relaxed_field_method:ident,
        distinguished: $distinguished:ident::$distinguished_method:ident,
        distinguished_value: $distinguished_value:ident::$distinguished_value_method:ident,
        distinguished_field: $distinguished_field:ident::$distinguished_field_method:ident,
        buf_ty: $buf_ty:ty,
        impl_buf_ty: $impl_buf_ty:ty,
        $(buf_generic: ($($buf_generic:tt)*),)?
        $(lifetime: $lifetime:lifetime,)?
    ) => {
        impl<$($lifetime,)? M, K, V, KE, VE> $relaxed_value <$($lifetime,)? Map<KE, VE>> for M
        where
            M: Mapping<Key = K, Value = V>,
            K: ForOverwrite + $relaxed_value <$($lifetime,)? KE>,
            V: ForOverwrite + $relaxed_value <$($lifetime,)? VE>,
        {
            fn $relaxed_value_method $($($buf_generic)*)? (
                value: &mut M,
                mut buf: Capped<$buf_ty>,
                ctx: DecodeContext,
            ) -> Result<(), DecodeError> {
                let mut capped = buf.take_length_delimited()?;
                // MSRV: this could be .is_some_and(..)
                if matches!(
                    combined_fixed_size(
                        <M::Key as Wiretyped<KE>>::WIRE_TYPE,
                        <M::Value as Wiretyped<VE>>::WIRE_TYPE,
                    ),
                    Some(fixed_size) if capped.remaining_before_cap() % fixed_size != 0
                ) {
                    // No number of fixed-sized key+value pairs can pack evenly into this size.
                    return Err(DecodeError::new(Truncated));
                }
                while capped.has_remaining()? {
                    let mut new_key = K::for_overwrite();
                    let mut new_val = V::for_overwrite();
                    $relaxed_value::<KE>::$relaxed_value_method(
                        &mut new_key, capped.lend(), ctx.clone())?;
                    $relaxed_value::<VE>::$relaxed_value_method(
                        &mut new_val, capped.lend(), ctx.clone())?;
                    value.insert(new_key, new_val)?;
                }
                Ok(())
            }
        }

        impl<$($lifetime,)? M, K, V, KE, VE>
        $distinguished_value <$($lifetime,)? Map<KE, VE>> for M
        where
            M: DistinguishedMapping<Key = K, Value = V> + Eq,
            K: ForOverwrite + Eq + $distinguished_value <$($lifetime,)? KE>,
            V: ForOverwrite + Eq + $distinguished_value <$($lifetime,)? VE>,
        {
            const CHECKS_EMPTY: bool = false;

            fn $distinguished_value_method <const ALLOW_EMPTY: bool>(
                value: &mut M,
                mut buf: Capped<$impl_buf_ty>,
                ctx: RestrictedDecodeContext,
            ) -> Result<Canonicity, DecodeError> {
                let mut capped = buf.take_length_delimited()?;
                // MSRV: this could be .is_some_and(..)
                if matches!(
                    combined_fixed_size(
                        <M::Key as Wiretyped<KE>>::WIRE_TYPE,
                        <M::Value as Wiretyped<VE>>::WIRE_TYPE,
                    ),
                    Some(fixed_size) if capped.remaining_before_cap() % fixed_size != 0
                ) {
                    // No number of fixed-sized key+value pairs can pack evenly into this size.
                    return Err(DecodeError::new(Truncated));
                }
                let mut canon = Canonicity::Canonical;
                while capped.has_remaining()? {
                    let mut new_key = K::for_overwrite();
                    let mut new_val = V::for_overwrite();
                    canon.update(
                        $distinguished_value::<KE>::$distinguished_value_method::<true>(
                            &mut new_key,
                            capped.lend(),
                            ctx.clone(),
                        )?,
                    );
                    canon.update(
                        $distinguished_value::<VE>::$distinguished_value_method::<true>(
                            &mut new_val,
                            capped.lend(),
                            ctx.clone(),
                        )?,
                    );
                    canon.update(ctx.check(value.insert_distinguished(new_key, new_val)?)?);
                }
                Ok(canon)
            }
        }
    }
}

decoding_modes::invoke!(impl_decoders, owned);
decoding_modes::invoke!(impl_decoders, borrowed);

#[cfg(test)]
mod test {
    mod btree {
        mod general {
            use crate::encoding::test::check_type_test;
            use crate::encoding::{GeneralInMessage, Map};
            use alloc::collections::BTreeMap;
            check_type_test!(
                Map<GeneralInMessage, GeneralInMessage>,
                relaxed,
                BTreeMap<u64, f32>,
                WireType::LengthDelimited
            );
            check_type_test!(
                Map<GeneralInMessage, GeneralInMessage>,
                distinguished,
                BTreeMap<u32, i32>,
                WireType::LengthDelimited
            );
        }

        mod fixed {
            use crate::encoding::test::check_type_test;
            use crate::encoding::{Fixed, Map};
            use alloc::collections::BTreeMap;
            check_type_test!(
                Map<Fixed, Fixed>,
                relaxed,
                BTreeMap<u64, f32>,
                WireType::LengthDelimited
            );
            check_type_test!(
                Map<Fixed, Fixed>,
                distinguished,
                BTreeMap<u32, i32>,
                WireType::LengthDelimited
            );
        }

        mod delegated_from_general {
            use crate::encoding::test::check_type_test;
            use crate::encoding::GeneralInMessage;
            use alloc::collections::BTreeMap;
            check_type_test!(
                GeneralInMessage,
                relaxed,
                BTreeMap<bool, f32>,
                WireType::LengthDelimited
            );
            check_type_test!(
                GeneralInMessage,
                distinguished,
                BTreeMap<bool, u32>,
                WireType::LengthDelimited
            );
        }
    }
}
