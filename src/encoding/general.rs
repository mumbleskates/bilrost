use alloc::borrow::Cow;
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::String;
use alloc::vec::Vec;
#[cfg(feature = "std")]
use core::hash::Hash;
use core::str;
#[cfg(feature = "std")]
use std::collections::{HashMap, HashSet};

use bytes::{Buf, BufMut, Bytes};

use crate::encoding::{
    delegate_encoding, delegate_value_encoding, encode_varint, encoded_len_varint, Capped,
    DecodeContext, DistinguishedEncoder, DistinguishedFieldEncoder, DistinguishedValueEncoder,
    Encoder, EqualDefaultAlwaysEmpty, FieldEncoder, HasEmptyState, Map, PlainBytes, TagMeasurer,
    TagWriter, Unpacked, ValueEncoder, WireType, Wiretyped,
};
use crate::message::{merge, merge_distinguished, RawDistinguishedMessage, RawMessage};
use crate::DecodeErrorKind::{InvalidValue, NotCanonical, UnexpectedlyRepeated};
use crate::{Blob, DecodeError};

pub struct General;

// General implements unpacked encodings by default, but only for select collection types. Other
// implementers of the `Collection` trait must use Unpacked or Packed.
delegate_encoding!(delegate from (General) to (Unpacked<General>)
    for type (Vec<T>) including distinguished with generics (T));
delegate_encoding!(delegate from (General) to (Unpacked<General>)
    for type (Cow<'a, [T]>) including distinguished
    with where clause (T: Clone)
    with generics ('a, T));
#[cfg(feature = "smallvec")]
delegate_encoding!(delegate from (General) to (Unpacked<General>)
    for type (smallvec::SmallVec<A>) including distinguished
    with where clause (A: smallvec::Array<Item = T>)
    with generics (T, A));
#[cfg(feature = "thin-vec")]
delegate_encoding!(delegate from (General) to (Unpacked<General>)
    for type (thin_vec::ThinVec<T>) including distinguished with generics (T));
#[cfg(feature = "tinyvec")]
delegate_encoding!(delegate from (General) to (Unpacked<General>)
    for type (tinyvec::TinyVec<A>) including distinguished
    with where clause (A: tinyvec::Array<Item = T>)
    with generics (T, A));
delegate_encoding!(delegate from (General) to (Unpacked<General>)
    for type (BTreeSet<T>) including distinguished with generics (T));
delegate_value_encoding!(delegate from (General) to (Map<General, General>)
    for type (BTreeMap<K, V>) including distinguished
    with where clause for expedient (K: Ord)
    with where clause for distinguished (V: Eq)
    with generics (K, V));
#[cfg(feature = "std")]
delegate_encoding!(delegate from (General) to (Unpacked<General>)
    for type (HashSet<T>) with generics (T));
#[cfg(feature = "std")]
delegate_value_encoding!(delegate from (General) to (Map<General, General>)
    for type (HashMap<K, V>)
    with where clause (K: Eq + Hash)
    with generics (K, V));
#[cfg(feature = "hashbrown")]
delegate_encoding!(delegate from (General) to (Unpacked<General>)
    for type (hashbrown::HashSet<T>) with generics (T));
#[cfg(feature = "hashbrown")]
delegate_value_encoding!(delegate from (General) to (Map<General, General>)
    for type (hashbrown::HashMap<K, V>)
    with where clause (K: Eq + Hash)
    with generics (K, V));

/// General encodes plain values only when they are non-default.
impl<T> Encoder<T> for General
where
    General: ValueEncoder<T>,
    T: HasEmptyState,
{
    #[inline]
    fn encode<B: BufMut + ?Sized>(tag: u32, value: &T, buf: &mut B, tw: &mut TagWriter) {
        if !value.is_empty() {
            Self::encode_field(tag, value, buf, tw);
        }
    }

    #[inline]
    fn encoded_len(tag: u32, value: &T, tm: &mut TagMeasurer) -> usize {
        if !value.is_empty() {
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
            return Err(DecodeError::new(UnexpectedlyRepeated));
        }
        Self::decode_field(wire_type, value, buf, ctx)
    }
}

/// General's distinguished encoding for plain values forbids encoding defaulted values. This
/// includes directly-nested message types, which are not emitted when all their fields are default.
impl<T> DistinguishedEncoder<T> for General
where
    General: DistinguishedValueEncoder<T> + Encoder<T>,
    T: Eq + HasEmptyState,
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
            return Err(DecodeError::new(UnexpectedlyRepeated));
        }
        Self::decode_field_distinguished(wire_type, value, buf, ctx)?;
        if value.is_empty() {
            return Err(DecodeError::new(NotCanonical));
        }
        Ok(())
    }
}

// General encodes bool and integers as varints.
delegate_value_encoding!(delegate from (General) to (crate::encoding::Varint)
    for type (bool) including distinguished);
delegate_value_encoding!(delegate from (General) to (crate::encoding::Varint)
    for type (u8) including distinguished);
delegate_value_encoding!(delegate from (General) to (crate::encoding::Varint)
    for type (i8) including distinguished);
delegate_value_encoding!(delegate from (General) to (crate::encoding::Varint)
    for type (u16) including distinguished);
delegate_value_encoding!(delegate from (General) to (crate::encoding::Varint)
    for type (i16) including distinguished);
delegate_value_encoding!(delegate from (General) to (crate::encoding::Varint)
    for type (u32) including distinguished);
delegate_value_encoding!(delegate from (General) to (crate::encoding::Varint)
    for type (i32) including distinguished);
delegate_value_encoding!(delegate from (General) to (crate::encoding::Varint)
    for type (u64) including distinguished);
delegate_value_encoding!(delegate from (General) to (crate::encoding::Varint)
    for type (i64) including distinguished);

