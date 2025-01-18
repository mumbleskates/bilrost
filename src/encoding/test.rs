use crate::encoding::{
    const_varint, decode_varint, decode_varint_slow, encode_varint, encoded_len_varint, Capped,
    DecodeContext, Decoder, DistinguishedDecoder, DistinguishedProxiable,
    DistinguishedValueDecoder, EmptyState, Encoder, Fixed, ForOverwrite, General, Map, Packed,
    PlainBytes, Proxiable, RestrictedDecodeContext, RuntimeTagMeasurer, TagReader, TagRevWriter,
    TagWriter, ValueDecoder, ValueEncoder, Varint, WireType,
};
use crate::DecodeErrorKind::{
    InvalidVarint, OutOfDomainValue, TagOverflowed, Truncated, WrongWireType,
};
use crate::{Blob, Canonicity, DecodeError};
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use bytes::{Buf, BufMut};
use core::fmt::Debug;
use proptest::{prelude::*, test_runner::TestCaseResult};

// TODO(widders): this needs to have borrowed decoding coverage
/// Generalized proptest macro. Kind must be either `relaxed` or `distinguished`.
macro_rules! check_type_test {
    ($encoder:ty, $kind:ident, $ty:ty, $wire_type:expr) => {
        crate::encoding::test::check_type_test!($encoder, $kind, from $ty, into $ty,
        converter(value) { value }, $wire_type);
    };
    ($encoder:ty, $kind:ident, from $from_ty:ty, into $into_ty:ty, $wire_type:expr) => {
        crate::encoding::test::check_type_test!($encoder, $kind, from $from_ty, into $into_ty,
            converter(value) { <$into_ty>::from(value) }, $wire_type);
    };
    (
        $encoder:ty,
        $kind:ident,
        from $from_ty:ty,
        into $into_ty:ty,
        converter($from_value:ident) $convert:expr,
        $wire_type:expr
    ) => {
        #[cfg(test)]
        mod $kind {
            use proptest::prelude::*;

            use crate::encoding::test::$kind::check_type;
            #[allow(unused_imports)]
            use crate::encoding::WireType;
            #[allow(unused_imports)]
            use super::*;

            proptest! {
                #[test]
                fn check($from_value: $from_ty, tag: u32) {
                    check_type::<$into_ty, $encoder>($convert, tag, $wire_type)?;
                }
                #[test]
                fn check_optional(opt_value: Option<$from_ty>, tag: u32) {
                    check_type::<Option<$into_ty>, $encoder>(
                        opt_value.map(|$from_value| $convert),
                        tag,
                        $wire_type,
                    )?;
                }
            }
        }
    };
}
pub(crate) use check_type_test;

macro_rules! check_borrowable {
    (borrowed: $ty:ty, encoding: $encoding:ty $(,)?) => {
        crate::encoding::test::check_borrowable!(
            mod borrow_equivalence,
            borrowed: $ty,
            encoding: $encoding,
        );
    };
    (mod $mod_name:ident, borrowed: $ty:ty, encoding: $encoding:ty $(,)?) => {
        mod $mod_name {
            #[allow(unused_imports)]
            use super::*;
            use crate::encoding::{
                Capped, DistinguishedValueBorrowDecoder, EmptyState, RestrictedDecodeContext,
                ValueEncoder,
            };
            use crate::Canonicity::Canonical;
            use alloc::vec::Vec;
            use core::borrow::Borrow;
            use proptest::prelude::*;

            proptest! {
                #[test]
                fn check(val: <$ty as alloc::borrow::ToOwned>::Owned) {
                    let mut buf = Vec::new();
                    ValueEncoder::<$encoding>::encode_value(&val, &mut buf);
                    let mut borrowed = <&$ty>::empty();
                    assert_eq!(
                        DistinguishedValueBorrowDecoder::<$encoding>::
                            borrow_decode_value_distinguished::<true>
                        (
                            &mut borrowed,
                            Capped::new(&mut buf.as_slice()),
                            RestrictedDecodeContext::new(Canonical),
                        )?,
                        Canonical,
                    );
                    assert_eq!(
                        borrowed,
                        Borrow::<$ty>::borrow(&val),
                    );
                }
            }
        }
    };
}
pub(crate) use check_borrowable;

fn check_legal_remaining(tag: u32, wire_type: WireType, remaining: usize) -> TestCaseResult {
    match wire_type {
        WireType::SixtyFourBit => 8..=8,
        WireType::ThirtyTwoBit => 4..=4,
        WireType::Varint => 1..=9,
        WireType::LengthDelimited => 1..=usize::MAX,
    }
    .contains(&remaining)
    .then_some(())
    .ok_or_else(|| {
        TestCaseError::fail(format!(
            "{wire_type:?} wire type illegal remaining: {remaining}, tag: {tag}"
        ))
    })
}

