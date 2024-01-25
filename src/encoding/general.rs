use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::String;
use alloc::vec::Vec;
#[cfg(feature = "std")]
use core::hash::Hash;
use core::mem;
use core::str;
#[cfg(feature = "std")]
use std::collections::{HashMap, HashSet};

use bytes::{Buf, BufMut, Bytes};

use crate::encoding::{
    delegate_encoding, delegate_value_encoding, encode_varint, encoded_len_varint, Capped,
    DecodeContext, DistinguishedEncoder, DistinguishedFieldEncoder, DistinguishedValueEncoder,
    Encoder, FieldEncoder, Map, TagMeasurer, TagWriter, ValueEncoder, WireType, Wiretyped,
};
use crate::message::{merge, merge_distinguished, RawDistinguishedMessage, RawMessage};
use crate::{Blob, DecodeError};

pub struct General;

// General implements unpacked encodings by default, but only for select collection types. Other
// implementers of the `Collection` trait must use Unpacked or Packed.
delegate_encoding!(delegate from (General) to (crate::encoding::Unpacked<General>)
    for type (Vec<T>) including distinguished with generics <T>);
delegate_encoding!(delegate from (General) to (crate::encoding::Unpacked<General>)
    for type (BTreeSet<T>) including distinguished with generics <T>);
delegate_value_encoding!(delegate from (General) to (Map<General, General>)
    for type (BTreeMap<K, V>) including distinguished
    with where clause for expedient (K: Ord)
    with where clause for distinguished (V: Eq)
    with generics <K, V>);
#[cfg(feature = "std")]
delegate_encoding!(delegate from (General) to (crate::encoding::Unpacked<General>)
    for type (HashSet<T>) with generics <T>);
#[cfg(feature = "std")]
delegate_value_encoding!(delegate from (General) to (Map<General, General>)
    for type (HashMap<K, V>)
    with where clause (K: Eq + Hash)
    with generics <K, V>);
#[cfg(feature = "hashbrown")]
delegate_encoding!(delegate from (General) to (crate::encoding::Unpacked<General>)
    for type (hashbrown::HashSet<T>) with generics <T>);
#[cfg(feature = "hashbrown")]
delegate_value_encoding!(delegate from (General) to (Map<General, General>)
    for type (hashbrown::HashMap<K, V>)
    with where clause (K: Eq + Hash)
    with generics <K, V>);

/// General encodes plain values only when they are non-default.
impl<T> Encoder<T> for General
where
    General: ValueEncoder<T>,
    T: Default + PartialEq,
{
    #[inline]
    fn encode<B: BufMut + ?Sized>(tag: u32, value: &T, buf: &mut B, tw: &mut TagWriter) {
        if *value != T::default() {
            Self::encode_field(tag, value, buf, tw);
        }
    }

    #[inline]
    fn encoded_len(tag: u32, value: &T, tm: &mut TagMeasurer) -> usize {
        if *value != T::default() {
            Self::field_encoded_len(tag, value, tm)
        } else {
            0
        }
    }

    #[inline]
    fn decode<B: Buf + ?Sized>(
        wire_type: WireType,
        duplicated: bool,
        value: &mut T,
        buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        if duplicated {
            return Err(DecodeError::new(
                "multiple occurrences of non-repeated field",
            ));
        }
        Self::decode_field(wire_type, value, buf, ctx)
    }
}

/// General's distinguished encoding for plain values forbids encoding defaulted values. This
/// includes directly-nested message types, which are not emitted when all their fields are default.
impl<T> DistinguishedEncoder<T> for General
where
    General: DistinguishedValueEncoder<T> + Encoder<T>,
    T: Default + Eq,
{
    #[inline]
    fn decode_distinguished<B: Buf + ?Sized>(
        wire_type: WireType,
        duplicated: bool,
        value: &mut T,
        buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        if duplicated {
            return Err(DecodeError::new(
                "multiple occurrences of non-repeated field",
            ));
        }
        Self::decode_field_distinguished(wire_type, value, buf, ctx)?;
        if *value == T::default() {
            return Err(DecodeError::new(
                "plain field was encoded with its zero value",
            ));
        }
        Ok(())
    }
}

