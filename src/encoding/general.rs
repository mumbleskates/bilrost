use alloc::borrow::Cow;
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
    delegate_encoding, delegate_value_encoding, encode_varint, encoded_len_varint,
    encoder_where_value_encoder, Canonicity, Capped, DecodeContext, DecodeError,
    DistinguishedValueEncoder, EmptyState, Encoder, Fixed, Map, PlainBytes, TagMeasurer, TagWriter,
    Unpacked, ValueEncoder, Varint, WireType, Wiretyped,
};
use crate::message::{merge, merge_distinguished, RawDistinguishedMessage, RawMessage};
use crate::Blob;
use crate::DecodeErrorKind::InvalidValue;

pub struct General;

encoder_where_value_encoder!(General);

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

// General encodes bool and integers as varints.
delegate_value_encoding!(delegate from (General) to (Varint)
    for type (bool) including distinguished);
delegate_value_encoding!(delegate from (General) to (Varint)
    for type (u16) including distinguished);
delegate_value_encoding!(delegate from (General) to (Varint)
    for type (i16) including distinguished);
delegate_value_encoding!(delegate from (General) to (Varint)
    for type (u32) including distinguished);
delegate_value_encoding!(delegate from (General) to (Varint)
    for type (i32) including distinguished);
delegate_value_encoding!(delegate from (General) to (Varint)
    for type (u64) including distinguished);
delegate_value_encoding!(delegate from (General) to (Varint)
    for type (i64) including distinguished);

// General also encodes floating point values.
delegate_value_encoding!(delegate from (General) to (Fixed) for type (f32));
delegate_value_encoding!(delegate from (General) to (Fixed) for type (f64));

impl EmptyState for String {
    #[inline]
    fn empty() -> Self {
        Self::new()
    }

    #[inline]
    fn is_empty(&self) -> bool {
        Self::is_empty(self)
    }

    #[inline]
    fn clear(&mut self) {
        Self::clear(self)
    }
}

impl Wiretyped<General> for String {
    const WIRE_TYPE: WireType = WireType::LengthDelimited;
}

impl ValueEncoder<General> for String {
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
        struct DropGuard<'a>(&'a mut Vec<u8>);
        impl Drop for DropGuard<'_> {
            #[inline]
            fn drop(&mut self) {
                self.0.clear();
            }
        }

        let source = buf.take_length_delimited()?.take_all();
        // If we must copy, make sure to copy only once.
        value.clear();
        value.reserve(source.remaining());
        unsafe {
            let drop_guard = DropGuard(value.as_mut_vec());
            drop_guard.0.put(source);
            match str::from_utf8(drop_guard.0) {
                Ok(_) => {
                    // Success; do not clear the bytes.
                    mem::forget(drop_guard);
                    Ok(())
                }
                Err(_) => Err(DecodeError::new(InvalidValue)),
            }
        }
    }
}

impl DistinguishedValueEncoder<General> for String {
    fn decode_value_distinguished<B: Buf + ?Sized>(
        value: &mut String,
        buf: Capped<B>,
        allow_empty: bool,
        ctx: DecodeContext,
    ) -> Result<Canonicity, DecodeError> {
        Self::decode_value(value, buf, ctx)?;
        Ok(if !allow_empty && value.is_empty() {
            Canonicity::NotCanonical
        } else {
            Canonicity::Canonical
        })
    }
}

#[cfg(test)]
mod string {
    use super::{General, String};
    use crate::encoding::test::check_type_test;
    check_type_test!(General, expedient, String, WireType::LengthDelimited);
    check_type_test!(General, distinguished, String, WireType::LengthDelimited);
}

impl EmptyState for Cow<'_, str> {
    #[inline]
    fn empty() -> Self {
        Self::default()
    }

    #[inline]
    fn is_empty(&self) -> bool {
        str::is_empty(self)
    }

    #[inline]
    fn clear(&mut self) {
        match self {
            Cow::Borrowed(_) => {
                *self = Cow::default();
            }
            Cow::Owned(owned) => {
                owned.clear();
            }
        }
    }
}

impl Wiretyped<General> for Cow<'_, str> {
    const WIRE_TYPE: WireType = WireType::LengthDelimited;
}

impl ValueEncoder<General> for Cow<'_, str> {
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
        ValueEncoder::<General>::decode_value(value.to_mut(), buf, ctx)
    }
}