macro_rules! check_type {
    ($kind:ident, $decoder_trait:ident, $context:expr, $decode:ident) => {
        pub mod $kind {
            use super::*;
            use crate::buf::ReverseBuffer;

            pub fn check_type<T, E>(value: T, tag: u32, wire_type: WireType) -> TestCaseResult
            where
                T: Debug + ForOverwrite + PartialEq + $decoder_trait<E>,
            {
                let expected_len =
                    <T as Encoder<E>>::encoded_len(tag, &value, &mut RuntimeTagMeasurer::new());

                let mut forward_encoded = Vec::with_capacity(expected_len);
                <T as Encoder<E>>::encode(tag, &value, &mut forward_encoded, &mut TagWriter::new());
                prop_assert_eq!(
                    expected_len,
                    forward_encoded.len(),
                    "forward encoded length was wrong"
                );

                let mut prepend_buf = ReverseBuffer::new();
                let mut trw = TagRevWriter::new();
                <T as Encoder<E>>::prepend_encode(tag, &value, &mut prepend_buf, &mut trw);
                trw.finalize(&mut prepend_buf);
                prop_assert_eq!(
                    expected_len,
                    prepend_buf.len(),
                    "prepend encoded length was wrong"
                );

                let mut prepended = Vec::new();
                prepended.put(prepend_buf);

                if check_type_prepend_must_match_forward::$kind::VALUE {
                    prop_assert_eq!(&forward_encoded, &prepended, "prepend did not match append",);
                }

                if forward_encoded.len() == 0 {
                    // Short circuit for omitted fields, which do not get decoded.
                    return Ok(());
                }

                for encoded in [forward_encoded, prepended] {
                    let mut slice = encoded.as_slice();
                    let mut buf = Capped::new(&mut slice);
                    let mut tr = TagReader::new();

                    let (decoded_tag, decoded_wire_type) = tr
                        .decode_key(buf.lend())
                        .map_err(|error| TestCaseError::fail(error.to_string()))?;
                    prop_assert_eq!(
                        tag,
                        decoded_tag,
                        "decoded tag does not match; expected: {}, actual: {}",
                        tag,
                        decoded_tag
                    );

                    prop_assert_eq!(
                        wire_type,
                        decoded_wire_type,
                        "decoded wire type does not match; expected: {:?}, actual: {:?}",
                        wire_type,
                        decoded_wire_type,
                    );

                    check_legal_remaining(tag, wire_type, buf.remaining())?;

                    let mut roundtrip_value = T::for_overwrite();
                    _ = <T as $decoder_trait<E>>::$decode(
                        wire_type,
                        false,
                        &mut roundtrip_value,
                        buf.lend(),
                        $context,
                    )
                    .map_err(|error| TestCaseError::fail(error.to_string()))?;

                    prop_assert!(
                        !buf.remaining() > 0,
                        "expected buffer to be empty, remaining: {}",
                        buf.remaining()
                    );

                    prop_assert_eq!(&value, &roundtrip_value);
                }

                Ok(())
            }

            pub fn check_type_unpacked<T, E>(
                value: T,
                tag: u32,
                wire_type: WireType,
            ) -> TestCaseResult
            where
                T: Debug + ForOverwrite + PartialEq + $decoder_trait<E>,
            {
                let expected_len =
                    <T as Encoder<E>>::encoded_len(tag, &value, &mut RuntimeTagMeasurer::new());

                let mut forward_encoded = Vec::with_capacity(expected_len);
                <T as Encoder<E>>::encode(tag, &value, &mut forward_encoded, &mut TagWriter::new());

                prop_assert_eq!(
                    expected_len,
                    forward_encoded.len(),
                    "forward encoded length was wrong",
                );

                let mut prepend_buf = ReverseBuffer::new();
                let mut trw = TagRevWriter::new();
                <T as Encoder<E>>::prepend_encode(tag, &value, &mut prepend_buf, &mut trw);
                trw.finalize(&mut prepend_buf);
                prop_assert_eq!(
                    expected_len,
                    prepend_buf.len(),
                    "prepend encoded length was wrong",
                );

                let mut prepended = Vec::new();
                prepended.put(prepend_buf);

                if check_type_prepend_must_match_forward::$kind::VALUE {
                    prop_assert_eq!(&forward_encoded, &prepended, "prepend did not match append",);
                }

                if forward_encoded.len() == 0 {
                    // Short circuit for omitted fields, which do not get decoded.
                    return Ok(());
                }

                for encoded in [forward_encoded, prepended] {
                    let mut slice = encoded.as_slice();
                    let mut buf = Capped::new(&mut slice);
                    let mut tr = TagReader::new();

                    let mut roundtrip_value = T::for_overwrite();
                    let (decoded_tag, decoded_wire_type) = tr
                        .decode_key(buf.lend())
                        .map_err(|error| TestCaseError::fail(error.to_string()))?;

                    prop_assert_eq!(
                        tag,
                        decoded_tag,
                        "decoded tag does not match; expected: {}, actual: {}",
                        tag,
                        decoded_tag
                    );

                    prop_assert_eq!(
                        wire_type,
                        decoded_wire_type,
                        "decoded wire type does not match; expected: {:?}, actual: {:?}",
                        wire_type,
                        decoded_wire_type
                    );

                    _ = <T as $decoder_trait<E>>::$decode(
                        wire_type,
                        false,
                        &mut roundtrip_value,
                        buf.lend(),
                        $context,
                    )
                    .map_err(|error| TestCaseError::fail(error.to_string()))?;

                    prop_assert!(
                        !buf.remaining() > 0,
                        "expected buffer to be empty, remaining: {}",
                        buf.remaining()
                    );
                    prop_assert_eq!(&value, &roundtrip_value);
                }

                Ok(())
            }
        }
    };
}
// Non-distinguished types either contain floating-point numbers (which are prepend-encoded
// trivially similarly to how they are forward-encoded) or hash-based collections and mappings.
// The latter don't have distinct "reversed" iterators, so if they have multiple items they
// won't necessarily prepend-encode the exact same bytes that they forward-encode and we needn't
// assert that they do.
mod check_type_prepend_must_match_forward {
    pub(crate) mod relaxed {
        pub(crate) const VALUE: bool = false;
    }
    pub(crate) mod distinguished {
        pub(crate) const VALUE: bool = true;
    }
}
check_type!(relaxed, Decoder, DecodeContext::default(), decode);
check_type!(
    distinguished,
    DistinguishedDecoder,
    RestrictedDecodeContext::new(Canonicity::Canonical),
    decode_distinguished
);

