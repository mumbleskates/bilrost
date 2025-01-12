use crate::buf::ReverseBuf;
use crate::encoding::message::{
    borrow_merge, borrow_merge_distinguished, merge, merge_distinguished,
    RawDistinguishedMessageDecoder, RawMessage,
};
use crate::encoding::{
    delegate_encoding, delegate_value_encoding, encode_varint, encoded_len_varint,
    encoding_implemented_via_value_encoding, prepend_varint, Canonicity, Capped, DecodeContext,
    DecodeError, DistinguishedProxiable, DistinguishedValueBorrowDecoder,
    DistinguishedValueDecoder, Fixed, Map, Packed, PlainBytes, Proxiable, Proxied,
    RawDistinguishedMessageBorrowDecoder, RawMessageBorrowDecoder, RawMessageDecoder,
    RestrictedDecodeContext, Unpacked, ValueBorrowDecoder, ValueDecoder, ValueEncoder, Varint,
    WireType, Wiretyped,
};
use crate::DecodeErrorKind::InvalidValue;
use crate::{Blob, DecodeErrorKind};
use alloc::borrow::Cow;
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::String;
use alloc::vec::Vec;
use bytes::{Buf, BufMut, Bytes};
use core::mem;
use core::str;

pub struct General;

encoding_implemented_via_value_encoding!(General);

// General implements unpacked encodings by default, but only for select collection types. Other
// implementers of the `Collection` trait must use Unpacked or Packed.
delegate_encoding!(delegate from (General) to (Unpacked<General>)
    for type (Vec<T>) including distinguished with generics (T));
