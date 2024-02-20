use bytes::{Buf, BufMut};
use core::cmp::min;

use crate::encoding::value_traits::{DistinguishedMapping, Mapping};
use crate::encoding::{
    encode_varint, encoded_len_varint, encoder_where_value_encoder, Canonicity, Capped,
    DecodeContext, DecodeError, DistinguishedEncoder, DistinguishedValueEncoder, Encoder,
    NewForOverwrite, TagMeasurer, TagWriter, ValueEncoder, WireType, Wiretyped,
};
use crate::DecodeErrorKind::Truncated;

pub struct Map<KE, VE>(KE, VE);

encoder_where_value_encoder!(Map<KE, VE>, with where clause (T: Mapping), with generics (KE, VE));

/// Maps are always length delimited.
impl<T, KE, VE> Wiretyped<T> for Map<KE, VE> {
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
    KE: ValueEncoder<M::Key>,
    VE: ValueEncoder<M::Value>,
{
    combined_fixed_size(KE::WIRE_TYPE, VE::WIRE_TYPE).map_or_else(
        || {
            value
                .iter()
                .map(|(k, v)| KE::value_encoded_len(k) + VE::value_encoded_len(v))
                .sum()
        },
        |fixed_size| value.len() * fixed_size, // Both key and value are constant length; shortcut
    )
}

impl<M, K, V, KE, VE> ValueEncoder<M> for Map<KE, VE>
where
    M: Mapping<Key = K, Value = V>,
    KE: ValueEncoder<K>,
    VE: ValueEncoder<V>,
    K: NewForOverwrite,
    V: NewForOverwrite,
{
    fn encode_value<B: BufMut + ?Sized>(value: &M, buf: &mut B) {
        encode_varint(map_encoded_length::<M, KE, VE>(value) as u64, buf);
        for (key, val) in value.iter() {
            KE::encode_value(key, buf);
            VE::encode_value(val, buf);
        }
    }

    fn value_encoded_len(value: &M) -> usize {
        let inner_len = map_encoded_length::<M, KE, VE>(value);
        encoded_len_varint(inner_len as u64) + inner_len
    }

    fn decode_value<B: Buf + ?Sized>(
        value: &mut M,
        mut buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        let capped = buf.take_length_delimited()?;
        if combined_fixed_size(KE::WIRE_TYPE, VE::WIRE_TYPE).map_or(false, |fixed_size| {
            capped.remaining_before_cap() % fixed_size != 0
        }) {
            return Err(DecodeError::new(Truncated));
        }
        capped
            .consume(|buf| {
                let mut new_key = K::new_for_overwrite();
                let mut new_val = V::new_for_overwrite();
                KE::decode_value(&mut new_key, buf.lend(), ctx.clone())?;
                VE::decode_value(&mut new_val, buf.lend(), ctx.clone())?;
                Ok((new_key, new_val))
            })
            .try_for_each(|item| {
                let (key, val) = item?;
                Ok(value.insert(key, val)?)
            })
    }
}

impl<M, K, V, KE, VE> DistinguishedValueEncoder<M> for Map<KE, VE>
where
    M: DistinguishedMapping<Key = K, Value = V> + Eq,
    KE: DistinguishedValueEncoder<K>,
    VE: DistinguishedValueEncoder<V>,
    K: NewForOverwrite + Eq,
    V: NewForOverwrite + Eq,
{
    fn decode_value_distinguished<B: Buf + ?Sized>(
        value: &mut M,
        mut buf: Capped<B>,
        allow_empty: bool,
        ctx: DecodeContext,
    ) -> Result<Canonicity, DecodeError> {
        let capped = buf.take_length_delimited()?;
        if !allow_empty && !capped.has_remaining() {
            return Ok(Canonicity::NotCanonical);
        }
        if combined_fixed_size(KE::WIRE_TYPE, VE::WIRE_TYPE).map_or(false, |fixed_size| {
            capped.remaining_before_cap() % fixed_size != 0
        }) {
            return Err(DecodeError::new(Truncated));
        }
        capped
            .consume(|buf| {
                let mut new_key = K::new_for_overwrite();
                let mut new_val = V::new_for_overwrite();
                let item_canon = min(
                    KE::decode_value_distinguished(&mut new_key, buf.lend(), true, ctx.clone())?,
                    VE::decode_value_distinguished(&mut new_val, buf.lend(), true, ctx.clone())?,
                );
                Ok(min(
                    item_canon,
                    value.insert_distinguished(new_key, new_val)?,
                ))
            })
            .collect()
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
                expedient,
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
                expedient,
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
                expedient,
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

    #[cfg(feature = "std")]
    mod hash {
        mod general {
            use crate::encoding::test::check_type_test;
            use crate::encoding::{General, Map};
            use std::collections::HashMap;
            check_type_test!(
                Map<General, General>,
                expedient,
                HashMap<u64, f32>,
                WireType::LengthDelimited
            );
        }

        mod fixed {
            use crate::encoding::test::check_type_test;
            use crate::encoding::{Fixed, Map};
            use std::collections::HashMap;
            check_type_test!(
                Map<Fixed, Fixed>,
                expedient,
                HashMap<u64, f32>,
                WireType::LengthDelimited
            );
        }

        mod delegated_from_general {
            use crate::encoding::test::check_type_test;
            use crate::encoding::General;
            use std::collections::HashMap;
            check_type_test!(
                General,
                expedient,
                HashMap<bool, u32>,
                WireType::LengthDelimited
            );
        }
    }

    #[cfg(feature = "hashbrown")]
    mod hashbrown_hash {
        mod general {
            use crate::encoding::test::check_type_test;
            use crate::encoding::{General, Map};
            use alloc::collections::BTreeMap;
            use hashbrown::HashMap;
            check_type_test!(
                Map<General, General>,
                expedient,
                from BTreeMap<u64, f32>,
                into HashMap<u64, f32>,
                converter(value) {
                    <HashMap<u64, f32> as FromIterator<_>>::from_iter(value.into_iter())
                },
                WireType::LengthDelimited
            );
        }

        mod fixed {
            use crate::encoding::test::check_type_test;
            use crate::encoding::{Fixed, Map};
            use alloc::collections::BTreeMap;
            use hashbrown::HashMap;
            check_type_test!(
                Map<Fixed, Fixed>,
                expedient,
                from BTreeMap<u64, f32>,
                into HashMap<u64, f32>,
                converter(value) {
                    <HashMap<u64, f32> as FromIterator<_>>::from_iter(value.into_iter())
                },
                WireType::LengthDelimited
            );
        }

        mod delegated_from_general {
            use crate::encoding::test::check_type_test;
            use crate::encoding::General;
            use alloc::collections::BTreeMap;
            use hashbrown::HashMap;
            check_type_test!(
                General,
                expedient,
                from BTreeMap<bool, u32>,
                into HashMap<bool, u32>,
                converter(value) {
                    <HashMap<bool, u32> as FromIterator<_>>::from_iter(value.into_iter())
                },
                WireType::LengthDelimited
            );
        }
    }
}
