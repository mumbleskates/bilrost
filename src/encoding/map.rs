use bytes::{Buf, BufMut};

use crate::buf::ReverseBuf;
use crate::encoding::value_traits::{DistinguishedMapping, Mapping};
use crate::encoding::{
    encode_varint, encoded_len_varint, encoder_where_value_encoder, prepend_varint, Canonicity,
    Capped, DecodeContext, DecodeError, DistinguishedValueDecoder, ForOverwrite,
    RestrictedDecodeContext, ValueDecoder, ValueEncoder, WireType, Wiretyped,
};
use crate::DecodeErrorKind::Truncated;

pub struct Map<KE, VE>(KE, VE);

encoder_where_value_encoder!(Map<KE, VE>, with where clause (T: Mapping), with generics (KE, VE));

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

impl<M, K, V, KE, VE> ValueDecoder<Map<KE, VE>> for M
where
    M: Mapping<Key = K, Value = V>,
    K: ForOverwrite + ValueDecoder<KE>,
    V: ForOverwrite + ValueDecoder<VE>,
{
    fn decode_value<B: Buf + ?Sized>(
        value: &mut M,
        mut buf: Capped<B>,
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
            ValueDecoder::<KE>::decode_value(&mut new_key, capped.lend(), ctx.clone())?;
            ValueDecoder::<VE>::decode_value(&mut new_val, capped.lend(), ctx.clone())?;
            value.insert(new_key, new_val)?;
        }
        Ok(())
    }
}

impl<M, K, V, KE, VE> DistinguishedValueDecoder<Map<KE, VE>> for M
where
    M: DistinguishedMapping<Key = K, Value = V> + Eq,
    K: ForOverwrite + Eq + DistinguishedValueDecoder<KE>,
    V: ForOverwrite + Eq + DistinguishedValueDecoder<VE>,
{
    const CHECKS_EMPTY: bool = false;

    fn decode_value_distinguished<const ALLOW_EMPTY: bool>(
        value: &mut M,
        mut buf: Capped<impl Buf + ?Sized>,
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
        let canon = &mut Canonicity::Canonical;
        while capped.has_remaining()? {
            let mut new_key = K::for_overwrite();
            let mut new_val = V::for_overwrite();
            ctx.update(
                canon,
                DistinguishedValueDecoder::<KE>::decode_value_distinguished::<true>(
                    &mut new_key,
                    capped.lend(),
                    ctx.clone(),
                )?,
            )?;
            ctx.update(
                canon,
                DistinguishedValueDecoder::<VE>::decode_value_distinguished::<true>(
                    &mut new_val,
                    capped.lend(),
                    ctx.clone(),
                )?,
            )?;
            ctx.update(canon, value.insert_distinguished(new_key, new_val)?)?;
        }
        Ok(*canon)
    }
}

#[cfg(test)]
mod test {
    mod btree {
        mod general {
            use crate::encoding::test::check_type_test;
            use crate::encoding::{General, Map};
            use alloc::collections::BTreeMap;
            check_type_test!(
                Map<General, General>,
                relaxed,
                BTreeMap<u64, f32>,
                WireType::LengthDelimited
            );
            check_type_test!(
                Map<General, General>,
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