/// Macro which emits implementations for variable width numeric encoding.
macro_rules! varint {
    (
        $name:ident,
        $ty:ty,
        to_uint64($to_uint64_value:ident) $to_uint64:expr,
        from_uint64($from_uint64_value:ident) $from_uint64:expr
    ) => {
        impl Wiretyped<$ty> for General {
            const WIRE_TYPE: WireType = WireType::Varint;
        }

        impl ValueEncoder<$ty> for General {
            #[inline]
            fn encode_value<B: BufMut + ?Sized>($to_uint64_value: &$ty, buf: &mut B) {
                encode_varint($to_uint64, buf);
            }

            #[inline]
            fn value_encoded_len($to_uint64_value: &$ty) -> usize {
                encoded_len_varint($to_uint64)
            }

            #[inline]
            fn decode_value<B: Buf + ?Sized>(
                __value: &mut $ty,
                mut buf: Capped<B>,
                _ctx: DecodeContext,
            ) -> Result<(), DecodeError> {
                let $from_uint64_value = buf.decode_varint()?;
                *__value = $from_uint64;
                Ok(())
            }
        }

        impl DistinguishedValueEncoder<$ty> for General {
            #[inline]
            fn decode_value_distinguished<B: Buf + ?Sized>(
                value: &mut $ty,
                buf: Capped<B>,
                ctx: DecodeContext,
            ) -> Result<(), DecodeError> {
                Self::decode_value(value, buf, ctx)
            }
        }

        #[cfg(test)]
        mod $name {
            crate::encoding::check_type_test!(General, expedient, $ty, WireType::Varint);
            crate::encoding::check_type_test!(General, distinguished, $ty, WireType::Varint);
        }
    };
}

varint!(varint_bool, bool,
to_uint64(value) {
    u64::from(*value)
},
from_uint64(value) {
    match value {
        0 => false,
        1 => true,
        _ => return Err(DecodeError::new("invalid varint value for bool"))
    }
});
varint!(varint_u32, u32,
to_uint64(value) {
    *value as u64
},
from_uint64(value) {
    u32::try_from(value).map_err(|_| DecodeError::new("varint overflows range of u32"))?
});
varint!(varint_u64, u64,
to_uint64(value) {
    *value
},
from_uint64(value) {
    value
});
varint!(varint_i32, i32,
to_uint64(value) {
    super::i32_to_unsigned(*value) as u64
},
from_uint64(value) {
    let value = u32::try_from(value)
        .map_err(|_| DecodeError::new("varint overflows range of i32"))?;
    super::u32_to_signed(value)
});
varint!(varint_i64, i64,
to_uint64(value) {
    super::i64_to_unsigned(*value)
},
from_uint64(value) {
    super::u64_to_signed(value)
});

// General also encodes floating point values.
delegate_value_encoding!(delegate from (General) to (crate::encoding::Fixed) for type (f32));
delegate_value_encoding!(delegate from (General) to (crate::encoding::Fixed) for type (f64));

impl Wiretyped<String> for General {
    const WIRE_TYPE: WireType = WireType::LengthDelimited;
}

// TODO(widders): rope string? Cow string? cow string is probably pretty doable. does it matter?

impl ValueEncoder<String> for General {
    fn encode_value<B: BufMut + ?Sized>(value: &String, buf: &mut B) {
        encode_varint(value.len() as u64, buf);
        buf.put_slice(value.as_bytes());
    }

    fn value_encoded_len(value: &String) -> usize {
        encoded_len_varint(value.len() as u64) + value.len()
    }

    fn decode_value<B: Buf + ?Sized>(
        value: &mut String,
        mut buf: Capped<B>,
        _ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        // ## Unsafety
        //
        // Copies string data from the buffer, with an additional check of utf-8 well-formedness.
        // If the utf-8 is not well-formed, or if any other error occurs while copying the data,
        // then the string is cleared so as to avoid leaking a string field with invalid data.
        //
        // This implementation uses the unsafe `String::as_mut_vec` method instead of the safe
        // alternative of temporarily swapping an empty `String` into the field, because it results
        // in up to 10% better performance on the protobuf message decoding benchmarks.
        //
        // It's required when using `String::as_mut_vec` that invalid utf-8 data not be leaked into
        // the backing `String`. To enforce this, even in the event of a panic in the decoder or
        // in the buf implementation, a drop guard is used.
        unsafe {
            struct DropGuard<'a>(&'a mut Vec<u8>);
            impl<'a> Drop for DropGuard<'a> {
                #[inline]
                fn drop(&mut self) {
                    self.0.clear();
                }
            }

            let source = buf.take_length_delimited()?.take_all();
            // If we must copy, make sure to copy only once.
            value.clear();
            value.reserve(source.remaining());
            let drop_guard = DropGuard(value.as_mut_vec());
            drop_guard.0.put(source);
            match str::from_utf8(drop_guard.0) {
                Ok(_) => {
                    // Success; do not clear the bytes.
                    mem::forget(drop_guard);
                    Ok(())
                }
                Err(_) => Err(DecodeError::new(
                    "invalid string value: data is not UTF-8 encoded",
                )),
            }
        }
    }
}