/// Types that have an empty state should have a consistent implementation; this can be
/// especially important for types that use proxied encoding, which may want to keep emptiness
/// of the original and proxy types in sync. That's what this tests right now, though it may be
/// possible that this could be loosened somewhat in the future. There's some possibility that
/// eventually EmptyState may become subsidiary to value encoding rather than an enforced global
/// semantic, but we don't need to get ahead of ourselves.
macro_rules! check_type_empty {
    ($ty:ty) => {
        #[test]
        fn check_type_empty() {
            $crate::encoding::test::check_type_empty_impl::<$ty>();
        }
    };

    ($ty:ty, via proxy) => {
        #[test]
        fn check_type_empty_via_proxy() {
            $crate::encoding::test::check_type_empty_proxied_impl::<$ty>();
            $crate::encoding::test::check_proxy_round_trip::<$ty>();
        }
    };

    ($ty:ty, via distinguished proxy) => {
        #[test]
        fn check_type_empty_via_distinguished_proxy() {
            $crate::encoding::test::check_type_empty_proxied_impl::<$ty>();
            $crate::encoding::test::check_proxy_round_trip_distinguished::<$ty>();
        }
    };
}
pub(crate) use check_type_empty;

pub(crate) fn check_type_empty_impl<T>()
where
    T: Debug + EmptyState + PartialEq,
{
    let mut empty = T::empty();
    assert!(empty.is_empty());
    empty.clear();
    assert!(empty.is_empty());
    assert_eq!(empty, T::empty());
}

pub(crate) fn check_type_empty_proxied_impl<T>()
where
    T: Debug + EmptyState + PartialEq + Proxiable,
    T::Proxy: Debug + EmptyState + PartialEq,
{
    check_type_empty_impl::<T>();
    check_type_empty_impl::<T::Proxy>();
}

pub(crate) fn check_proxy_round_trip<T>()
where
    T: Debug + EmptyState + PartialEq + Proxiable,
    T::Proxy: Debug + EmptyState + PartialEq,
{
    let start = T::empty();
    let proxy = start.encode_proxy();
    assert!(proxy.is_empty());
    let mut end = T::for_overwrite();
    end.decode_proxy(proxy).unwrap();
    assert!(end.is_empty());
    assert_eq!(start, end);
}

pub(crate) fn check_proxy_round_trip_distinguished<T>()
where
    T: Debug + EmptyState + Eq + DistinguishedProxiable,
    T::Proxy: Debug + EmptyState + Eq,
{
    let start = T::empty();
    let proxy = start.encode_proxy();
    assert!(proxy.is_empty());
    let mut end = T::for_overwrite();
    let canon = end.decode_proxy_distinguished(proxy).unwrap();
    assert_eq!(canon, Canonicity::Canonical);
    assert!(end.is_empty());
    assert_eq!(start, end);
}

fn present_empty_not_canon<T, E>()
where
    T: EmptyState + Eq + DistinguishedDecoder<E> + ValueEncoder<E>,
{
    let mut encoded = <Vec<u8>>::new();
    Encoder::<E>::encode(123, &Some(T::empty()), &mut encoded, &mut TagWriter::new());
    let mut buf = &*encoded;
    let mut capped = Capped::new(&mut buf);
    let (tag, wire_type) = TagReader::new().decode_key(capped.lend()).unwrap();
    assert_eq!(tag, 123);
    let mut decoded = T::for_overwrite();
    assert_eq!(
        DistinguishedDecoder::<E>::decode_distinguished(
            wire_type,
            false,
            &mut decoded,
            capped,
            RestrictedDecodeContext::new(Canonicity::NotCanonical),
        ),
        Ok(Canonicity::NotCanonical)
    );
    assert!(decoded.is_empty());
}

