use crate::encoding::value_traits::Mapping;
use crate::encoding::{
    encoded_len_varint, Capped, DecodeContext, General, NewForOverwrite, ValueEncoder, WireType,
    Wiretyped,
};
use crate::DecodeError;
use bytes::{Buf, BufMut};

struct Map<KE = General, VE = General>(KE, VE);

/// Maps are always length delimited.
impl<T, KE, VE> Wiretyped<T> for Map<KE, VE> {
    const WIRE_TYPE: WireType = WireType::LengthDelimited;
}

impl<M, K, V, KE, VE> ValueEncoder<M> for Map<KE, VE>
where
    M: Mapping<Key = K, Value = V>,
    KE: ValueEncoder<K>,
    VE: ValueEncoder<V>,
    K: NewForOverwrite,
    V: NewForOverwrite,
{
    fn encode_value<B: BufMut>(_value: &M, _buf: &mut B) {
        todo!()
    }

    fn value_encoded_len(value: &M) -> usize {
        let inner_len: usize = value
            .iter()
            .map(|(k, v)| KE::value_encoded_len(k) + VE::value_encoded_len(v))
            .sum();
        encoded_len_varint(inner_len as u64) + inner_len
    }

    fn decode_value<B: Buf>(
        _value: &mut M,
        _buf: &mut Capped<B>,
        _ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        todo!()
    }
}

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
