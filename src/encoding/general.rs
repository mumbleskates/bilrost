use crate::buf::ReverseBuf;
use crate::encoding::message::{RawDistinguishedMessageDecoder, RawMessage};
use crate::encoding::proxy::SealedBilrostTag;
use crate::encoding::{
    delegate_encoding, delegate_proxied_encoding, delegate_value_encoding,
    encoding_implemented_via_value_encoding, encoding_uses_base_empty_state,
    impl_cow_value_encoding, Canonicity, Capped, DecodeContext, DecodeError,
    DistinguishedProxiable, DistinguishedValueBorrowDecoder, DistinguishedValueDecoder, EmptyState,
    Fixed, Map, MessageEncoding, Packed, PlainBytes, Proxiable,
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

const PREFER_UNPACKED: u8 = 0;
const PREFER_PACKED: u8 = 1;

/// The `GeneralGeneric` struct is used for general encoding, and is the default with some
/// contextual differences; it is parametrized differently depending on whether it's in a message or a
/// oneof. Different defaults make sense in different contexts; `General<PreferUnpacked>` becomes
/// the `general` encoding in message attributes and is the implicit default for non-annotated
/// fields in Messages, and `General<PreferPacked>` likewise becomes the `general_packed` encoding
/// and is the implicit default for variants with no annotated encoding in `Oneof` enums, as well as
/// the default for fields nested inside fields that are already collections.
///
/// These are also available as the type aliases `General` and `GeneralPacked`; `GeneralGeneric` is
/// still public to allow for generic implementations that are the same when packedness does not
/// matter, which is most of the time.
pub struct GeneralGeneric<const P: u8>;
pub type General = GeneralGeneric<PREFER_UNPACKED>;
pub type GeneralPacked = GeneralGeneric<PREFER_PACKED>;

encoding_uses_base_empty_state!(GeneralGeneric<P>, with generics (const P: u8));
encoding_implemented_via_value_encoding!(GeneralGeneric<P>, with generics (const P: u8));

// `general` and `general_in_oneof` delegate to the `unpacked` and `packed` encodings respectively
// by default, but only for select collection types. Other implementers of the `Collection` trait
// must choose an encoding explicitly.
delegate_encoding!(
    delegate from (General) to (Unpacked)
    for type (Vec<T>) including distinguished
    with generics (T)
);
delegate_encoding!(
    delegate from (General) to (Unpacked)
    for type (Cow<'a, [T]>) including distinguished
    with where clause (T: Clone)
    with generics ('a, T)
);
delegate_encoding!(
    delegate from (General) to (Unpacked)
    for type (BTreeSet<T>) including distinguished
    with generics (T)
);

delegate_value_encoding!(
    delegate from (GeneralPacked) to (Packed)
    for type (Vec<T>) including distinguished
    with generics (T)
);
delegate_value_encoding!(
    delegate from (GeneralPacked) to (Packed)
    for type (Cow<'a, [T]>) including distinguished
    with where clause for relaxed (T: Clone)
    with generics ('a, T: 'a)
);
delegate_value_encoding!(
    delegate from (GeneralPacked) to (Packed)
    for type (BTreeSet<T>) including distinguished
    with generics (T)
);

delegate_value_encoding!(
    delegate from (GeneralGeneric<P>) to (Map)
    for type (BTreeMap<K, V>) including distinguished
    with where clause for relaxed (K: Ord)
    with where clause for distinguished (V: Eq)
    with generics (const P: u8, K, V)
);

// General encodings encode Range and RangeInclusive with the General encoding; to use a different
// encoding for the T value, encode ranges as tuples directly
delegate_value_encoding!(
    delegate from (GeneralGeneric<P>) to ((General, General))
    for type (core::ops::Range<T>) including distinguished
    with generics (const P: u8, T)
);
delegate_value_encoding!(
    delegate from (GeneralGeneric<P>) to ((General, General))
    for type (core::ops::RangeInclusive<T>) including distinguished
    with generics (const P: u8, T)
);

// General encodes bool and integers as varints.
delegate_value_encoding!(delegate from (GeneralGeneric<P>) to (Varint)
    for type (bool) including distinguished with generics (const P: u8));
delegate_value_encoding!(delegate from (GeneralGeneric<P>) to (Varint)
    for type (u16) including distinguished with generics (const P: u8));
delegate_value_encoding!(delegate from (GeneralGeneric<P>) to (Varint)
    for type (i16) including distinguished with generics (const P: u8));
delegate_value_encoding!(delegate from (GeneralGeneric<P>) to (Varint)
    for type (u32) including distinguished with generics (const P: u8));
delegate_value_encoding!(delegate from (GeneralGeneric<P>) to (Varint)
    for type (i32) including distinguished with generics (const P: u8));
delegate_value_encoding!(delegate from (GeneralGeneric<P>) to (Varint)
    for type (u64) including distinguished with generics (const P: u8));
delegate_value_encoding!(delegate from (GeneralGeneric<P>) to (Varint)
    for type (i64) including distinguished with generics (const P: u8));
delegate_value_encoding!(delegate from (GeneralGeneric<P>) to (Varint)
    for type (usize) including distinguished with generics (const P: u8));
delegate_value_encoding!(delegate from (GeneralGeneric<P>) to (Varint)
    for type (isize) including distinguished with generics (const P: u8));

delegate_value_encoding!(delegate from (GeneralGeneric<P>) to (Varint)
    for type (core::num::NonZeroU8) including distinguished with generics (const P: u8));
delegate_value_encoding!(delegate from (GeneralGeneric<P>) to (Varint)
    for type (core::num::NonZeroI8) including distinguished with generics (const P: u8));
delegate_value_encoding!(delegate from (GeneralGeneric<P>) to (Varint)
    for type (core::num::NonZeroU16) including distinguished with generics (const P: u8));
delegate_value_encoding!(delegate from (GeneralGeneric<P>) to (Varint)
    for type (core::num::NonZeroI16) including distinguished with generics (const P: u8));
delegate_value_encoding!(delegate from (GeneralGeneric<P>) to (Varint)
    for type (core::num::NonZeroU32) including distinguished with generics (const P: u8));
delegate_value_encoding!(delegate from (GeneralGeneric<P>) to (Varint)
    for type (core::num::NonZeroI32) including distinguished with generics (const P: u8));
delegate_value_encoding!(delegate from (GeneralGeneric<P>) to (Varint)
    for type (core::num::NonZeroU64) including distinguished with generics (const P: u8));
delegate_value_encoding!(delegate from (GeneralGeneric<P>) to (Varint)
    for type (core::num::NonZeroI64) including distinguished with generics (const P: u8));
delegate_value_encoding!(delegate from (GeneralGeneric<P>) to (Varint)
    for type (core::num::NonZeroUsize) including distinguished with generics (const P: u8));
delegate_value_encoding!(delegate from (GeneralGeneric<P>) to (Varint)
    for type (core::num::NonZeroIsize) including distinguished with generics (const P: u8));

// General also encodes floating point values.
delegate_value_encoding!(delegate from (GeneralGeneric<P>) to (Fixed)
    for type (f32) with generics (const P: u8));
delegate_value_encoding!(delegate from (GeneralGeneric<P>) to (Fixed)
    for type (f64) with generics (const P: u8));

impl<const P: u8> Wiretyped<GeneralGeneric<P>, &str> for () {
    const WIRE_TYPE: WireType = WireType::LengthDelimited;
}

impl<const P: u8> ValueEncoder<GeneralGeneric<P>, &str> for () {
    #[inline]
    fn encode_value<B: BufMut + ?Sized>(value: &&str, buf: &mut B) {
        <() as ValueEncoder<PlainBytes, _>>::encode_value(&value.as_bytes(), buf)
    }

    #[inline]
    fn prepend_value<B: ReverseBuf + ?Sized>(value: &&str, buf: &mut B) {
        <() as ValueEncoder<PlainBytes, _>>::prepend_value(&value.as_bytes(), buf)
    }

    #[inline]
    fn value_encoded_len(value: &&str) -> usize {
        <() as ValueEncoder<PlainBytes, _>>::value_encoded_len(&value.as_bytes())
    }
}

impl<'a, const P: u8> ValueBorrowDecoder<'a, GeneralGeneric<P>, &'a str> for () {
    #[inline]
    fn borrow_decode_value(
        value: &mut &'a str,
        mut buf: Capped<&'a [u8]>,
        _ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        *value = str::from_utf8(buf.take_borrowed_length_delimited()?).map_err(|_| InvalidValue)?;
        Ok(())
    }
}

impl<'a, const P: u8> DistinguishedValueBorrowDecoder<'a, GeneralGeneric<P>, &'a str> for () {
    const CHECKS_EMPTY: bool = false;

    #[inline]
    fn borrow_decode_value_distinguished<const ALLOW_EMPTY: bool>(
        value: &mut &'a str,
        buf: Capped<&'a [u8]>,
        ctx: RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError> {
        <() as ValueBorrowDecoder<GeneralGeneric<P>, _>>::borrow_decode_value(
            value,
            buf,
            ctx.into_inner(),
        )?;
        Ok(Canonicity::Canonical)
    }
}

#[cfg(test)]
mod ref_str {
    crate::encoding::test::check_borrowable!(
        borrowed: str, encoding: crate::encoding::General);
}

impl<const P: u8> Wiretyped<GeneralGeneric<P>, String> for () {
    const WIRE_TYPE: WireType = WireType::LengthDelimited;
}

impl<const P: u8> ValueEncoder<GeneralGeneric<P>, String> for () {
    #[inline]
    fn encode_value<B: BufMut + ?Sized>(value: &String, buf: &mut B) {
        <() as ValueEncoder<PlainBytes, _>>::encode_value(&value.as_bytes(), buf)
    }

    #[inline]
    fn prepend_value<B: ReverseBuf + ?Sized>(value: &String, buf: &mut B) {
        <() as ValueEncoder<PlainBytes, _>>::prepend_value(&value.as_bytes(), buf)
    }

    #[inline]
    fn value_encoded_len(value: &String) -> usize {
        <() as ValueEncoder<PlainBytes, _>>::value_encoded_len(&value.as_bytes())
    }
}

impl<const P: u8> ValueDecoder<GeneralGeneric<P>, String> for () {
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

impl<const P: u8> DistinguishedValueDecoder<GeneralGeneric<P>, String> for () {
    const CHECKS_EMPTY: bool = false;

    #[inline]
    fn decode_value_distinguished<const ALLOW_EMPTY: bool>(
        value: &mut String,
        buf: Capped<impl Buf + ?Sized>,
        ctx: RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError> {
        <() as ValueDecoder<GeneralGeneric<P>, _>>::decode_value(value, buf, ctx.into_inner())?;
        Ok(Canonicity::Canonical)
    }
}

delegate_value_encoding!(
    encoding (GeneralGeneric<P>)
    borrows type (String) as owned
    including distinguished
    with generics (const P: u8)
);

#[cfg(test)]
mod string {
    use super::{General, String};
    use crate::encoding::test::check_type_test;
    check_type_test!(General, relaxed, String, WireType::LengthDelimited);
    check_type_test!(General, distinguished, String, WireType::LengthDelimited);
}

impl_cow_value_encoding!(
    borrowed str,
    owned String,
    encoding GeneralGeneric<P>,
    with generic (const P: u8)
);

#[cfg(test)]
mod cow_string {
    use super::{Cow, General};
    use crate::encoding::test::check_type_test;
    check_type_test!(General, relaxed, Cow<str>, WireType::LengthDelimited);
    check_type_test!(General, distinguished, Cow<str>, WireType::LengthDelimited);
}

impl<const P: u8> Wiretyped<GeneralGeneric<P>, Bytes> for () {
    const WIRE_TYPE: WireType = WireType::LengthDelimited;
}

impl<const P: u8> ValueEncoder<GeneralGeneric<P>, Bytes> for () {
    #[inline]
    fn encode_value<B: BufMut + ?Sized>(value: &Bytes, buf: &mut B) {
        <() as ValueEncoder<PlainBytes, _>>::encode_value(&&**value, buf)
    }

    #[inline]
    fn prepend_value<B: ReverseBuf + ?Sized>(value: &Bytes, buf: &mut B) {
        <() as ValueEncoder<PlainBytes, _>>::prepend_value(&&**value, buf)
    }

    #[inline]
    fn value_encoded_len(value: &Bytes) -> usize {
        <() as ValueEncoder<PlainBytes, _>>::value_encoded_len(&&**value)
    }
}

impl<const P: u8> ValueDecoder<GeneralGeneric<P>, Bytes> for () {
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

impl<const P: u8> DistinguishedValueDecoder<GeneralGeneric<P>, Bytes> for () {
    const CHECKS_EMPTY: bool = false;

    #[inline]
    fn decode_value_distinguished<const ALLOW_EMPTY: bool>(
        value: &mut Bytes,
        buf: Capped<impl Buf + ?Sized>,
        ctx: RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError> {
        <() as ValueDecoder<GeneralGeneric<P>, _>>::decode_value(value, buf, ctx.into_inner())?;
        Ok(Canonicity::Canonical)
    }
}

delegate_value_encoding!(
    encoding (GeneralGeneric<P>) borrows type (Bytes) as owned
    including distinguished with generics (const P: u8)
);

#[cfg(test)]
mod bytes_blob {
    use super::{Bytes, General, Vec};
    use crate::encoding::test::check_type_test;
    check_type_test!(General, relaxed, from Vec<u8>, into Bytes,
        WireType::LengthDelimited);
    check_type_test!(General, distinguished, from Vec<u8>, into Bytes,
        WireType::LengthDelimited);
}

impl<const P: u8> Wiretyped<GeneralGeneric<P>, Blob> for () {
    const WIRE_TYPE: WireType = WireType::LengthDelimited;
}

impl<const P: u8> ValueEncoder<GeneralGeneric<P>, Blob> for () {
    #[inline]
    fn encode_value<B: BufMut + ?Sized>(value: &Blob, buf: &mut B) {
        <() as ValueEncoder<PlainBytes, _>>::encode_value(&value.as_slice(), buf)
    }

    #[inline]
    fn prepend_value<B: ReverseBuf + ?Sized>(value: &Blob, buf: &mut B) {
        <() as ValueEncoder<PlainBytes, _>>::prepend_value(&value.as_slice(), buf)
    }

    #[inline]
    fn value_encoded_len(value: &Blob) -> usize {
        <() as ValueEncoder<PlainBytes, _>>::value_encoded_len(&value.as_slice())
    }
}

impl<const P: u8> ValueDecoder<GeneralGeneric<P>, Blob> for () {
    #[inline]
    fn decode_value<B: Buf + ?Sized>(
        value: &mut Blob,
        buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        <() as ValueDecoder<PlainBytes, _>>::decode_value(&mut **value, buf, ctx)
    }
}

impl<const P: u8> DistinguishedValueDecoder<GeneralGeneric<P>, Blob> for () {
    const CHECKS_EMPTY: bool = <() as DistinguishedValueDecoder<PlainBytes, Vec<u8>>>::CHECKS_EMPTY;

    #[inline]
    fn decode_value_distinguished<const ALLOW_EMPTY: bool>(
        value: &mut Blob,
        buf: Capped<impl Buf + ?Sized>,
        ctx: RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError> {
        <() as DistinguishedValueDecoder<PlainBytes, _>>::decode_value_distinguished::<ALLOW_EMPTY>(
            &mut **value,
            buf,
            ctx,
        )
    }
}

delegate_value_encoding!(
    encoding (GeneralGeneric<P>) borrows type (Blob) as owned
    including distinguished with generics (const P: u8)
);

#[cfg(test)]
mod blob {
    use super::{Blob, General};
    use crate::encoding::test::check_type_test;
    check_type_test!(General, relaxed, Blob, WireType::LengthDelimited);
    check_type_test!(General, distinguished, Blob, WireType::LengthDelimited);
}

impl Proxiable<SealedBilrostTag> for core::time::Duration {
    type Proxy = crate::encoding::local_proxy::LocalProxy<u64, 2>;

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

delegate_proxied_encoding!(
    use encoding (Packed<Varint>) to encode proxied type (core::time::Duration)
    using proxy tag (SealedBilrostTag)
    with general encodings including distinguished
);

#[cfg(test)]
mod core_time {
    use super::*;
    use crate::encoding::test::{check_type_empty, check_type_test};

    check_type_empty!(core::time::Duration, via proxy with tag SealedBilrostTag);
    check_type_test!(
        General,
        relaxed,
        core::time::Duration,
        WireType::LengthDelimited
    );
    check_type_empty!(core::time::Duration, via distinguished proxy with tag SealedBilrostTag);
    check_type_test!(
        General,
        distinguished,
        core::time::Duration,
        WireType::LengthDelimited
    );
}

mod delegate_to_message_encoding {
    use super::*;

    impl<const P: u8, T> Wiretyped<GeneralGeneric<P>, T> for ()
    where
        T: RawMessage,
        (): EmptyState<(), T>,
    {
        const WIRE_TYPE: WireType = <() as Wiretyped<MessageEncoding, T>>::WIRE_TYPE;
    }

    impl<const P: u8, T> ValueEncoder<GeneralGeneric<P>, T> for ()
    where
        T: RawMessage,
        (): EmptyState<(), T>,
    {
        #[inline(always)]
        fn encode_value<B: BufMut + ?Sized>(value: &T, buf: &mut B) {
            <() as ValueEncoder<MessageEncoding, _>>::encode_value(value, buf)
        }

        #[inline(always)]
        fn prepend_value<B: ReverseBuf + ?Sized>(value: &T, buf: &mut B) {
            <() as ValueEncoder<MessageEncoding, _>>::prepend_value(value, buf)
        }

        #[inline(always)]
        fn value_encoded_len(value: &T) -> usize {
            <() as ValueEncoder<MessageEncoding, _>>::value_encoded_len(value)
        }
    }

    impl<const P: u8, T> ValueDecoder<GeneralGeneric<P>, T> for ()
    where
        T: RawMessageDecoder,
        (): EmptyState<(), T>,
    {
        #[inline(always)]
        fn decode_value<B: Buf + ?Sized>(
            value: &mut T,
            buf: Capped<B>,
            ctx: DecodeContext,
        ) -> Result<(), DecodeError> {
            <() as ValueDecoder<MessageEncoding, _>>::decode_value(value, buf, ctx)
        }
    }

    impl<const P: u8, T> DistinguishedValueDecoder<GeneralGeneric<P>, T> for ()
    where
        T: RawDistinguishedMessageDecoder + Eq,
        (): EmptyState<(), T>,
    {
        const CHECKS_EMPTY: bool =
            <() as DistinguishedValueDecoder<MessageEncoding, T>>::CHECKS_EMPTY;

        #[inline(always)]
        fn decode_value_distinguished<const ALLOW_EMPTY: bool>(
            value: &mut T,
            buf: Capped<impl Buf + ?Sized>,
            ctx: RestrictedDecodeContext,
        ) -> Result<Canonicity, DecodeError> {
            <() as DistinguishedValueDecoder<MessageEncoding, _>>::decode_value_distinguished::<
                ALLOW_EMPTY,
            >(value, buf, ctx)
        }
    }

    impl<'a, const P: u8, T> ValueBorrowDecoder<'a, GeneralGeneric<P>, T> for ()
    where
        T: RawMessageBorrowDecoder<'a>,
        (): EmptyState<(), T>,
    {
        #[inline(always)]
        fn borrow_decode_value(
            value: &mut T,
            buf: Capped<&'a [u8]>,
            ctx: DecodeContext,
        ) -> Result<(), DecodeError> {
            <() as ValueBorrowDecoder<MessageEncoding, _>>::borrow_decode_value(value, buf, ctx)
        }
    }

    impl<'a, const P: u8, T> DistinguishedValueBorrowDecoder<'a, GeneralGeneric<P>, T> for ()
    where
        T: RawDistinguishedMessageBorrowDecoder<'a> + Eq,
        (): EmptyState<(), T>,
    {
        const CHECKS_EMPTY: bool =
            <() as DistinguishedValueBorrowDecoder<MessageEncoding, T>>::CHECKS_EMPTY;

        #[inline(always)]
        fn borrow_decode_value_distinguished<const ALLOW_EMPTY: bool>(
            value: &mut T,
            buf: Capped<&'a [u8]>,
            ctx: RestrictedDecodeContext,
        ) -> Result<Canonicity, DecodeError> {
            <() as DistinguishedValueBorrowDecoder<MessageEncoding, _>>::borrow_decode_value_distinguished::<
                ALLOW_EMPTY,
            >(value, buf, ctx)
        }
    }
}