#[test]
fn test_present_and_empty() {
    // Any value that's present not in an `Option` that is not omitted must err when decoded in
    // distinguished mode
    present_empty_not_canon::<u32, General>();
    present_empty_not_canon::<u64, General>();
    present_empty_not_canon::<i32, General>();
    present_empty_not_canon::<i64, General>();
    present_empty_not_canon::<u32, Fixed>();
    present_empty_not_canon::<u64, Fixed>();
    present_empty_not_canon::<i32, Fixed>();
    present_empty_not_canon::<i64, Fixed>();
    present_empty_not_canon::<bool, General>();
    present_empty_not_canon::<String, General>();
    present_empty_not_canon::<Blob, General>();
    present_empty_not_canon::<Vec<u8>, PlainBytes>();

    present_empty_not_canon::<Vec<u32>, Packed<General>>();
    present_empty_not_canon::<Vec<u64>, Packed<General>>();
    present_empty_not_canon::<Vec<i32>, Packed<General>>();
    present_empty_not_canon::<Vec<i64>, Packed<General>>();
    present_empty_not_canon::<Vec<u32>, Packed<Fixed>>();
    present_empty_not_canon::<Vec<u64>, Packed<Fixed>>();
    present_empty_not_canon::<Vec<i32>, Packed<Fixed>>();
    present_empty_not_canon::<Vec<i64>, Packed<Fixed>>();
    present_empty_not_canon::<Vec<bool>, Packed<General>>();
    present_empty_not_canon::<Vec<String>, Packed<General>>();
    present_empty_not_canon::<Vec<Blob>, Packed<General>>();
    present_empty_not_canon::<Vec<Vec<u8>>, Packed<PlainBytes>>();
    present_empty_not_canon::<[u32; 5], Packed<General>>();
    present_empty_not_canon::<[(u32, String); 5], Packed<General>>();

    present_empty_not_canon::<BTreeSet<u32>, Packed<General>>();
    present_empty_not_canon::<BTreeSet<u64>, Packed<General>>();
    present_empty_not_canon::<BTreeSet<i32>, Packed<General>>();
    present_empty_not_canon::<BTreeSet<i64>, Packed<General>>();
    present_empty_not_canon::<BTreeSet<u32>, Packed<Fixed>>();
    present_empty_not_canon::<BTreeSet<u64>, Packed<Fixed>>();
    present_empty_not_canon::<BTreeSet<i32>, Packed<Fixed>>();
    present_empty_not_canon::<BTreeSet<i64>, Packed<Fixed>>();
    present_empty_not_canon::<BTreeSet<bool>, Packed<General>>();
    present_empty_not_canon::<BTreeSet<String>, Packed<General>>();
    present_empty_not_canon::<BTreeSet<Blob>, Packed<General>>();
    present_empty_not_canon::<BTreeSet<Vec<u8>>, Packed<PlainBytes>>();

    present_empty_not_canon::<BTreeMap<u32, u32>, Map<General, General>>();
    present_empty_not_canon::<BTreeMap<u64, u64>, Map<General, General>>();
    present_empty_not_canon::<BTreeMap<i32, i32>, Map<General, General>>();
    present_empty_not_canon::<BTreeMap<i64, i64>, Map<General, General>>();
    present_empty_not_canon::<BTreeMap<u32, u32>, Map<Fixed, Fixed>>();
    present_empty_not_canon::<BTreeMap<u64, u64>, Map<Fixed, Fixed>>();
    present_empty_not_canon::<BTreeMap<i32, i32>, Map<Fixed, Fixed>>();
    present_empty_not_canon::<BTreeMap<i64, i64>, Map<Fixed, Fixed>>();
    present_empty_not_canon::<BTreeMap<bool, bool>, Map<General, General>>();
    present_empty_not_canon::<BTreeMap<String, String>, Map<General, General>>();
    present_empty_not_canon::<BTreeMap<Blob, Blob>, Map<General, General>>();
    present_empty_not_canon::<BTreeMap<Vec<u8>, Vec<u8>>, Map<PlainBytes, PlainBytes>>();

    present_empty_not_canon::<Vec<BTreeMap<u32, u32>>, Packed<Map<General, General>>>();
    present_empty_not_canon::<Vec<BTreeMap<u64, u64>>, Packed<Map<General, General>>>();
    present_empty_not_canon::<Vec<BTreeMap<i32, i32>>, Packed<Map<General, General>>>();
    present_empty_not_canon::<Vec<BTreeMap<i64, i64>>, Packed<Map<General, General>>>();
    present_empty_not_canon::<Vec<BTreeMap<u32, u32>>, Packed<Map<Fixed, Fixed>>>();
    present_empty_not_canon::<Vec<BTreeMap<u64, u64>>, Packed<Map<Fixed, Fixed>>>();
    present_empty_not_canon::<Vec<BTreeMap<i32, i32>>, Packed<Map<Fixed, Fixed>>>();
    present_empty_not_canon::<Vec<BTreeMap<i64, i64>>, Packed<Map<Fixed, Fixed>>>();
    present_empty_not_canon::<Vec<BTreeMap<bool, bool>>, Packed<Map<General, General>>>();
    present_empty_not_canon::<Vec<BTreeMap<String, String>>, Packed<Map<General, General>>>();
    present_empty_not_canon::<Vec<BTreeMap<Blob, Blob>>, Packed<Map<General, General>>>();
    present_empty_not_canon::<Vec<BTreeMap<Vec<u8>, Vec<u8>>>, Packed<Map<PlainBytes, PlainBytes>>>(
    );

    present_empty_not_canon::<(bool,), General>();
    present_empty_not_canon::<(bool, bool), General>();
    present_empty_not_canon::<(bool, bool, bool), General>();
    present_empty_not_canon::<(bool, bool, bool, bool), General>();
    present_empty_not_canon::<(bool, bool, bool, bool, bool), General>();
    present_empty_not_canon::<(bool, bool, bool, bool, bool, bool), General>();
    present_empty_not_canon::<(bool, bool, bool, bool, bool, bool, bool), General>();
    present_empty_not_canon::<(bool, bool, bool, bool, bool, bool, bool, bool), General>();
    present_empty_not_canon::<(bool, bool, bool, bool, bool, bool, bool, bool, bool), General>();
    present_empty_not_canon::<(u16, u16, u16, u16, u16, u16, u16, u16, u16, u16), General>();
    present_empty_not_canon::<(u16, u16, u16, u16, u16, u16, u16, u16, u16, u16, u16), General>();
    present_empty_not_canon::<(u16, u16, u16, u16, u16, u16, u16, u16, u16, u16, u16, u16), General>(
    );
    present_empty_not_canon::<(bool,), General>();
    present_empty_not_canon::<(bool, u32), General>();
    present_empty_not_canon::<(bool, bool, String), General>();
    present_empty_not_canon::<(bool, i64, Blob, bool), General>();
    present_empty_not_canon::<(bool, bool, bool, bool, bool), General>();
    present_empty_not_canon::<(bool, bool, (), bool, bool, bool), General>();
    present_empty_not_canon::<(bool, bool, bytes::Bytes, bool, bool, bool, bool), General>();
    present_empty_not_canon::<(bool, bool, u16, bool, i16, bool, bool, bool), General>();
    present_empty_not_canon::<(bool, String, bool, bool, bool, bool, bool, bool, bool), General>();
    present_empty_not_canon::<(String, u16, u16, u16, u16, u16, i64, u16, u16, u16), General>();
    present_empty_not_canon::<(u16, u16, u16, bool, u16, u16, u16, u16, bool, u16, u16), General>();
    present_empty_not_canon::<
        (
            u16,
            u16,
            u16,
            u16,
            u16,
            bool,
            u16,
            String,
            u64,
            u16,
            u16,
            u16,
        ),
        General,
    >();
}