impl DistinguishedValueEncoder<String> for General {
    fn decode_value_distinguished<B: Buf + ?Sized>(
        value: &mut String,
        buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        Self::decode_value(value, buf, ctx)
    }
}

#[cfg(test)]
mod string {
    use crate::encoding::check_type_test;
    check_type_test!(
        General,
        expedient,
        alloc::string::String,
        WireType::LengthDelimited
    );
    check_type_test!(
        General,
        distinguished,
        alloc::string::String,
        WireType::LengthDelimited
    );
}

impl Wiretyped<Bytes> for General {
    const WIRE_TYPE: WireType = WireType::LengthDelimited;
}

impl ValueEncoder<Bytes> for General {
    fn encode_value<B: BufMut + ?Sized>(value: &Bytes, mut buf: &mut B) {
        encode_varint(value.len() as u64, buf);
        (&mut buf).put(value.clone()); // `put` needs Self to be sized, so we use the ref type
    }

    fn value_encoded_len(value: &Bytes) -> usize {
        encoded_len_varint(value.len() as u64) + value.len()
    }

    fn decode_value<B: Buf + ?Sized>(
        value: &mut Bytes,
        mut buf: Capped<B>,
        _ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        let mut buf = buf.take_length_delimited()?;
        let len = buf.remaining_before_cap();
        *value = buf.copy_to_bytes(len);
        Ok(())
    }
}

impl DistinguishedValueEncoder<Bytes> for General {
    fn decode_value_distinguished<B: Buf + ?Sized>(
        value: &mut Bytes,
        buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        Self::decode_value(value, buf, ctx)
    }
}

#[cfg(test)]
mod bytes_blob {
    use crate::encoding::check_type_test;
    check_type_test!(
        General,
        expedient,
        from Vec<u8>,
        into bytes::Bytes,
        WireType::LengthDelimited
    );
    check_type_test!(
        General,
        distinguished,
        from Vec<u8>,
        into bytes::Bytes,
        WireType::LengthDelimited
    );
}

impl Wiretyped<Blob> for General {
    const WIRE_TYPE: WireType = WireType::LengthDelimited;
}

impl ValueEncoder<Blob> for General {
    #[inline]
    fn encode_value<B: BufMut + ?Sized>(value: &Blob, buf: &mut B) {
        crate::encoding::VecBlob::encode_value(value, buf)
    }

    #[inline]
    fn value_encoded_len(value: &Blob) -> usize {
        crate::encoding::VecBlob::value_encoded_len(value)
    }

    #[inline]
    fn decode_value<B: Buf + ?Sized>(
        value: &mut Blob,
        buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        crate::encoding::VecBlob::decode_value(value, buf, ctx)
    }
}

impl DistinguishedValueEncoder<Blob> for General {
    #[inline]
    fn decode_value_distinguished<B: Buf + ?Sized>(
        value: &mut Blob,
        buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        crate::encoding::VecBlob::decode_value_distinguished(value, buf, ctx)
    }
}

#[cfg(test)]
mod blob {
    use crate::encoding::check_type_test;
    check_type_test!(General, expedient, crate::Blob, WireType::LengthDelimited);
    check_type_test!(
        General,
        distinguished,
        crate::Blob,
        WireType::LengthDelimited
    );
}

impl<T> Wiretyped<T> for General
where
    T: RawMessage,
{
    const WIRE_TYPE: WireType = WireType::LengthDelimited;
}

impl<T> ValueEncoder<T> for General
where
    T: RawMessage,
{
    fn encode_value<B: BufMut + ?Sized>(value: &T, buf: &mut B) {
        encode_varint(value.raw_encoded_len() as u64, buf);
        value.raw_encode(buf);
    }

    fn value_encoded_len(value: &T) -> usize {
        let inner_len = value.raw_encoded_len();
        encoded_len_varint(inner_len as u64) + inner_len
    }

    fn decode_value<B: Buf + ?Sized>(
        value: &mut T,
        mut buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        ctx.limit_reached()?;
        merge(value, buf.take_length_delimited()?, ctx.enter_recursion())
    }
}

impl<T> DistinguishedValueEncoder<T> for General
where
    T: RawDistinguishedMessage + Eq,
{
    fn decode_value_distinguished<B: Buf + ?Sized>(
        value: &mut T,
        mut buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        ctx.limit_reached()?;
        merge_distinguished(value, buf.take_length_delimited()?, ctx.enter_recursion())
    }
}
