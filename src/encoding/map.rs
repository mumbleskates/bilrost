use crate::encoding::value_traits::Mapping;
use crate::encoding::{
    encode_varint, encoded_len_varint, Capped, DecodeContext, Encoder, General, NewForOverwrite,
    ValueEncoder, WireType, Wiretyped,
};
use crate::DecodeError;
use bytes::{Buf, BufMut};

struct Map<KE = General, VE = General>(KE, VE);

/// Maps are always length delimited.
impl<T, KE, VE> Wiretyped<T> for Map<KE, VE> {
    const WIRE_TYPE: WireType = WireType::LengthDelimited;
}

const fn combined_fixed_size(a: WireType, b: WireType) -> Option<usize> {
    Some(a.fixed_size()? + b.fixed_size()?)
}

fn map_encoded_length<M, KE, VE>(value: &M) -> usize
where
    M: Mapping,
    KE: Wiretyped<M::Key>,
    VE: Wiretyped<M::Value>,
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
    fn encode_value<B: BufMut>(value: &M, buf: &mut B) {
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

    fn decode_value<B: Buf>(
        value: &mut M,
        buf: &mut Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        let capped = buf.take_length_delimited()?;
        if combined_fixed_size(KE::WIRE_TYPE, VE::WIRE_TYPE).map_or(false, |fixed_size| {
            buf.remaining_before_cap() % fixed_size != 0
        }) {
            return Err(DecodeError::new("packed field is not a valid length"));
        }
        for item in capped.consume(|buf| {
            let mut new_key = K::new_for_overwrite();
            let mut new_val = V::new_for_overwrite();
            KE::decode_value(&mut new_key, buf, ctx.clone())?;
            VE::decode_value(&mut new_val, buf, ctx.clone())?;
            Ok((new_key, new_val))
        }) {
            let (key, val) = item?;
            value.insert(key, val).map_err(DecodeError::new)?;
        }
        Ok(())
    }
}

// TODO(widders): DistinguishedValueEncoder for maps

// TODO(widders): Encoder & DistinguishedEncoder for Map<..> (bare values rejecting empty in
//  distinguished mode)

// TODO(widders): change map configurations
//  * maps should be packed! keys and values should directly alternate within a length-
//    delineated field
//  * delegate value encoding for maps from General to Map<General, General>

// TODO(widders): hashbrown support in a feature

#[cfg(test)]
mod test {
    // TODO(widders): tests
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