#[test]
fn unaligned_fixed64_packed() {
    // Construct a length-delineated field that is not a multiple of 8 bytes.
    let mut buf = Vec::<u8>::new();
    encode_varint(12, &mut buf);
    buf.extend([1; 12]);

    let mut parsed = Vec::<u64>::new();
    let res = ValueDecoder::<Packed<Fixed>>::decode_value(
        &mut parsed,
        Capped::new(&mut buf.as_slice()),
        DecodeContext::default(),
    );
    assert_eq!(
        res.expect_err("unaligned packed fixed64 decoded without error")
            .kind(),
        Truncated
    );
    let res = DistinguishedValueDecoder::<Packed<Fixed>>::decode_value_distinguished::<true>(
        &mut parsed,
        Capped::new(&mut buf.as_slice()),
        RestrictedDecodeContext::new(Canonicity::NotCanonical),
    );
    assert_eq!(
        res.expect_err("unaligned packed fixed64 decoded without error")
            .kind(),
        Truncated
    );
}

#[test]
fn unaligned_fixed32_packed() {
    // Construct a length-delineated field that is not a multiple of 4 bytes.
    let mut buf = Vec::<u8>::new();
    encode_varint(17, &mut buf);
    buf.extend([1; 17]);

    let mut parsed = Vec::<u32>::new();
    let res = ValueDecoder::<Packed<Fixed>>::decode_value(
        &mut parsed,
        Capped::new(&mut buf.as_slice()),
        DecodeContext::default(),
    );
    assert_eq!(
        res.expect_err("unaligned packed fixed32 decoded without error")
            .kind(),
        Truncated
    );
    let res = DistinguishedValueDecoder::<Packed<Fixed>>::decode_value_distinguished::<true>(
        &mut parsed,
        Capped::new(&mut buf.as_slice()),
        RestrictedDecodeContext::new(Canonicity::NotCanonical),
    );
    assert_eq!(
        res.expect_err("unaligned packed fixed32 decoded without error")
            .kind(),
        Truncated
    );
}

#[test]
fn unaligned_map_packed() {
    // Construct a length-delineated field that is not a multiple of the sum of fixed size key
    // and value in a map. In the case we are testing it is a fixed size 4+8 = 12 bytes per
    // entry.
    let mut buf = Vec::<u8>::new();
    encode_varint(16, &mut buf);
    buf.extend([1; 16]);

    // The entries for this map always consume 12 bytes each.
    let mut parsed = BTreeMap::<u32, u64>::new();
    let res = ValueDecoder::<Map<Fixed, Fixed>>::decode_value(
        &mut parsed,
        Capped::new(&mut buf.as_slice()),
        DecodeContext::default(),
    );
    assert_eq!(
        res.expect_err("unaligned 12-byte map decoded without error")
            .kind(),
        Truncated
    );
    let res = DistinguishedValueDecoder::<Map<Fixed, Fixed>>::decode_value_distinguished::<true>(
        &mut parsed,
        Capped::new(&mut buf.as_slice()),
        RestrictedDecodeContext::new(Canonicity::NotCanonical),
    );
    assert_eq!(
        res.expect_err("unaligned 12-byte map decoded without error")
            .kind(),
        Truncated
    );
}

#[test]
fn string_merge_invalid_utf8() {
    let mut s = String::new();
    let buf = b"\x02\x80\x80";

    let r = ValueDecoder::<General>::decode_value(
        &mut s,
        Capped::new(&mut buf.as_slice()),
        DecodeContext::default(),
    );
    r.expect_err("must be an error");
    assert!(s.is_empty());
}

