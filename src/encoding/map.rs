use crate::encoding::value_traits::{DistinguishedMapping, Mapping};
use crate::encoding::{
    check_wire_type, encode_varint, encoded_len_varint, Capped, DecodeContext,
    DistinguishedEncoder, DistinguishedValueEncoder, Encoder, FieldEncoder, NewForOverwrite,
    TagMeasurer, TagWriter, ValueEncoder, WireType, Wiretyped,
};
use crate::DecodeError;
use bytes::{Buf, BufMut};

pub struct Map<KE, VE>(KE, VE);

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
            return Err(DecodeError::new("packed field is not a valid length"));
        }
        for item in capped.consume(|buf| {
            let mut new_key = K::new_for_overwrite();
            let mut new_val = V::new_for_overwrite();
            KE::decode_value(&mut new_key, buf.lend(), ctx.clone())?;
            VE::decode_value(&mut new_val, buf.lend(), ctx.clone())?;
            Ok((new_key, new_val))
        }) {
            let (key, val) = item?;
            value.insert(key, val).map_err(DecodeError::new)?;
        }
        Ok(())
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
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        let capped = buf.take_length_delimited()?;
        if combined_fixed_size(KE::WIRE_TYPE, VE::WIRE_TYPE).map_or(false, |fixed_size| {
            capped.remaining_before_cap() % fixed_size != 0
        }) {
            return Err(DecodeError::new("packed field is not a valid length"));
        }
        for item in capped.consume(|buf| {
            let mut new_key = K::new_for_overwrite();
            let mut new_val = V::new_for_overwrite();
            KE::decode_value_distinguished(&mut new_key, buf.lend(), ctx.clone())?;
            VE::decode_value_distinguished(&mut new_val, buf.lend(), ctx.clone())?;
            Ok((new_key, new_val))
        }) {
            let (key, val) = item?;
            value.insert(key, val).map_err(DecodeError::new)?;
        }
        Ok(())
    }
}

impl<M, KE, VE> Encoder<M> for Map<KE, VE>
where
    M: Mapping,
    Self: ValueEncoder<M>,
{
    fn encode<B: BufMut + ?Sized>(tag: u32, value: &M, buf: &mut B, tw: &mut TagWriter) {
        if !value.is_empty() {
            Self::encode_field(tag, value, buf, tw);
        }
    }

    fn encoded_len(tag: u32, value: &M, tm: &mut TagMeasurer) -> usize {
        if !value.is_empty() {
            Self::field_encoded_len(tag, value, tm)
        } else {
            0
        }
    }

    fn decode<B: Buf + ?Sized>(
        wire_type: WireType,
        duplicated: bool,
        value: &mut M,
        buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        check_wire_type(WireType::LengthDelimited, wire_type)?;
        if duplicated {
            return Err(DecodeError::new("multiple occurrences of map field"));
        }
        Self::decode_value(value, buf, ctx)
    }
}

impl<M, KE, VE> DistinguishedEncoder<M> for Map<KE, VE>
where
    M: DistinguishedMapping + Eq,
    Self: DistinguishedValueEncoder<M> + Encoder<M>,
{
    fn decode_distinguished<B: Buf + ?Sized>(
        wire_type: WireType,
        duplicated: bool,
        value: &mut M,
        buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        check_wire_type(WireType::LengthDelimited, wire_type)?;
        if duplicated {
            return Err(DecodeError::new("multiple occurrences of map field"));
        }
        Self::decode_value_distinguished(value, buf, ctx)?;
        if value.is_empty() {
            return Err(DecodeError::new("map field encoded with no items"));
        }
        Ok(())
    }
}

// TODO(widders): test hashbrown support

#[cfg(test)]
mod test {
    mod btree {
        use crate::encoding::check_type_test;
        check_type_test!(
            Map<General, General>,
            expedient,
            alloc::collections::BTreeMap<u64, f32>,
            WireType::LengthDelimited
        );
        check_type_test!(
            Map<General, General>,
            distinguished,
            alloc::collections::BTreeMap<u32, i32>,
            WireType::LengthDelimited
        );

        mod delegated_from_general {
            use crate::encoding::check_type_test;
            check_type_test!(
                General,
                expedient,
                alloc::collections::BTreeMap<bool, u32>,
                WireType::LengthDelimited
            );
            check_type_test!(
                General,
                distinguished,
                alloc::collections::BTreeMap<bool, u32>,
                WireType::LengthDelimited
            );
        }
    }

    #[cfg(feature = "std")]
    mod hash {
        use crate::encoding::check_type_test;
        check_type_test!(
            Map<General, General>,
            expedient,
            std::collections::HashMap<u64, f32>,
            WireType::LengthDelimited
        );

        mod delegated_from_general {
            use crate::encoding::check_type_test;
            check_type_test!(
                General,
                expedient,
                std::collections::HashMap<bool, u32>,
                WireType::LengthDelimited
            );
        }
    }

    // TODO(widders): more tests
    // map_tests!(keys: [
    //     (u32, uint32),
    //     (u64, uint64),
    //     (i32, sint32),
    //     (i64, sint64),
    //     (u32, ufixed32),
    //     (u64, ufixed64),
    //     (i32, sfixed32),
    //     (i64, sfixed64),
    //     (bool, bool),
    //     (String, string),
    //     (Vec<u8>, bytes)
    // ],
    // vals: [
    //     (f32, float32),
    //     (f64, float64),
    //     (u32, uint32),
    //     (u64, uint64),
    //     (i32, sint32),
    //     (i64, sint64),
    //     (u32, ufixed32),
    //     (u64, ufixed64),
    //     (i32, sfixed32),
    //     (i64, sfixed64),
    //     (bool, bool),
    //     (String, string),
    //     (Vec<u8>, bytes)
    // ]);
}