delegate_encoding!(delegate from (General) to (Unpacked<General>)
    for type (Cow<'a, [T]>) including distinguished
    with where clause (T: Clone)
    with generics ('a, T));
delegate_encoding!(delegate from (General) to (Unpacked<General>)
    for type (BTreeSet<T>) including distinguished with generics (T));
delegate_value_encoding!(delegate from (General) to (Map<General, General>)
    for type (BTreeMap<K, V>) including distinguished
    with where clause for relaxed (K: Ord)
    with where clause for distinguished (V: Eq)
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
delegate_value_encoding!(delegate from (General) to (Varint)
    for type (usize) including distinguished);
delegate_value_encoding!(delegate from (General) to (Varint)
    for type (isize) including distinguished);

// General also encodes floating point values.
delegate_value_encoding!(delegate from (General) to (Fixed) for type (f32));
delegate_value_encoding!(delegate from (General) to (Fixed) for type (f64));

impl Wiretyped<General> for String {
    const WIRE_TYPE: WireType = WireType::LengthDelimited;
}

impl ValueEncoder<General> for String {
    #[inline]
    fn encode_value<B: BufMut + ?Sized>(value: &String, buf: &mut B) {
        encode_varint(value.len() as u64, buf);
        buf.put_slice(value.as_bytes());
    }

    #[inline]
    fn prepend_value<B: ReverseBuf + ?Sized>(value: &String, buf: &mut B) {
        buf.prepend_slice(value.as_bytes());
        prepend_varint(value.len() as u64, buf);
    }

    #[inline]
    fn value_encoded_len(value: &String) -> usize {
        encoded_len_varint(value.len() as u64) + value.len()
    }
}

impl ValueDecoder<General> for String {
    #[inline]
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

impl DistinguishedValueDecoder<General> for String {
    const CHECKS_EMPTY: bool = false;

    #[inline]
    fn decode_value_distinguished<const ALLOW_EMPTY: bool>(
        value: &mut String,
        buf: Capped<impl Buf + ?Sized>,
        ctx: RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError> {
        Self::decode_value(value, buf, ctx.into_inner())?;
        Ok(Canonicity::Canonical)
    }
}

delegate_value_encoding!(encoding (General) borrows type (String) as owned including distinguished);

#[cfg(test)]
mod string {
    use super::{General, String};
    use crate::encoding::test::check_type_test;
    check_type_test!(General, relaxed, String, WireType::LengthDelimited);
    check_type_test!(General, distinguished, String, WireType::LengthDelimited);
}

impl Wiretyped<General> for Cow<'_, str> {
    const WIRE_TYPE: WireType = WireType::LengthDelimited;
}

impl ValueEncoder<General> for Cow<'_, str> {
    #[inline]
    fn encode_value<B: BufMut + ?Sized>(value: &Cow<str>, buf: &mut B) {
        encode_varint(value.len() as u64, buf);
        buf.put_slice(value.as_bytes());
    }

    #[inline]
    fn prepend_value<B: ReverseBuf + ?Sized>(value: &Cow<str>, buf: &mut B) {
        buf.prepend_slice(value.as_bytes());
        prepend_varint(value.len() as u64, buf);
    }

    #[inline]
    fn value_encoded_len(value: &Cow<str>) -> usize {
        encoded_len_varint(value.len() as u64) + value.len()
    }
}

impl ValueDecoder<General> for Cow<'_, str> {
    #[inline]
    fn decode_value<B: Buf + ?Sized>(
        value: &mut Cow<str>,
        buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        ValueDecoder::<General>::decode_value(value.to_mut(), buf, ctx)
    }
}

impl DistinguishedValueDecoder<General> for Cow<'_, str> {
    const CHECKS_EMPTY: bool = <String as DistinguishedValueDecoder<General>>::CHECKS_EMPTY;

    #[inline]
    fn decode_value_distinguished<const ALLOW_EMPTY: bool>(
        value: &mut Cow<str>,
        buf: Capped<impl Buf + ?Sized>,
        ctx: RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError> {
        DistinguishedValueDecoder::<General>::decode_value_distinguished::<ALLOW_EMPTY>(
            value.to_mut(),
            buf,
            ctx,
        )
    }
}

// TODO(widders): borrow cow

#[cfg(test)]
mod cow_string {
    use super::{Cow, General};
    use crate::encoding::test::check_type_test;
    check_type_test!(General, relaxed, Cow<str>, WireType::LengthDelimited);
    check_type_test!(General, distinguished, Cow<str>, WireType::LengthDelimited);
}

impl Wiretyped<General> for Bytes {
    const WIRE_TYPE: WireType = WireType::LengthDelimited;
}

impl ValueEncoder<General> for Bytes {
    #[inline]
    fn encode_value<B: BufMut + ?Sized>(value: &Bytes, mut buf: &mut B) {
        encode_varint(value.len() as u64, buf);
        (&mut buf).put(value.clone()); // `put` needs Self to be sized, so we use the ref type
    }

    #[inline]
    fn prepend_value<B: ReverseBuf + ?Sized>(value: &Bytes, buf: &mut B) {
        buf.prepend_slice(value);
        prepend_varint(value.len() as u64, buf);
    }

    #[inline]
    fn value_encoded_len(value: &Bytes) -> usize {
        encoded_len_varint(value.len() as u64) + value.len()
    }
}

impl ValueDecoder<General> for Bytes {
    #[inline]
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

impl DistinguishedValueDecoder<General> for Bytes {
    const CHECKS_EMPTY: bool = false;

    #[inline]
    fn decode_value_distinguished<const ALLOW_EMPTY: bool>(
        value: &mut Bytes,
        buf: Capped<impl Buf + ?Sized>,
        ctx: RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError> {
        Self::decode_value(value, buf, ctx.into_inner())?;
        Ok(Canonicity::Canonical)
    }
}

delegate_value_encoding!(encoding (General) borrows type (Bytes) as owned including distinguished);

#[cfg(test)]
mod bytes_blob {
    use super::{Bytes, General, Vec};
    use crate::encoding::test::check_type_test;
    check_type_test!(General, relaxed, from Vec<u8>, into Bytes, WireType::LengthDelimited);
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
    fn prepend_value<B: ReverseBuf + ?Sized>(value: &Blob, buf: &mut B) {
        buf.prepend_slice(value);
        prepend_varint(value.len() as u64, buf);
    }

    #[inline]
    fn value_encoded_len(value: &Blob) -> usize {
        ValueEncoder::<PlainBytes>::value_encoded_len(&**value)
    }
}

impl ValueDecoder<General> for Blob {
    #[inline]
    fn decode_value<B: Buf + ?Sized>(
        value: &mut Blob,
        buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        ValueDecoder::<PlainBytes>::decode_value(&mut **value, buf, ctx)
    }
}

impl DistinguishedValueDecoder<General> for Blob {
    const CHECKS_EMPTY: bool = <Vec<u8> as DistinguishedValueDecoder<PlainBytes>>::CHECKS_EMPTY;

    #[inline]
    fn decode_value_distinguished<const ALLOW_EMPTY: bool>(
        value: &mut Blob,
        buf: Capped<impl Buf + ?Sized>,
        ctx: RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError> {
        DistinguishedValueDecoder::<PlainBytes>::decode_value_distinguished::<ALLOW_EMPTY>(
            &mut **value,
            buf,
            ctx,
        )
    }
}

delegate_value_encoding!(encoding (General) borrows type (Blob) as owned including distinguished);

#[cfg(test)]
mod blob {
    use super::{Blob, General};
    use crate::encoding::test::check_type_test;
    check_type_test!(General, relaxed, Blob, WireType::LengthDelimited);
    check_type_test!(General, distinguished, Blob, WireType::LengthDelimited);
}

impl Proxiable for core::time::Duration {
    type Proxy = crate::encoding::local_proxy::LocalProxy<u64, 2>;

    fn new_proxy() -> Self::Proxy {
        Self::Proxy::new_empty()
    }

    fn encode_proxy(&self) -> Self::Proxy {
        Self::Proxy::new_without_empty_suffix([self.as_secs(), self.subsec_nanos() as u64])
    }

    fn decode_proxy(&mut self, proxy: Self::Proxy) -> Result<(), DecodeErrorKind> {
        let [secs, nanos @ 0..=999_999_999] = proxy.into_inner() else {
            return Err(InvalidValue);
        };
        *self = core::time::Duration::new(secs, nanos as u32);
        Ok(())
    }
}

impl DistinguishedProxiable for core::time::Duration {
    fn decode_proxy_distinguished(
        &mut self,
        proxy: Self::Proxy,
    ) -> Result<Canonicity, DecodeErrorKind> {
        let ([secs, nanos @ 0..=999_999_999], canon) = proxy.into_inner_distinguished() else {
            return Err(InvalidValue);
        };
        *self = core::time::Duration::new(secs, nanos as u32);
        Ok(canon)
    }
}

delegate_value_encoding!(delegate from (General) to (Proxied<Packed<Varint>>)
    for type (core::time::Duration) including distinguished);

#[cfg(test)]
mod core_time {
    use super::*;
    use crate::encoding::test::{check_type_empty, check_type_test};

    check_type_empty!(core::time::Duration, via proxy);
    check_type_test!(
        General,
        relaxed,
        core::time::Duration,
        WireType::LengthDelimited
    );
    check_type_empty!(core::time::Duration, via distinguished proxy);
    check_type_test!(
        General,
        distinguished,
        core::time::Duration,
        WireType::LengthDelimited
    );
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
    #[inline]
    fn encode_value<B: BufMut + ?Sized>(value: &T, buf: &mut B) {
        encode_varint(value.raw_encoded_len() as u64, buf);
        value.raw_encode(buf);
    }

    #[inline]
    fn prepend_value<B: ReverseBuf + ?Sized>(value: &T, buf: &mut B) {
        let end = buf.remaining();
        value.raw_prepend(buf);
        prepend_varint((buf.remaining() - end) as u64, buf);
    }

    #[inline]
    fn value_encoded_len(value: &T) -> usize {
        let inner_len = value.raw_encoded_len();
        encoded_len_varint(inner_len as u64) + inner_len
    }
}

impl<T> ValueDecoder<General> for T
where
    T: RawMessageDecoder,
{
    #[inline]
    fn decode_value<B: Buf + ?Sized>(
        value: &mut T,
        mut buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        ctx.limit_reached()?;
        merge(value, buf.take_length_delimited()?, ctx.enter_recursion())
    }
}

impl<T> DistinguishedValueDecoder<General> for T
where
    T: RawDistinguishedMessageDecoder + Eq,
{
    const CHECKS_EMPTY: bool = true; // Empty messages are always zero-length

    #[inline]
    fn decode_value_distinguished<const ALLOW_EMPTY: bool>(
        value: &mut T,
        mut buf: Capped<impl Buf + ?Sized>,
        ctx: RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError> {
        ctx.limit_reached()?;
        let buf = buf.take_length_delimited()?;
        // Empty message types always encode and decode from zero bytes. It is far cheaper to check
        // here than to check after the value has been decoded and checking the message's
        // `is_empty()`.
        if !ALLOW_EMPTY && buf.remaining_before_cap() == 0 {
            return ctx.check(Canonicity::NotCanonical);
        }
        merge_distinguished(value, buf, ctx.enter_recursion())
    }
}

impl<'a, T> ValueBorrowDecoder<'a, General> for T
where
    T: RawMessageBorrowDecoder<'a>,
{
    #[inline]
    fn borrow_decode_value(
        value: &mut T,
        mut buf: Capped<&'a [u8]>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        ctx.limit_reached()?;
        borrow_merge(value, buf.take_length_delimited()?, ctx.enter_recursion())
    }
}

impl<'a, T> DistinguishedValueBorrowDecoder<'a, General> for T
where
    T: RawDistinguishedMessageBorrowDecoder<'a> + Eq,
{
    const CHECKS_EMPTY: bool = true; // Empty messages are always zero-length

    #[inline]
    fn borrow_decode_value_distinguished<const ALLOW_EMPTY: bool>(
        value: &mut T,
        mut buf: Capped<&'a [u8]>,
        ctx: RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError> {
        ctx.limit_reached()?;
        let buf = buf.take_length_delimited()?;
        // Empty message types always encode and decode from zero bytes. It is far cheaper to check
        // here than to check after the value has been decoded and checking the message's
        // `is_empty()`.
        if !ALLOW_EMPTY && buf.remaining_before_cap() == 0 {
            return ctx.check(Canonicity::NotCanonical);
        }
        borrow_merge_distinguished(value, buf, ctx.enter_recursion())
    }
}