// General also encodes floating point values.
delegate_value_encoding!(delegate from (General) to (crate::encoding::Fixed) for type (f32));
delegate_value_encoding!(delegate from (General) to (crate::encoding::Fixed) for type (f64));

impl EqualDefaultAlwaysEmpty for String {}

impl Wiretyped<String> for General {
    const WIRE_TYPE: WireType = WireType::LengthDelimited;
}

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
        let mut string_data = Vec::<u8>::new();
        string_data.put(buf.take_length_delimited()?.take_all());
        *value = String::from_utf8(string_data).map_err(|_| DecodeError::new(InvalidValue))?;
        Ok(())
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
    use super::{General, String};
    use crate::encoding::test::check_type_test;
    check_type_test!(General, expedient, String, WireType::LengthDelimited);
    check_type_test!(General, distinguished, String, WireType::LengthDelimited);
}

impl EqualDefaultAlwaysEmpty for Cow<'_, str> {}

impl Wiretyped<Cow<'_, str>> for General {
    const WIRE_TYPE: WireType = WireType::LengthDelimited;
}

impl ValueEncoder<Cow<'_, str>> for General {
    fn encode_value<B: BufMut + ?Sized>(value: &Cow<str>, buf: &mut B) {
        encode_varint(value.len() as u64, buf);
        buf.put_slice(value.as_bytes());
    }

    fn value_encoded_len(value: &Cow<str>) -> usize {
        encoded_len_varint(value.len() as u64) + value.len()
    }

    fn decode_value<B: Buf + ?Sized>(
        value: &mut Cow<str>,
        buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        Self::decode_value(value.to_mut(), buf, ctx)
    }
}

impl DistinguishedValueEncoder<Cow<'_, str>> for General {
    fn decode_value_distinguished<B: Buf + ?Sized>(
        value: &mut Cow<str>,
        buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        Self::decode_value(value, buf, ctx)
    }
}

#[cfg(test)]
mod cow_string {
    use super::{Cow, General};
    use crate::encoding::test::check_type_test;
    check_type_test!(General, expedient, Cow<str>, WireType::LengthDelimited);
    check_type_test!(General, distinguished, Cow<str>, WireType::LengthDelimited);
}

#[cfg(feature = "bytestring")]
impl EqualDefaultAlwaysEmpty for bytestring::ByteString {}

#[cfg(feature = "bytestring")]
impl Wiretyped<bytestring::ByteString> for General {
    const WIRE_TYPE: WireType = WireType::LengthDelimited;
}

#[cfg(feature = "bytestring")]
impl ValueEncoder<bytestring::ByteString> for General {
    fn encode_value<B: BufMut + ?Sized>(value: &bytestring::ByteString, buf: &mut B) {
        encode_varint(value.len() as u64, buf);
        buf.put_slice(value.as_bytes());
    }

    fn value_encoded_len(value: &bytestring::ByteString) -> usize {
        encoded_len_varint(value.len() as u64) + value.len()
    }

    fn decode_value<B: Buf + ?Sized>(
        value: &mut bytestring::ByteString,
        mut buf: Capped<B>,
        _ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        let mut string_data = buf.take_length_delimited()?;
        let string_len = string_data.remaining_before_cap();
        *value = bytestring::ByteString::try_from(string_data.copy_to_bytes(string_len))
            .map_err(|_| DecodeError::new(InvalidValue))?;
        Ok(())
    }
}

#[cfg(feature = "bytestring")]
impl DistinguishedValueEncoder<bytestring::ByteString> for General {
    fn decode_value_distinguished<B: Buf + ?Sized>(
        value: &mut bytestring::ByteString,
        buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        Self::decode_value(value, buf, ctx)
    }
}

#[cfg(feature = "bytestring")]
#[cfg(test)]
mod bytestring_string {
    use super::{General, String};
    use crate::encoding::test::check_type_test;
    check_type_test!(General, expedient, from String, into bytestring::ByteString, WireType::LengthDelimited);
    check_type_test!(General, distinguished, from String, into bytestring::ByteString,
        WireType::LengthDelimited);
}

impl EqualDefaultAlwaysEmpty for Bytes {}

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
    use super::{Bytes, General, Vec};
    use crate::encoding::test::check_type_test;
    check_type_test!(General, expedient, from Vec<u8>, into Bytes, WireType::LengthDelimited);
    check_type_test!(General, distinguished, from Vec<u8>, into Bytes, WireType::LengthDelimited);
}

impl EqualDefaultAlwaysEmpty for Blob {}

impl Wiretyped<Blob> for General {
    const WIRE_TYPE: WireType = WireType::LengthDelimited;
}

impl ValueEncoder<Blob> for General {
    #[inline]
    fn encode_value<B: BufMut + ?Sized>(value: &Blob, buf: &mut B) {
        PlainBytes::encode_value(&**value, buf)
    }

    #[inline]
    fn value_encoded_len(value: &Blob) -> usize {
        PlainBytes::value_encoded_len(&**value)
    }

    #[inline]
    fn decode_value<B: Buf + ?Sized>(
        value: &mut Blob,
        buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        PlainBytes::decode_value(&mut **value, buf, ctx)
    }
}

impl DistinguishedValueEncoder<Blob> for General {
    #[inline]
    fn decode_value_distinguished<B: Buf + ?Sized>(
        value: &mut Blob,
        buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        PlainBytes::decode_value_distinguished(&mut **value, buf, ctx)
    }
}

#[cfg(test)]
mod blob {
    use super::{Blob, General};
    use crate::encoding::test::check_type_test;
    check_type_test!(General, expedient, Blob, WireType::LengthDelimited);
    check_type_test!(General, distinguished, Blob, WireType::LengthDelimited);
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
