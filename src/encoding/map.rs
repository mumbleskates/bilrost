use bytes::{Buf, BufMut};

use crate::encoding::value_traits::{DistinguishedMapping, Mapping};
use crate::encoding::{
    encode_varint, encoded_len_varint, encoder_where_value_encoder, Canonicity, Capped,
    DecodeContext, DecodeError, DistinguishedValueEncoder, Encoder, NewForOverwrite, TagMeasurer,
    TagWriter, ValueEncoder, WireType, Wiretyped,
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
    K: NewForOverwrite + ValueEncoder<KE>,
    V: NewForOverwrite + ValueEncoder<VE>,
{
    fn encode_value<B: BufMut + ?Sized>(value: &M, buf: &mut B) {
        encode_varint(map_encoded_length::<M, KE, VE>(value) as u64, buf);
        for (key, val) in value.iter() {
            ValueEncoder::<KE>::encode_value(key, buf);
            ValueEncoder::<VE>::encode_value(val, buf);
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
        let mut capped = buf.take_length_delimited()?;
        if combined_fixed_size(
            <M::Key as Wiretyped<KE>>::WIRE_TYPE,
            <M::Value as Wiretyped<VE>>::WIRE_TYPE,
        )
        .map_or(false, |fixed_size| {
            capped.remaining_before_cap() % fixed_size != 0
        }) {
            return Err(DecodeError::new(Truncated));
        }
        while capped.has_remaining()? {
            let mut new_key = K::new_for_overwrite();
            let mut new_val = V::new_for_overwrite();
            ValueEncoder::<KE>::decode_value(&mut new_key, capped.lend(), ctx.clone())?;
            ValueEncoder::<VE>::decode_value(&mut new_val, capped.lend(), ctx.clone())?;
            value.insert(new_key, new_val)?;
        }
        Ok(())
    }
}

impl<M, K, V, KE, VE> DistinguishedValueEncoder<Map<KE, VE>> for M
where
    M: DistinguishedMapping<Key = K, Value = V> + Eq,
    K: NewForOverwrite + Eq + DistinguishedValueEncoder<KE>,
    V: NewForOverwrite + Eq + DistinguishedValueEncoder<VE>,
{
    fn decode_value_distinguished<B: Buf + ?Sized>(
        value: &mut M,
        mut buf: Capped<B>,
        allow_empty: bool,
        ctx: DecodeContext,
    ) -> Result<Canonicity, DecodeError> {
        let mut capped = buf.take_length_delimited()?;
        if !allow_empty && capped.remaining_before_cap() == 0 {
            return Ok(Canonicity::NotCanonical);
        }
        if combined_fixed_size(
            <M::Key as Wiretyped<KE>>::WIRE_TYPE,
            <M::Value as Wiretyped<VE>>::WIRE_TYPE,
        )
        .map_or(false, |fixed_size| {
            capped.remaining_before_cap() % fixed_size != 0
        }) {
            return Err(DecodeError::new(Truncated));
        }
        let mut canon = Canonicity::Canonical;
        while capped.has_remaining()? {
            let mut new_key = K::new_for_overwrite();
            let mut new_val = V::new_for_overwrite();
            canon.update(DistinguishedValueEncoder::<KE>::decode_value_distinguished(
                &mut new_key,
                capped.lend(),
                true,
                ctx.clone(),
            )?);
            canon.update(DistinguishedValueEncoder::<VE>::decode_value_distinguished(
                &mut new_val,
                capped.lend(),
                true,
                ctx.clone(),
            )?);
            canon.update(value.insert_distinguished(new_key, new_val)?);
        }
        Ok(canon)
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