#[test]
fn varint() {
    fn check(value: u64, encoded: &[u8]) {
        // Small buffer.
        let mut buf = Vec::with_capacity(1);
        encode_varint(value, &mut buf);
        assert_eq!(buf, encoded);

        // Large buffer.
        let mut buf = Vec::with_capacity(100);
        encode_varint(value, &mut buf);
        assert_eq!(buf, encoded);

        // Constant encoded.
        assert_eq!(&*const_varint(value), encoded);

        assert_eq!(encoded_len_varint(value), encoded.len());

        let roundtrip_value = decode_varint(&mut &*encoded).expect("decoding failed");
        assert_eq!(value, roundtrip_value);

        let roundtrip_value = decode_varint_slow(&mut &*encoded).expect("slow decoding failed");
        assert_eq!(value, roundtrip_value);
    }

    check(2u64.pow(0) - 1, &[0x00]);
    check(2u64.pow(0), &[0x01]);

    check(2u64.pow(7) - 1, &[0x7F]);
    check(256, &[0x80, 0x01]);
    check(128, &[0x80, 0x00]);
    check(300, &[0xAC, 0x01]);

    check(2u64.pow(14) - 1, &[0xFF, 0x7E]);
    check(2u64.pow(14), &[0x80, 0x7f]);

    check(0x407f, &[0xFF, 0x7F]);
    check(0x4080, &[0x80, 0x80, 0x00]);
    check(0x8080, &[0x80, 0x80, 0x01]);

    check(2u64.pow(21) - 1, &[0xFF, 0xFE, 0x7E]);
    check(2u64.pow(21), &[0x80, 0xFF, 0x7E]);

    check(0x20407f, &[0xFF, 0xFF, 0x7F]);
    check(0x204080, &[0x80, 0x80, 0x80, 0x00]);
    check(0x404080, &[0x80, 0x80, 0x80, 0x01]);

    check(2u64.pow(28) - 1, &[0xFF, 0xFE, 0xFE, 0x7E]);
    check(2u64.pow(28), &[0x80, 0xFF, 0xFE, 0x7E]);

    check(0x1020407f, &[0xFF, 0xFF, 0xFF, 0x7F]);
    check(0x10204080, &[0x80, 0x80, 0x80, 0x80, 0x00]);
    check(0x20204080, &[0x80, 0x80, 0x80, 0x80, 0x01]);

    check(2u64.pow(35) - 1, &[0xFF, 0xFE, 0xFE, 0xFE, 0x7E]);
    check(2u64.pow(35), &[0x80, 0xFF, 0xFE, 0xFE, 0x7E]);

    check(0x81020407f, &[0xFF, 0xFF, 0xFF, 0xFF, 0x7F]);
    check(0x810204080, &[0x80, 0x80, 0x80, 0x80, 0x80, 0x00]);
    check(0x1010204080, &[0x80, 0x80, 0x80, 0x80, 0x80, 0x01]);

    check(2u64.pow(42) - 1, &[0xFF, 0xFE, 0xFE, 0xFE, 0xFE, 0x7E]);
    check(2u64.pow(42), &[0x80, 0xFF, 0xFE, 0xFE, 0xFE, 0x7E]);

    check(0x4081020407f, &[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x7F]);
    check(0x40810204080, &[0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x00]);
    check(0x80810204080, &[0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x01]);

    check(
        2u64.pow(49) - 1,
        &[0xFF, 0xFE, 0xFE, 0xFE, 0xFE, 0xFE, 0x7E],
    );
    check(2u64.pow(49), &[0x80, 0xFF, 0xFE, 0xFE, 0xFE, 0xFE, 0x7E]);

    check(0x204081020407f, &[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x7F]);
    check(
        0x2040810204080,
        &[0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x00],
    );
    check(
        0x4040810204080,
        &[0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x01],
    );

    check(
        2u64.pow(56) - 1,
        &[0xFF, 0xFE, 0xFE, 0xFE, 0xFE, 0xFE, 0xFE, 0x7E],
    );
    check(
        2u64.pow(56),
        &[0x80, 0xFF, 0xFE, 0xFE, 0xFE, 0xFE, 0xFE, 0x7E],
    );

    check(
        0x10204081020407f,
        &[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x7F],
    );
    check(
        0x102040810204080,
        &[0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x00],
    );
    check(
        0x202040810204080,
        &[0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x01],
    );

    check(
        2u64.pow(63) - 1,
        &[0xFF, 0xFE, 0xFE, 0xFE, 0xFE, 0xFE, 0xFE, 0xFE, 0x7E],
    );
    check(
        2u64.pow(63),
        &[0x80, 0xFF, 0xFE, 0xFE, 0xFE, 0xFE, 0xFE, 0xFE, 0x7E],
    );

    check(
        0x810204081020407f,
        &[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x7F],
    );
    check(
        0x8102040810204080,
        &[0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80],
    );
    // check(
    //     0x10102040810204080, //
    //     &[0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x01],
    // );

    check(
        u64::MAX,
        &[0xFF, 0xFE, 0xFE, 0xFE, 0xFE, 0xFE, 0xFE, 0xFE, 0xFE],
    );
    check(
        i64::MAX as u64,
        &[0xFF, 0xFE, 0xFE, 0xFE, 0xFE, 0xFE, 0xFE, 0xFE, 0x7E],
    );
}

#[test]
fn varint_overflow() {
    let u64_max_plus_one: &[u8] = &[0x80, 0xFF, 0xFE, 0xFE, 0xFE, 0xFE, 0xFE, 0xFE, 0xFE];

    assert_eq!(
        decode_varint(&mut &*u64_max_plus_one)
            .expect_err("decoding u64::MAX + 1 succeeded")
            .kind(),
        InvalidVarint
    );
    assert_eq!(
        decode_varint_slow(&mut &*u64_max_plus_one)
            .expect_err("slow decoding u64::MAX + 1 succeeded")
            .kind(),
        InvalidVarint
    );

    let u64_over_max: &[u8] = &[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];

    assert_eq!(
        decode_varint(&mut &*u64_over_max)
            .expect_err("decoding over-max succeeded")
            .kind(),
        InvalidVarint
    );
    assert_eq!(
        decode_varint_slow(&mut &*u64_over_max)
            .expect_err("slow decoding over-max succeeded")
            .kind(),
        InvalidVarint
    );
}