impl DistinguishedValueEncoder<General> for Cow<'_, str> {
    fn decode_value_distinguished<B: Buf + ?Sized>(
        value: &mut Cow<str>,
        buf: Capped<B>,
        allow_empty: bool,
        ctx: DecodeContext,
    ) -> Result<Canonicity, DecodeError> {
        DistinguishedValueEncoder::<General>::decode_value_distinguished(
            value.to_mut(),
            buf,
            allow_empty,
            ctx,
        )
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
impl EmptyState for bytestring::ByteString {
    #[inline]
    fn empty() -> Self {
        Self::new()
    }

    #[inline]
    fn is_empty(&self) -> bool {
        str::is_empty(self)
    }

    #[inline]
    fn clear(&mut self) {
        *self = Self::empty();
    }
}

#[cfg(feature = "bytestring")]
impl Wiretyped<General> for bytestring::ByteString {
    const WIRE_TYPE: WireType = WireType::LengthDelimited;
}

#[cfg(feature = "bytestring")]
impl ValueEncoder<General> for bytestring::ByteString {
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
impl DistinguishedValueEncoder<General> for bytestring::ByteString {
    fn decode_value_distinguished<B: Buf + ?Sized>(
        value: &mut bytestring::ByteString,
        buf: Capped<B>,
        allow_empty: bool,
        ctx: DecodeContext,
    ) -> Result<Canonicity, DecodeError> {
        Self::decode_value(value, buf, ctx)?;
        Ok(if !allow_empty && value.is_empty() {
            Canonicity::NotCanonical
        } else {
            Canonicity::Canonical
        })
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

impl EmptyState for Bytes {
    #[inline]
    fn empty() -> Self {
        Self::new()
    }

    #[inline]
    fn is_empty(&self) -> bool {
        Self::is_empty(self)
    }

    #[inline]
    fn clear(&mut self) {
        *self = Self::empty();
    }
}

impl Wiretyped<General> for Bytes {
    const WIRE_TYPE: WireType = WireType::LengthDelimited;
}

impl ValueEncoder<General> for Bytes {
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

impl DistinguishedValueEncoder<General> for Bytes {
    fn decode_value_distinguished<B: Buf + ?Sized>(
        value: &mut Bytes,
        buf: Capped<B>,
        allow_empty: bool,
        ctx: DecodeContext,
    ) -> Result<Canonicity, DecodeError> {
        Self::decode_value(value, buf, ctx)?;
        Ok(if !allow_empty && value.is_empty() {
            Canonicity::NotCanonical
        } else {
            Canonicity::Canonical
        })
    }
}

#[cfg(test)]
mod bytes_blob {
    use super::{Bytes, General, Vec};
    use crate::encoding::test::check_type_test;
    check_type_test!(General, expedient, from Vec<u8>, into Bytes, WireType::LengthDelimited);
    check_type_test!(General, distinguished, from Vec<u8>, into Bytes, WireType::LengthDelimited);
}

impl Wiretyped<General> for Blob {
    const WIRE_TYPE: WireType = WireType::LengthDelimited;
}

impl ValueEncoder<General> for Blob {
    #[inline]
    fn encode_value<B: BufMut + ?Sized>(value: &Blob, buf: &mut B) {
        ValueEncoder::<PlainBytes>::encode_value(&**value, buf)
    }

    #[inline]
    fn value_encoded_len(value: &Blob) -> usize {
        ValueEncoder::<PlainBytes>::value_encoded_len(&**value)
    }

    #[inline]
    fn decode_value<B: Buf + ?Sized>(
        value: &mut Blob,
        buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        ValueEncoder::<PlainBytes>::decode_value(&mut **value, buf, ctx)
    }
}

impl DistinguishedValueEncoder<General> for Blob {
    #[inline]
    fn decode_value_distinguished<B: Buf + ?Sized>(
        value: &mut Blob,
        buf: Capped<B>,
        allow_empty: bool,
        ctx: DecodeContext,
    ) -> Result<Canonicity, DecodeError> {
        DistinguishedValueEncoder::<PlainBytes>::decode_value_distinguished(
            &mut **value,
            buf,
            allow_empty,
            ctx,
        )
    }
}

#[cfg(test)]
mod blob {
    use super::{Blob, General};
    use crate::encoding::test::check_type_test;
    check_type_test!(General, expedient, Blob, WireType::LengthDelimited);
    check_type_test!(General, distinguished, Blob, WireType::LengthDelimited);
}

impl<T> Wiretyped<General> for T
where
    T: RawMessage,
{
    const WIRE_TYPE: WireType = WireType::LengthDelimited;
}

impl<T> ValueEncoder<General> for T
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

impl<T> DistinguishedValueEncoder<General> for T
where
    T: RawDistinguishedMessage + Eq,
{
    fn decode_value_distinguished<B: Buf + ?Sized>(
        value: &mut T,
        mut buf: Capped<B>,
        allow_empty: bool,
        ctx: DecodeContext,
    ) -> Result<Canonicity, DecodeError> {
        ctx.limit_reached()?;
        let buf = buf.take_length_delimited()?;
        // Empty message types always encode and decode from zero bytes. It is far cheaper to check
        // here than to check after the value has been decoded and checking the message's
        // `is_empty()`.
        if !allow_empty && buf.remaining_before_cap() == 0 {
            return Ok(Canonicity::NotCanonical);
        }
        merge_distinguished(value, buf, ctx.enter_recursion())
    }
}
