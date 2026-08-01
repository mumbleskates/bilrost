use crate::buf::ReverseBuf;
use crate::encoding::schema::{PopulateSchema, Schema, ValueRepr};
use crate::encoding::value_traits::{DistinguishedMapping, Mapping};
use crate::encoding::{
    decoding_modes, encode_varint, encoded_len_varint, encoding_implemented_via_value_encoding,
    encoding_uses_base_empty_state, prepend_varint, Canonicity, Capped, DecodeContext, DecodeError,
    DistinguishedValueBorrowDecoder, DistinguishedValueDecoder, EmptyState, ForOverwrite,
    GeneralPacked, RestrictedDecodeContext, ValueBorrowDecoder, ValueDecoder, ValueEncoder,
    WireType, Wiretyped,
};
use crate::DecodeErrorKind::Truncated;
use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use bytes::{Buf, BufMut};
use core::fmt::Display;

pub struct Map<KE = GeneralPacked, VE = GeneralPacked>(KE, VE);

encoding_uses_base_empty_state!(Map<KE, VE>, with generics (KE, VE));
encoding_implemented_via_value_encoding!(
    Map<KE, VE>,
    with where clause (T: Mapping, (): EmptyState<(), T>),
    with generics (KE, VE),
);

/// Maps are always length delimited.
impl<T, KE, VE> Wiretyped<Map<KE, VE>, T> for () {
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
    (): EmptyState<(), M> + ValueEncoder<KE, M::Key> + ValueEncoder<VE, M::Value>,
{
    combined_fixed_size(
        <() as Wiretyped<KE, M::Key>>::WIRE_TYPE,
        <() as Wiretyped<VE, M::Value>>::WIRE_TYPE,
    )
    .map_or_else(
        || {
            value
                .iter()
                .map(|(k, v)| {
                    <() as ValueEncoder<KE, _>>::value_encoded_len(k)
                        + <() as ValueEncoder<VE, _>>::value_encoded_len(v)
                })
                .sum()
        },
        |fixed_size| value.len() * fixed_size, // Both key and value are constant length; shortcut
    )
}

impl<M, K, V, KE, VE> ValueRepr<Map<KE, VE>, M> for ()
where
    M: Mapping<Key = K, Value = V>,
    (): EmptyState<(), M> + ValueRepr<KE, K> + ValueRepr<VE, V>,
{
    fn repr(schema: &Schema) -> Box<dyn Display> {
        schema.make_lazy_repr(|schema| {
            let bounds = match M::BOUNDS.end {
                None => String::new(),
                Some(max) => format!("; at most {max} items"),
            };
            format!(
                "delimited map (keys: ({key_repr}); values: ({value_repr}){bounds})",
                key_repr = <() as ValueRepr<KE, K>>::repr(schema),
                value_repr = <() as ValueRepr<VE, V>>::repr(schema),
            )
        })
    }
}

impl<M, K, V, KE, VE> ValueEncoder<Map<KE, VE>, M> for ()
where
    M: Mapping<Key = K, Value = V>,
    (): EmptyState<(), M> + ValueEncoder<KE, K> + ValueEncoder<VE, V>,
{
    fn encode_value<B: BufMut + ?Sized>(value: &M, buf: &mut B) {
        encode_varint(map_encoded_length::<M, KE, VE>(value) as u64, buf);
        for (key, val) in value.iter() {
            <() as ValueEncoder<KE, _>>::encode_value(key, buf);
            <() as ValueEncoder<VE, _>>::encode_value(val, buf);
        }
    }

    fn prepend_value<B: ReverseBuf + ?Sized>(value: &M, buf: &mut B) {
        let end = buf.remaining();
        for (key, val) in value.reversed() {
            <() as ValueEncoder<VE, _>>::prepend_value(val, buf);
            <() as ValueEncoder<KE, _>>::prepend_value(key, buf);
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
        impl<$($lifetime,)? M, K, V, KE, VE> $relaxed_value <$($lifetime,)? Map<KE, VE>, M> for ()
        where
            M: Mapping<Key = K, Value = V>,
            (): EmptyState<(), M>
                + ForOverwrite<KE, K>
                + ForOverwrite<VE, V>
                + $relaxed_value <$($lifetime,)? KE, K>
                + $relaxed_value <$($lifetime,)? VE, V>,
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
                        <() as Wiretyped<KE, M::Key>>::WIRE_TYPE,
                        <() as Wiretyped<VE, M::Value>>::WIRE_TYPE,
                    ),
                    Some(fixed_size) if capped.remaining_before_cap() % fixed_size != 0
                ) {
                    // No number of fixed-sized key+value pairs can pack evenly into this size.
                    return Err(DecodeError::new(Truncated));
                }
                while capped.has_remaining()? {
                    let mut new_key = <() as ForOverwrite::<KE, K>>::for_overwrite();
                    let mut new_val = <() as ForOverwrite::<VE, V>>::for_overwrite();
                    <() as $relaxed_value<KE, _>>::$relaxed_value_method(
                        &mut new_key, capped.lend(), ctx.clone())?;
                    <() as $relaxed_value<VE, _>>::$relaxed_value_method(
                        &mut new_val, capped.lend(), ctx.clone())?;
                    value.insert(new_key, new_val)?;
                }
                Ok(())
            }
        }

        impl<$($lifetime,)? M, K, V, KE, VE>
        $distinguished_value <$($lifetime,)? Map<KE, VE>, M> for ()
        where
            M: DistinguishedMapping<Key = K, Value = V> + Eq,
            K: Eq,
            V: Eq,
            (): EmptyState<(), M>
                + ForOverwrite<KE, K>
                + ForOverwrite<VE, V>
                + $distinguished_value <$($lifetime,)? KE, K>
                + $distinguished_value <$($lifetime,)? VE, V>,
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
                        <() as Wiretyped<KE, M::Key>>::WIRE_TYPE,
                        <() as Wiretyped<VE, M::Value>>::WIRE_TYPE,
                    ),
                    Some(fixed_size) if capped.remaining_before_cap() % fixed_size != 0
                ) {
                    // No number of fixed-sized key+value pairs can pack evenly into this size.
                    return Err(DecodeError::new(Truncated));
                }
                let mut canon = Canonicity::Canonical;
                while capped.has_remaining()? {
                    let mut new_key = <() as ForOverwrite<KE, K>>::for_overwrite();
                    let mut new_val = <() as ForOverwrite<VE, V>>::for_overwrite();
                    canon.update(
                        <() as $distinguished_value<KE, _>>::$distinguished_value_method::<true>(
                            &mut new_key,
                            capped.lend(),
                            ctx.clone(),
                        )?,
                    );
                    canon.update(
                        <() as $distinguished_value<VE, _>>::$distinguished_value_method::<true>(
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

decoding_modes::__invoke!(impl_decoders, owned);
decoding_modes::__invoke!(impl_decoders, borrowed);

#[cfg(test)]
mod test {
    mod btree {
        mod general {
            use crate::encoding::test::check_type_test;
            use crate::encoding::Map;
            use alloc::collections::BTreeMap;
            check_type_test!(
                Map,
                relaxed,
                BTreeMap<u64, f32>,
                WireType::LengthDelimited
            );
            check_type_test!(
                Map,
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
            use crate::encoding::General;
            use alloc::collections::BTreeMap;
            check_type_test!(
                General,
                relaxed,
                BTreeMap<bool, f32>,
                WireType::LengthDelimited
            );
            check_type_test!(
                General,
                distinguished,
                BTreeMap<bool, u32>,
                WireType::LengthDelimited
            );
        }
    }
}