#[test]
fn varint_truncated() {
    let truncated_one_byte: &[u8] = &[0x80];
    assert_eq!(
        decode_varint(&mut &*truncated_one_byte)
            .expect_err("decoding truncated 1 byte succeeded")
            .kind(),
        Truncated
    );
    assert_eq!(
        decode_varint_slow(&mut &*truncated_one_byte)
            .expect_err("slow decoding truncated 1 byte succeeded")
            .kind(),
        Truncated
    );

    let truncated_two_bytes: &[u8] = &[0x80, 0xFF];
    assert_eq!(
        decode_varint(&mut &*truncated_two_bytes)
            .expect_err("decoding truncated 6 bytes succeeded")
            .kind(),
        Truncated
    );
    assert_eq!(
        decode_varint_slow(&mut &*truncated_two_bytes)
            .expect_err("slow decoding truncated 6 bytes succeeded")
            .kind(),
        Truncated
    );

    let truncated_six_bytes: &[u8] = &[0x80, 0x81, 0x82, 0x8A, 0x8B, 0x8C];
    assert_eq!(
        decode_varint(&mut &*truncated_six_bytes)
            .expect_err("decoding truncated 6 bytes succeeded")
            .kind(),
        Truncated
    );
    assert_eq!(
        decode_varint_slow(&mut &*truncated_six_bytes)
            .expect_err("slow decoding truncated 6 bytes succeeded")
            .kind(),
        Truncated
    );

    let truncated_eight_bytes: &[u8] = &[0x80, 0x81, 0x82, 0x8A, 0x8B, 0x8C, 0xBE, 0xEF];
    assert_eq!(
        decode_varint(&mut &*truncated_eight_bytes)
            .expect_err("decoding truncated 8 bytes succeeded")
            .kind(),
        Truncated
    );
    assert_eq!(
        decode_varint_slow(&mut &*truncated_eight_bytes)
            .expect_err("slow decoding truncated 8 bytes succeeded")
            .kind(),
        Truncated
    );
}

fn check_rejects_wrong_wire_type<T: ForOverwrite + Decoder<E>, E>(wire_type: WireType) {
    let mut out = T::for_overwrite();
    assert_eq!(
        <T as Decoder<E>>::decode(
            wire_type,
            false,
            &mut out,
            Capped::new(&mut [0u8; 0].as_slice()),
            DecodeContext::default(),
        ),
        Err(DecodeError::new(WrongWireType))
    );
}

fn check_rejects_wrong_wire_type_distinguished<T, E>(wire_type: WireType)
where
    T: ForOverwrite + Decoder<E> + DistinguishedDecoder<E>,
{
    let mut out = T::for_overwrite();
    assert_eq!(
        <T as DistinguishedDecoder<E>>::decode_distinguished(
            wire_type,
            false,
            &mut out,
            Capped::new(&mut [0u8; 0].as_slice()),
            RestrictedDecodeContext::new(Canonicity::NotCanonical),
        ),
        Err(DecodeError::new(WrongWireType))
    );
    check_rejects_wrong_wire_type::<T, E>(wire_type);
}

#[test]
fn varints_reject_wrong_wire_type() {
    for wire_type in [
        WireType::LengthDelimited,
        WireType::ThirtyTwoBit,
        WireType::SixtyFourBit,
    ] {
        check_rejects_wrong_wire_type_distinguished::<u32, General>(wire_type);
        check_rejects_wrong_wire_type_distinguished::<u64, General>(wire_type);
        check_rejects_wrong_wire_type_distinguished::<i32, General>(wire_type);
        check_rejects_wrong_wire_type_distinguished::<i64, General>(wire_type);
        check_rejects_wrong_wire_type_distinguished::<bool, General>(wire_type);
    }
}

#[test]
fn floats_reject_wrong_wire_type() {
    for wire_type in [
        WireType::Varint,
        WireType::LengthDelimited,
        WireType::SixtyFourBit,
    ] {
        check_rejects_wrong_wire_type::<f32, General>(wire_type);
        check_rejects_wrong_wire_type::<f32, Fixed>(wire_type);
    }
    for wire_type in [
        WireType::Varint,
        WireType::LengthDelimited,
        WireType::ThirtyTwoBit,
    ] {
        check_rejects_wrong_wire_type::<f64, General>(wire_type);
        check_rejects_wrong_wire_type::<f64, Fixed>(wire_type);
    }
}

#[test]
fn variable_length_values_reject_wrong_wire_type() {
    for wire_type in [
        WireType::Varint,
        WireType::ThirtyTwoBit,
        WireType::SixtyFourBit,
    ] {
        check_rejects_wrong_wire_type_distinguished::<String, General>(wire_type);
        check_rejects_wrong_wire_type_distinguished::<Blob, General>(wire_type);
        check_rejects_wrong_wire_type_distinguished::<Vec<u8>, PlainBytes>(wire_type);
    }
}

