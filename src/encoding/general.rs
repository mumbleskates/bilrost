use crate::buf::ReverseBuf;
use crate::encoding::message::{
    borrow_merge, borrow_merge_distinguished, merge, merge_distinguished,
    RawDistinguishedMessageDecoder, RawMessage,
};
use crate::encoding::proxy::SealedBilrostTag;
use crate::encoding::{
    delegate_encoding, delegate_value_encoding, encode_varint, encoded_len_varint,
    encoding_implemented_via_value_encoding, impl_cow_value_encoding, prepend_varint, Canonicity,
    Capped, DecodeContext, DecodeError, DistinguishedProxiable, DistinguishedValueBorrowDecoder,
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

#[repr(u8)]
pub enum GeneralEncodingContext {
    PreferPacked,
    PreferUnpacked,
}
use GeneralEncodingContext::{PreferPacked, PreferUnpacked};

/// The generic `General` struct is parametrized by its location, whether it's in a message or a
/// oneof. Different defaults make sense in different contexts; `General<PreferUnpacked>` becomes
/// the `general` encoding in message attributes and is the implicit default for non-annotated
/// fields in Messages, and `General<PreferPacked>` likewise becomes the `general_in_oneof` encoding
/// and is the implicit default for variants with no annotated encoding in `Oneof` enums, as well as
/// the default for fields nested inside already packed fields.
///
/// These are also available as the type aliases `GeneralInMessage`, `GeneralInOneof`, and
/// `GeneralInsidePacked`; `General` is still public to allow for generic implementations that are
/// the same when packedness does not matter, which is most of the time.
pub struct General<const Gctx: u8>;
pub type GeneralInMessage = General<{ PreferUnpacked as u8 }>;
pub type GeneralInOneof = General<{ PreferPacked as u8 }>;
pub type GeneralInsidePacked = General<{ PreferPacked as u8 }>;

encoding_implemented_via_value_encoding!(General<Gctx>, with generics (const Gctx: u8));

// `general` and `general_in_oneof` delegate to the `unpacked` and `packed` encodings respectively
// by default, but only for select collection types. Other implementers of the `Collection` trait
// must choose an encoding explicitly.
delegate_encoding!(
    delegate from (GeneralInMessage) to (Unpacked<GeneralInMessage>)
    for type (Vec<T>) including distinguished
    with generics (T)
);
delegate_encoding!(
    delegate from (GeneralInMessage) to (Unpacked<GeneralInMessage>)
    for type (Cow<'a, [T]>) including distinguished
    with where clause (T: Clone)
    with generics ('a, T)
);
delegate_encoding!(
    delegate from (GeneralInMessage) to (Unpacked<GeneralInMessage>)
    for type (BTreeSet<T>) including distinguished
    with generics (T)
);

delegate_encoding!(
    delegate from (GeneralInOneof) to (Packed<GeneralInsidePacked>)
    for type (Vec<T>) including distinguished
    with generics (T)
);
delegate_encoding!(
    delegate from (GeneralInOneof) to (Packed<GeneralInsidePacked>)
    for type (Cow<'a, [T]>) including distinguished
    with where clause (T: Clone)
    with generics ('a, T)
);
delegate_encoding!(
    delegate from (GeneralInOneof) to (Packed<GeneralInsidePacked>)
    for type (BTreeSet<T>) including distinguished
    with generics (T)
);

delegate_value_encoding!(
    delegate from (General<Gctx>)
    to (Map<GeneralInsidePacked, GeneralInsidePacked>)
    for type (BTreeMap<K, V>)
    including distinguished
    with where clause for relaxed (K: Ord)
    with where clause for distinguished (V: Eq)
    with generics (const Gctx: u8, K, V)
);

// General encodes bool and integers as varints.
delegate_value_encoding!(delegate from (General<Gctx>) to (Varint)
    for type (bool) including distinguished with generics (const Gctx: u8));
delegate_value_encoding!(delegate from (General<Gctx>) to (Varint)
    for type (u16) including distinguished with generics (const Gctx: u8));
delegate_value_encoding!(delegate from (General<Gctx>) to (Varint)
    for type (i16) including distinguished with generics (const Gctx: u8));
delegate_value_encoding!(delegate from (General<Gctx>) to (Varint)
    for type (u32) including distinguished with generics (const Gctx: u8));
delegate_value_encoding!(delegate from (General<Gctx>) to (Varint)
    for type (i32) including distinguished with generics (const Gctx: u8));
delegate_value_encoding!(delegate from (General<Gctx>) to (Varint)
    for type (u64) including distinguished with generics (const Gctx: u8));
delegate_value_encoding!(delegate from (General<Gctx>) to (Varint)
    for type (i64) including distinguished with generics (const Gctx: u8));
delegate_value_encoding!(delegate from (General<Gctx>) to (Varint)
    for type (usize) including distinguished with generics (const Gctx: u8));
delegate_value_encoding!(delegate from (General<Gctx>) to (Varint)
    for type (isize) including distinguished with generics (const Gctx: u8));

// General also encodes floating point values.
delegate_value_encoding!(delegate from (General<Gctx>) to (Fixed)
    for type (f32) with generics (const Gctx: u8));
delegate_value_encoding!(delegate from (General<Gctx>) to (Fixed)
    for type (f64) with generics (const Gctx: u8));

impl<const Gctx: u8> Wiretyped<General<Gctx>> for &str {
    const WIRE_TYPE: WireType = WireType::LengthDelimited;
}

impl<const Gctx: u8> ValueEncoder<General<Gctx>> for &str {
    #[inline]
    fn encode_value<B: BufMut + ?Sized>(value: &&str, buf: &mut B) {
        ValueEncoder::<PlainBytes>::encode_value(&value.as_bytes(), buf)
    }

    #[inline]
    fn prepend_value<B: ReverseBuf + ?Sized>(value: &&str, buf: &mut B) {
        ValueEncoder::<PlainBytes>::prepend_value(&value.as_bytes(), buf)
    }

    #[inline]
    fn value_encoded_len(value: &&str) -> usize {
        ValueEncoder::<PlainBytes>::value_encoded_len(&value.as_bytes())
    }
}

impl<'a, const Gctx: u8> ValueBorrowDecoder<'a, General<Gctx>> for &'a str {
    #[inline]
    fn borrow_decode_value(
        value: &mut Self,
        mut buf: Capped<&'a [u8]>,
        _ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        *value = str::from_utf8(buf.take_borrowed_length_delimited()?).map_err(|_| InvalidValue)?;
        Ok(())
    }
}

impl<'a, const Gctx: u8> DistinguishedValueBorrowDecoder<'a, General<Gctx>> for &'a str {
    const CHECKS_EMPTY: bool = false;

    #[inline]
    fn borrow_decode_value_distinguished<const ALLOW_EMPTY: bool>(
        value: &mut Self,
        buf: Capped<&'a [u8]>,
        ctx: RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError> {
        ValueBorrowDecoder::<General<Gctx>>::borrow_decode_value(value, buf, ctx.into_inner())?;
        Ok(Canonicity::Canonical)
    }
}

#[cfg(test)]
mod ref_str {
    crate::encoding::test::check_borrowable!(
        borrowed: str, encoding: crate::encoding::GeneralInMessage);
}

impl<const Gctx: u8> Wiretyped<General<Gctx>> for String {
    const WIRE_TYPE: WireType = WireType::LengthDelimited;
}

impl<const Gctx: u8> ValueEncoder<General<Gctx>> for String {
    #[inline]
    fn encode_value<B: BufMut + ?Sized>(value: &String, buf: &mut B) {
        ValueEncoder::<PlainBytes>::encode_value(&value.as_bytes(), buf)
    }

    #[inline]
    fn prepend_value<B: ReverseBuf + ?Sized>(value: &String, buf: &mut B) {
        ValueEncoder::<PlainBytes>::prepend_value(&value.as_bytes(), buf)
    }

    #[inline]
    fn value_encoded_len(value: &String) -> usize {
        ValueEncoder::<PlainBytes>::value_encoded_len(&value.as_bytes())
    }
}

impl<const Gctx: u8> ValueDecoder<General<Gctx>> for String {
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

impl<const Gctx: u8> DistinguishedValueDecoder<General<Gctx>> for String {
    const CHECKS_EMPTY: bool = false;

    #[inline]
    fn decode_value_distinguished<const ALLOW_EMPTY: bool>(
        value: &mut String,
        buf: Capped<impl Buf + ?Sized>,
        ctx: RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError> {
        ValueDecoder::<General<Gctx>>::decode_value(value, buf, ctx.into_inner())?;
        Ok(Canonicity::Canonical)
    }
}

delegate_value_encoding!(
    encoding (General<Gctx>)
    borrows type (String) as owned
    including distinguished
    with generics (const Gctx: u8)
);

#[cfg(test)]
mod string {
    use super::{GeneralInMessage, String};
    use crate::encoding::test::check_type_test;
    check_type_test!(GeneralInMessage, relaxed, String, WireType::LengthDelimited);
    check_type_test!(
        GeneralInMessage,
        distinguished,
        String,
        WireType::LengthDelimited
    );
}

impl_cow_value_encoding!(
    borrowed str,
    owned String,
    encoding General<Gctx>,
    with generic (const Gctx: u8)
);

#[cfg(test)]
mod cow_string {
    use super::{Cow, GeneralInMessage};
    use crate::encoding::test::check_type_test;
    check_type_test!(
        GeneralInMessage,
        relaxed,
        Cow<str>,
        WireType::LengthDelimited
    );
    check_type_test!(
        GeneralInMessage,
        distinguished,
        Cow<str>,
        WireType::LengthDelimited
    );
}

impl<const Gctx: u8> Wiretyped<General<Gctx>> for Bytes {
    const WIRE_TYPE: WireType = WireType::LengthDelimited;
}

impl<const Gctx: u8> ValueEncoder<General<Gctx>> for Bytes {
    #[inline]
    fn encode_value<B: BufMut + ?Sized>(value: &Bytes, buf: &mut B) {
        ValueEncoder::<PlainBytes>::encode_value(&&**value, buf)
    }

    #[inline]
    fn prepend_value<B: ReverseBuf + ?Sized>(value: &Bytes, buf: &mut B) {
        ValueEncoder::<PlainBytes>::prepend_value(&&**value, buf)
    }

    #[inline]
    fn value_encoded_len(value: &Bytes) -> usize {
        ValueEncoder::<PlainBytes>::value_encoded_len(&&**value)
    }
}

impl<const Gctx: u8> ValueDecoder<General<Gctx>> for Bytes {
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

impl<const Gctx: u8> DistinguishedValueDecoder<General<Gctx>> for Bytes {
    const CHECKS_EMPTY: bool = false;

    #[inline]
    fn decode_value_distinguished<const ALLOW_EMPTY: bool>(
        value: &mut Bytes,
        buf: Capped<impl Buf + ?Sized>,
        ctx: RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError> {
        ValueDecoder::<General<Gctx>>::decode_value(value, buf, ctx.into_inner())?;
        Ok(Canonicity::Canonical)
    }
}

delegate_value_encoding!(
    encoding (General<Gctx>) borrows type (Bytes) as owned
    including distinguished with generics (const Gctx: u8)
);

#[cfg(test)]
mod bytes_blob {
    use super::{Bytes, GeneralInMessage, Vec};
    use crate::encoding::test::check_type_test;
    check_type_test!(GeneralInMessage, relaxed, from Vec<u8>, into Bytes,
        WireType::LengthDelimited);
    check_type_test!(GeneralInMessage, distinguished, from Vec<u8>, into Bytes,
        WireType::LengthDelimited);
}

impl<const Gctx: u8> Wiretyped<General<Gctx>> for Blob {
    const WIRE_TYPE: WireType = WireType::LengthDelimited;
}

impl<const Gctx: u8> ValueEncoder<General<Gctx>> for Blob {
    #[inline]
    fn encode_value<B: BufMut + ?Sized>(value: &Blob, buf: &mut B) {
        ValueEncoder::<PlainBytes>::encode_value(&value.as_slice(), buf)
    }

    #[inline]
    fn prepend_value<B: ReverseBuf + ?Sized>(value: &Blob, buf: &mut B) {
        ValueEncoder::<PlainBytes>::prepend_value(&value.as_slice(), buf)
    }

    #[inline]
    fn value_encoded_len(value: &Blob) -> usize {
        ValueEncoder::<PlainBytes>::value_encoded_len(&value.as_slice())
    }
}

impl<const Gctx: u8> ValueDecoder<General<Gctx>> for Blob {
    #[inline]
    fn decode_value<B: Buf + ?Sized>(
        value: &mut Blob,
        buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        ValueDecoder::<PlainBytes>::decode_value(&mut **value, buf, ctx)
    }
}

impl<const Gctx: u8> DistinguishedValueDecoder<General<Gctx>> for Blob {
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

delegate_value_encoding!(
    encoding (General<Gctx>) borrows type (Blob) as owned
    including distinguished with generics (const Gctx: u8)
);

#[cfg(test)]
mod blob {
    use super::{Blob, GeneralInMessage};
    use crate::encoding::test::check_type_test;
    check_type_test!(GeneralInMessage, relaxed, Blob, WireType::LengthDelimited);
    check_type_test!(
        GeneralInMessage,
        distinguished,
        Blob,
        WireType::LengthDelimited
    );
}

impl Proxiable<SealedBilrostTag> for core::time::Duration {
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

impl DistinguishedProxiable<SealedBilrostTag> for core::time::Duration {
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

delegate_value_encoding!(
    delegate from (General<Gctx>) to (Proxied<Packed<Varint>, SealedBilrostTag>)
    for type (core::time::Duration) including distinguished with generics (const Gctx: u8));

#[cfg(test)]
mod core_time {
    use super::*;
    use crate::encoding::test::{check_type_empty, check_type_test};

    check_type_empty!(core::time::Duration, via proxy with tag SealedBilrostTag);
    check_type_test!(
        GeneralInMessage,
        relaxed,
        core::time::Duration,
        WireType::LengthDelimited
    );
    check_type_empty!(core::time::Duration, via distinguished proxy with tag SealedBilrostTag);
    check_type_test!(
        GeneralInMessage,
        distinguished,
        core::time::Duration,
        WireType::LengthDelimited
    );
}

impl<const Gctx: u8, T> Wiretyped<General<Gctx>> for T
where
    T: RawMessage,
{
    const WIRE_TYPE: WireType = WireType::LengthDelimited;
}

impl<const Gctx: u8, T> ValueEncoder<General<Gctx>> for T
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

impl<const Gctx: u8, T> ValueDecoder<General<Gctx>> for T
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

impl<const Gctx: u8, T> DistinguishedValueDecoder<General<Gctx>> for T
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

impl<'a, const Gctx: u8, T> ValueBorrowDecoder<'a, General<Gctx>> for T
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

impl<'a, const Gctx: u8, T> DistinguishedValueBorrowDecoder<'a, General<Gctx>> for T
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