proptest! {
    #[test]
    fn u32_in_u64(value: u32) {
        let mut buf = Vec::<u8>::new();
        ValueEncoder::<General>::encode_value(&value, &mut buf);
        let mut out = 0u64;
        prop_assert!(ValueDecoder::<General>::decode_value(
            &mut out,
            Capped::new(&mut &*buf),
            DecodeContext::default(),
        ).is_ok());
        prop_assert_eq!(out, value as u64);
    }

    #[test]
    fn i32_in_i64(value: i32) {
        let mut buf = Vec::<u8>::new();
        ValueEncoder::<General>::encode_value(&value, &mut buf);
        let mut out = 0i64;
        prop_assert!(ValueDecoder::<General>::decode_value(
            &mut out,
            Capped::new(&mut &*buf),
            DecodeContext::default(),
        ).is_ok());
        prop_assert_eq!(out, value as i64);
    }

    #[test]
    fn u64_in_u32(value: u32) {
        let value = value as u64;
        let mut buf = Vec::<u8>::new();
        ValueEncoder::<General>::encode_value(&value, &mut buf);
        let mut out = 0u32;
        prop_assert!(ValueDecoder::<General>::decode_value(
            &mut out,
            Capped::new(&mut &*buf),
            DecodeContext::default(),
        ).is_ok());
        prop_assert_eq!(out as u64, value);
    }

    #[test]
    fn i64_in_i32(value: i32) {
        let value = value as i64;
        let mut buf = Vec::<u8>::new();
        ValueEncoder::<General>::encode_value(&value, &mut buf);
        let mut out = 0i32;
        prop_assert!(ValueDecoder::<General>::decode_value(
            &mut out,
            Capped::new(&mut &*buf),
            DecodeContext::default(),
        ).is_ok());
        prop_assert_eq!(out as i64, value);
    }

    #[test]
    fn u32_out_of_range(value in u32::MAX as u64 + 1..) {
        let mut buf = Vec::<u8>::new();
        ValueEncoder::<General>::encode_value(&value, &mut buf);
        let mut out = 0u32;
        prop_assert_eq!(
            ValueDecoder::<General>::decode_value(
                &mut out,
                Capped::new(&mut &*buf),
                DecodeContext::default(),
            ),
            Err(DecodeError::new(OutOfDomainValue))
        );
    }

    #[test]
    fn i32_out_of_range(
        low_value in ..i32::MIN as i64 - 1,
        high_value in i32::MAX as i64 + 1..,
    ) {
        for value in [low_value, high_value] {
            let mut buf = Vec::<u8>::new();
            ValueEncoder::<General>::encode_value(&value, &mut buf);
            let mut out = 0i32;
            prop_assert_eq!(
                ValueDecoder::<General>::decode_value(
                    &mut out,
                    Capped::new(&mut &*buf),
                    DecodeContext::default(),
                ),
                Err(DecodeError::new(OutOfDomainValue))
            );
        }
    }

    #[test]
    fn u16_out_of_range(value in u16::MAX as u64 + 1..) {
        let mut buf = Vec::<u8>::new();
        ValueEncoder::<General>::encode_value(&value, &mut buf);
        let mut out = 0u16;
        prop_assert_eq!(
            ValueDecoder::<General>::decode_value(
                &mut out,
                Capped::new(&mut &*buf),
                DecodeContext::default(),
            ),
            Err(DecodeError::new(OutOfDomainValue))
        );
    }

    #[test]
    fn i16_out_of_range(
        low_value in ..i16::MIN as i64 - 1,
        high_value in i16::MAX as i64 + 1..,
    ) {
        for value in [low_value, high_value] {
            let mut buf = Vec::<u8>::new();
            ValueEncoder::<General>::encode_value(&value, &mut buf);
            let mut out = 0i16;
            prop_assert_eq!(
                ValueDecoder::<General>::decode_value(
                    &mut out,
                    Capped::new(&mut &*buf),
                    DecodeContext::default(),
                ),
                Err(DecodeError::new(OutOfDomainValue))
            );
        }
    }

    #[test]
    fn u8_out_of_range(value in u8::MAX as u64 + 1..) {
        let mut buf = Vec::<u8>::new();
        ValueEncoder::<Varint>::encode_value(&value, &mut buf);
        let mut out = 0u8;
        prop_assert_eq!(
            ValueDecoder::<Varint>::decode_value(
                &mut out,
                Capped::new(&mut &*buf),
                DecodeContext::default(),
            ),
            Err(DecodeError::new(OutOfDomainValue))
        );
    }

    #[test]
    fn i8_out_of_range(
        low_value in ..i8::MIN as i64 - 1,
        high_value in i8::MAX as i64 + 1..,
    ) {
        for value in [low_value, high_value] {
            let mut buf = Vec::<u8>::new();
            ValueEncoder::<Varint>::encode_value(&value, &mut buf);
            let mut out = 0i8;
            prop_assert_eq!(
                ValueDecoder::<Varint>::decode_value(
                    &mut out,
                    Capped::new(&mut &*buf),
                    DecodeContext::default(),
                ),
                Err(DecodeError::new(OutOfDomainValue))
            );
        }
    }

    #[test]
    fn bool_out_of_range(varint in 2u64..) {
        let mut buf = Vec::<u8>::new();
        encode_varint(varint, &mut buf);
        let mut out = false;
        prop_assert_eq!(
            ValueDecoder::<General>::decode_value(
                &mut out,
                Capped::new(&mut &*buf),
                DecodeContext::default(),
            ),
            Err(DecodeError::new(OutOfDomainValue))
        );
    }

    #[test]
    fn field_key_too_big(tag in u32::MAX as u64 + 1..) {
        let mut buf = Vec::<u8>::new();
        encode_varint(tag << 2, &mut buf);
        prop_assert_eq!(
            TagReader::new().decode_key(Capped::new(&mut buf.as_slice())),
            Err(DecodeError::new(TagOverflowed))
        );
    }
}
