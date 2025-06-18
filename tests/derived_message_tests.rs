//! This file should contain most of the specific tests for the observed behavior and available
//! types of bilrost messages and their fields. If there's an observed behavior in a type of message
//! or field that we implement, we want to demonstrate it here.

use bilrost::encoding::opaque::{OpaqueMessage, OpaqueValue as OV};
use bilrost::encoding::{
    encode_varint, Collection, DistinguishedOneofBorrowDecoder, DistinguishedOneofDecoder,
    EmptyState, General, Oneof, OneofBorrowDecoder, OneofDecoder, Varint,
};
use bilrost::Canonicity::{Canonical, HasExtensions, NotCanonical};
use bilrost::DecodeErrorKind::{
    ConflictingFields, InvalidValue, InvalidVarint, OutOfDomainValue, TagOverflowed, Truncated,
    UnexpectedlyRepeated, WrongWireType,
};
use bilrost::{
    BorrowedMessage, DecodeErrorKind, DistinguishedBorrowedMessage, DistinguishedOwnedMessage,
    Enumeration, Message, Oneof, OwnedMessage,
};
use core::mem::size_of;
use itertools::{repeat_n, Itertools};
use std::borrow::Cow;
use std::default::Default;
use std::fmt::Debug;
use std::iter;
use std::marker::PhantomData;

trait IntoOpaqueMessage<'a> {
    fn into_opaque_message(self) -> OpaqueMessage<'a>;
}

impl<'a, T> IntoOpaqueMessage<'a> for &T
where
    T: Clone + IntoOpaqueMessage<'a>,
{
    fn into_opaque_message(self) -> OpaqueMessage<'a> {
        self.clone().into_opaque_message()
    }
}

impl<'a, const N: usize> IntoOpaqueMessage<'a> for [(u32, OV<'a>); N] {
    fn into_opaque_message(self) -> OpaqueMessage<'a> {
        OpaqueMessage::from_iter(self)
    }
}

impl<'a> IntoOpaqueMessage<'a> for &[(u32, OV<'a>)] {
    fn into_opaque_message(self) -> OpaqueMessage<'a> {
        OpaqueMessage::from_iter(self.iter().cloned())
    }
}

impl IntoOpaqueMessage<'static> for Vec<u8> {
    fn into_opaque_message(self) -> OpaqueMessage<'static> {
        <() as OwnedMessage>::decode(self.as_slice()).expect("did not decode with ignore unit");
        OpaqueMessage::decode(self.as_slice()).expect("did not decode")
    }
}

impl<'a> IntoOpaqueMessage<'a> for OpaqueMessage<'a> {
    fn into_opaque_message(self) -> OpaqueMessage<'a> {
        self
    }
}

impl<'a, I, F> IntoOpaqueMessage<'a> for iter::Map<I, F>
where
    Self: Iterator<Item = (u32, OV<'a>)>,
{
    fn into_opaque_message(self) -> OpaqueMessage<'a> {
        self.collect()
    }
}

trait FromOpaque {
    fn from_opaque<'a>(from: impl IntoOpaqueMessage<'a>) -> Self;
}

impl<T: OwnedMessage> FromOpaque for T {
    fn from_opaque<'a>(from: impl IntoOpaqueMessage<'a>) -> Self {
        Self::decode(&*from.into_opaque_message().encode_to_vec()).expect("failed to decode")
    }
}

mod assert {
    use super::*;
    use bilrost::Canonicity::Canonical;
    use bilrost::{
        BorrowedMessage, Canonicity, DecodeError, DistinguishedBorrowedMessage, WithCanonicity,
    };
    use bytes::BufMut;

    #[allow(unused_variables)]
    pub(super) fn assert_error(
        err: DecodeError,
        expected_kind: DecodeErrorKind,
        expected_path: &str,
    ) {
        assert_eq!(err.kind(), expected_kind);
        #[cfg(feature = "detailed-errors")]
        assert_eq!(
            err.path()
                .iter()
                .rev()
                .map(|p| format!("{}.{}", p.message, p.field))
                .join("/"),
            expected_path
        );
    }

    macro_rules! decodes {
        (owned relaxed, $from:expr, $into:expr $(,)?) => {{
            let encoded = $from.into_opaque_message().encode_to_vec();
            $crate::assert::decodes_owned(&encoded, $into);
            $crate::assert::decodes_borrowed(&encoded, $into);
        }};
        (borrowed relaxed, $from:expr, $into:expr $(,)?) => {{
            let encoded = $from.into_opaque_message().encode_to_vec();
            $crate::assert::decodes_borrowed(&encoded, $into);
        }};

        (owned distinguished, $from:expr, $into:expr $(,)?) => {{
            let encoded = $from.into_opaque_message().encode_to_vec();
            $crate::assert::decodes_distinguished_owned(&encoded, $into);
            $crate::assert::decodes_distinguished_borrowed(&encoded, $into);
        }};
        (borrowed distinguished, $from:expr, $into:expr $(,)?) => {{
            let encoded = $from.into_opaque_message().encode_to_vec();
            $crate::assert::decodes_distinguished_borrowed(&encoded, $into);
        }};

        (owned non-canonically, $from:expr, $into:expr, $canon:expr, $err:expr $(,)?) => {{
            let encoded = $from.into_opaque_message().encode_to_vec();
            $crate::assert::decodes_non_canonically_owned(&encoded, $into, $canon, $err);
            $crate::assert::decodes_non_canonically_borrowed(&encoded, $into, $canon, $err);
        }};
        (borrowed non-canonically, $from:expr, $into:expr, $canon:expr, $err:expr $(,)?) => {{
            let encoded = $from.into_opaque_message().encode_to_vec();
            $crate::assert::decodes_non_canonically_borrowed(&encoded, $into, $canon, $err);
        }};

        (owned relaxed errs for $ty:ty, $from:expr, $err:expr, $path:expr $(,)?) => {{
            let encoded = $from.into_opaque_message().encode_to_vec();
            $crate::assert::doesnt_decode_owned::<$ty>(&encoded, $err, $path);
            $crate::assert::doesnt_decode_borrowed::<$ty>(&encoded, $err, $path);
        }};
        (borrowed relaxed errs for $ty:ty, $from:expr, $err:expr, $path:expr $(,)?) => {{
            let encoded = $from.into_opaque_message().encode_to_vec();
            $crate::assert::doesnt_decode_borrowed::<$ty>(&encoded, $err, $path);
        }};

        (owned never decodes $ty:ty, $from:expr, $err:expr, $path:expr $(,)?) => {{
            let encoded = $from.into_opaque_message().encode_to_vec();
            $crate::assert::never_decodes_owned::<$ty>(&encoded, $err, $path);
            $crate::assert::never_decodes_borrowed::<$ty>(&encoded, $err, $path);
        }};
        (borrowed never decodes $ty:ty, $from:expr, $err:expr, $path:expr $(,)?) => {{
            let encoded = $from.into_opaque_message().encode_to_vec();
            $crate::assert::never_decodes_borrowed::<$ty>(&encoded, $err, $path);
        }};

        (owned always invalid for $ty:ty, $from:expr, $err:expr, $path:expr $(,)?) => {{
            let encoded: Vec<u8> = $from.to_owned();
            $crate::assert::never_decodes_owned::<$ty>(&encoded, $err, $path);
            $crate::assert::never_decodes_borrowed::<$ty>(&encoded, $err, $path);
        }};
        (borrowed always invalid for $ty:ty, $from:expr, $err:expr, $path:expr $(,)?) => {{
            let encoded: Vec<u8> = $from.to_owned();
            $crate::assert::never_decodes_borrowed::<$ty>(&encoded, $err, $path);
        }};
    }
    pub(super) use decodes;

    pub(super) fn decodes_owned<'a, M>(from: &'a [u8], into: M)
    where
        M: OwnedMessage + BorrowedMessage<'a> + Debug + PartialEq,
    {
        assert_eq!(M::decode(from).as_ref(), Ok(&into));
        let mut to_replace = M::new_empty();
        to_replace.replace_from(from).unwrap();
        assert_eq!(&to_replace, &into);
        decodes_borrowed(from, into);
    }

    pub(super) fn decodes_borrowed<'a, M>(from: &'a [u8], into: M)
    where
        M: BorrowedMessage<'a> + PartialEq + Debug,
    {
        assert_eq!(M::decode_borrowed(from).as_ref(), Ok(&into));
        let mut to_replace = M::new_empty();
        to_replace.replace_borrowed_from(from).unwrap();
        assert_eq!(&to_replace, &into);
    }

    pub(super) fn doesnt_decode_owned<'a, M>(from: &'a [u8], err: DecodeErrorKind, err_path: &str)
    where
        M: OwnedMessage + BorrowedMessage<'a> + Debug,
    {
        assert_error(
            M::decode(from).expect_err("unexpectedly decoded without error"),
            err,
            err_path,
        );
        let mut to_replace = M::new_empty();
        assert_error(
            to_replace
                .replace_from(from)
                .expect_err("unexpectedly replaced without error"),
            err,
            err_path,
        );
        doesnt_decode_borrowed::<M>(from, err, err_path);
    }

    pub(super) fn doesnt_decode_borrowed<'a, M>(
        from: &'a [u8],
        err: DecodeErrorKind,
        err_path: &str,
    ) where
        M: BorrowedMessage<'a> + Debug,
    {
        assert_error(
            M::decode_borrowed(from).expect_err("unexpectedly decoded without error"),
            err,
            err_path,
        );
        let mut to_replace = M::new_empty();
        assert_error(
            to_replace
                .replace_borrowed_from(from)
                .expect_err("unexpectedly replaced without error"),
            err,
            err_path,
        );
    }

    pub(super) fn decodes_distinguished_owned<'a, M>(from: &'a [u8], into: M)
    where
        M: DistinguishedOwnedMessage + DistinguishedBorrowedMessage<'a> + Debug + Eq,
    {
        assert_eq!(M::decode(from).as_ref(), Ok(&into));
        let (decoded, canon) =
            M::decode_distinguished(from).expect("distinguished decoding failed");
        assert_eq!(&decoded, &into, "distinguished decoded doesn't match");
        assert_eq!(canon, Canonical);
        let mut to_replace = M::new_empty();
        to_replace.replace_from(from).unwrap();
        assert_eq!(&to_replace, &into, "doesn't match after relaxed replace");
        to_replace = M::new_empty();
        assert_eq!(to_replace.replace_distinguished_from(from), Ok(Canonical));
        assert_eq!(
            &to_replace, &into,
            "doesn't match after distinguished replace"
        );
        assert_eq!(
            from,
            into.encode_to_vec(),
            "distinguished encoding does not round trip"
        );
        assert_eq!(into.encoded_len(), from.len(), "encoded_len was wrong");
        let mut prepend_round_trip = Vec::new();
        prepend_round_trip.put(into.encode_fast());
        assert_eq!(
            from, prepend_round_trip,
            "distinguished encoding does not round trip with prepend",
        );
        assert_eq!(from, into.encode_contiguous().into_vec());
        decodes_distinguished_borrowed(from, into);
    }

    pub(super) fn decodes_distinguished_borrowed<'a, M>(from: &'a [u8], into: M)
    where
        M: DistinguishedBorrowedMessage<'a> + Debug + Eq,
    {
        assert_eq!(M::decode_borrowed(from).as_ref(), Ok(&into));
        let (decoded, canon) =
            M::decode_distinguished_borrowed(from).expect("distinguished borrowed decoding failed");
        assert_eq!(
            &decoded, &into,
            "distinguished borrowed decoded doesn't match"
        );
        assert_eq!(canon, Canonical);
        let mut to_replace = M::new_empty();
        to_replace.replace_borrowed_from(from).unwrap();
        assert_eq!(
            &to_replace, &into,
            "doesn't match after relaxed borrowed replace"
        );
        to_replace = M::new_empty();
        assert_eq!(
            to_replace.replace_distinguished_borrowed_from(from),
            Ok(Canonical)
        );
        assert_eq!(
            &to_replace, &into,
            "doesn't match after distinguished borrowed replace"
        );
        assert_eq!(
            from,
            into.encode_to_vec(),
            "distinguished encoding does not round trip"
        );
        assert_eq!(into.encoded_len(), from.len(), "encoded_len was wrong");
        let mut prepend_round_trip = Vec::new();
        prepend_round_trip.put(into.encode_fast());
        assert_eq!(
            from, prepend_round_trip,
            "distinguished encoding does not round trip with prepend",
        );
        assert_eq!(from, into.encode_contiguous().into_vec());
    }

    /// Trait for easily passing expectations for restricted decoding results to
    /// `decodes_non_canonically`.
    pub(super) trait RestrictedExpectations {
        type Iter<'a>: IntoIterator<Item = (Canonicity, &'a str)>
        where
            Self: 'a;

        fn for_expected_canonicity(&self, expected: Canonicity) -> Self::Iter<'_>;
    }

    impl RestrictedExpectations for &str {
        type Iter<'a>
            = [(Canonicity, &'a str); 1]
        where
            Self: 'a;

        fn for_expected_canonicity(&self, expected: Canonicity) -> Self::Iter<'_> {
            [(expected, *self)]
        }
    }

    impl<const N: usize> RestrictedExpectations for [(Canonicity, &str); N] {
        type Iter<'a>
            = [(Canonicity, &'a str); N]
        where
            Self: 'a;

        fn for_expected_canonicity(&self, _: Canonicity) -> Self::Iter<'_> {
            assert!(!self.is_empty()); // we must not be lazy
            *self
        }
    }

    pub(super) fn decodes_non_canonically_owned<'a, M>(
        from: &'a [u8],
        into: M,
        expected_canon: Canonicity,
        err_expectations: impl RestrictedExpectations,
    ) where
        M: DistinguishedOwnedMessage + DistinguishedBorrowedMessage<'a> + Debug + Eq,
    {
        assert_ne!(expected_canon, Canonical); // otherwise why call this function

        assert_eq!(M::decode(from).as_ref(), Ok(&into));

        let mut to_replace = M::new_empty();
        to_replace.replace_from(from).unwrap();
        assert_eq!(&to_replace, &into);

        let (decoded, canon) = M::decode_distinguished(from)
            .expect("error decoding in distinguished mode with non-canonical data");
        assert_eq!(&decoded, &into, "distinguished decoded doesn't match");
        assert_eq!(canon, expected_canon);

        let mut to_replace = M::new_empty();
        assert_eq!(
            to_replace
                .replace_distinguished_from(from)
                .expect("error replacing in distinguished mode with non-canonical data"),
            expected_canon
        );
        assert_eq!(
            &to_replace, &into,
            "doesn't match after distinguished replace"
        );

        // also check that restricted mode errs, and errs correctly
        for (restricted_canon, error_path) in
            err_expectations.for_expected_canonicity(expected_canon)
        {
            let more_strict = match restricted_canon {
                NotCanonical => HasExtensions,
                HasExtensions => Canonical,
                Canonical => unreachable!(),
            };
            let expected_canon_err = restricted_canon.canonical().unwrap_err();
            assert_error(
                M::decode_restricted(from, more_strict).expect_err(
                    "decoded non-distinguished data in restricted mode but got no error",
                ),
                expected_canon_err,
                error_path,
            );
            assert_error(
                to_replace
                    .replace_restricted_from(from, more_strict)
                    .expect_err(
                        "replaced non-distinguished data in restricted mode but got no error",
                    ),
                expected_canon_err,
                error_path,
            );
        }
        decodes_non_canonically_borrowed(from, into, expected_canon, err_expectations);
    }

    pub(super) fn decodes_non_canonically_borrowed<'a, M>(
        from: &'a [u8],
        into: M,
        expected_canon: Canonicity,
        err_expectations: impl RestrictedExpectations,
    ) where
        M: DistinguishedBorrowedMessage<'a> + Debug + Eq,
    {
        assert_ne!(expected_canon, Canonical); // otherwise why call this function

        assert_eq!(M::decode_borrowed(from).as_ref(), Ok(&into));

        let mut to_replace = M::new_empty();
        to_replace.replace_borrowed_from(from).unwrap();
        assert_eq!(&to_replace, &into);

        let (decoded, canon) = M::decode_distinguished_borrowed(from)
            .expect("error decoding in distinguished mode with non-canonical data");
        assert_eq!(&decoded, &into, "distinguished decoded doesn't match");
        assert_eq!(canon, expected_canon);

        let mut to_replace = M::new_empty();
        assert_eq!(
            to_replace
                .replace_distinguished_borrowed_from(from)
                .expect("error replacing in distinguished mode with non-canonical data"),
            expected_canon
        );
        assert_eq!(
            &to_replace, &into,
            "doesn't match after distinguished replace"
        );

        // also check that restricted mode errs, and errs correctly
        for (restricted_canon, error_path) in
            err_expectations.for_expected_canonicity(expected_canon)
        {
            let more_strict = match restricted_canon {
                NotCanonical => HasExtensions,
                HasExtensions => Canonical,
                Canonical => unreachable!(),
            };
            let expected_canon_err = restricted_canon.canonical().unwrap_err();
            assert_error(
                M::decode_restricted_borrowed(from, more_strict).expect_err(
                    "decoded non-distinguished data in restricted mode but got no error",
                ),
                expected_canon_err,
                error_path,
            );
            assert_error(
                to_replace
                    .replace_restricted_borrowed_from(from, more_strict)
                    .expect_err(
                        "replaced non-distinguished data in restricted mode but got no error",
                    ),
                expected_canon_err,
                error_path,
            );
        }

        let round_tripped = into.encode_to_vec();
        assert_ne!(
            round_tripped, from,
            "encoding round tripped, but did not decode distinguished"
        );
        assert_eq!(
            into.encoded_len(),
            round_tripped.len(),
            "encoded_len was wrong"
        );
    }

    pub(super) fn never_decodes_owned<'a, M>(from: &'a [u8], err: DecodeErrorKind, err_path: &str)
    where
        M: DistinguishedOwnedMessage + DistinguishedBorrowedMessage<'a> + Debug,
    {
        assert_error(
            M::decode(from).expect_err("unepectedly decoded in relaxed mode without error"),
            err,
            err_path,
        );
        let mut to_replace = M::new_empty();
        assert_error(
            to_replace
                .replace_from(from)
                .expect_err("unexpectedly replaced in relaxed mode without error"),
            err,
            err_path,
        );
        assert_error(
            M::decode_distinguished(from)
                .expect_err("unexpectedly decoded in distinguished mode without error"),
            err,
            err_path,
        );
        let mut to_replace = M::new_empty();
        assert_error(
            to_replace
                .replace_distinguished_from(from)
                .expect_err("unexpectedly replaced in distinguished mode without error"),
            err,
            err_path,
        );
        never_decodes_borrowed::<M>(from, err, err_path);
    }

    pub(super) fn never_decodes_borrowed<'a, M>(
        from: &'a [u8],
        err: DecodeErrorKind,
        err_path: &str,
    ) where
        M: DistinguishedBorrowedMessage<'a> + Debug,
    {
        assert_error(
            M::decode_borrowed(from)
                .expect_err("unepectedly decoded in relaxed mode without error"),
            err,
            err_path,
        );
        let mut to_replace = M::new_empty();
        assert_error(
            to_replace
                .replace_borrowed_from(from)
                .expect_err("unexpectedly replaced in relaxed mode without error"),
            err,
            err_path,
        );
        assert_error(
            M::decode_distinguished_borrowed(from)
                .expect_err("unexpectedly decoded in distinguished mode without error"),
            err,
            err_path,
        );
        let mut to_replace = M::new_empty();
        assert_error(
            to_replace
                .replace_distinguished_borrowed_from(from)
                .expect_err("unexpectedly replaced in distinguished mode without error"),
            err,
            err_path,
        );
    }

    pub(super) fn encodes<'a, M: OwnedMessage>(value: M, becomes: impl IntoOpaqueMessage<'a>) {
        let forward_encoded = value.encode_to_vec();
        assert_eq!(
            OpaqueMessage::decode(forward_encoded.as_slice()),
            Ok(becomes.into_opaque_message())
        );
        assert_eq!(
            value.encoded_len(),
            forward_encoded.len(),
            "encoded_len was wrong"
        );
        let prepended = value.encode_fast();
        let mut prepend_encoded = Vec::new();
        prepend_encoded.put(prepended);
        assert_eq!(forward_encoded, prepend_encoded);
        assert_eq!(forward_encoded, value.encode_contiguous().into_vec());
    }
}

// Tests for derived trait bounds

#[test]
fn derived_trait_bounds() {
    #[allow(dead_code)]
    struct X; // Not encodable

    #[allow(dead_code)]
    #[derive(PartialEq, Eq, Oneof)]
    #[bilrost(distinguished)]
    enum A<T> {
        Empty,
        #[bilrost(1)]
        One(bool),
        #[bilrost(2)]
        Two(T),
    }
    static_assertions::assert_impl_all!(A<bool>: DistinguishedOneofDecoder);
    static_assertions::assert_impl_all!(A<f32>: OneofDecoder);
    static_assertions::assert_not_impl_any!(A<f32>: DistinguishedOneofDecoder);
    static_assertions::assert_not_impl_any!(A<X>: DistinguishedOneofDecoder);

    #[allow(dead_code)]
    #[derive(PartialEq, Eq, Message)]
    #[bilrost(distinguished)]
    struct Inner<U>(U);
    static_assertions::assert_impl_all!(Inner<bool>: DistinguishedOwnedMessage);
    static_assertions::assert_impl_all!(Inner<f32>: OwnedMessage);
    static_assertions::assert_not_impl_any!(Inner<f32>: DistinguishedOwnedMessage);
    static_assertions::assert_not_impl_any!(Inner<X>: DistinguishedOwnedMessage);

    #[allow(dead_code)]
    #[derive(PartialEq, Eq, Message)]
    #[bilrost(distinguished)]
    struct Foo<T, U, V>(#[bilrost(oneof(1, 2))] A<T>, Inner<U>, V);
    static_assertions::assert_impl_all!(Foo<bool, bool, bool>: DistinguishedOwnedMessage);
    static_assertions::assert_impl_all!(Foo<f32, bool, bool>: OwnedMessage);
    static_assertions::assert_impl_all!(Foo<bool, f32, bool>: OwnedMessage);
    static_assertions::assert_impl_all!(Foo<bool, bool, f32>: OwnedMessage);
    static_assertions::assert_not_impl_any!(Foo<f32, bool, bool>: DistinguishedOwnedMessage);
    static_assertions::assert_not_impl_any!(Foo<bool, f32, bool>: DistinguishedOwnedMessage);
    static_assertions::assert_not_impl_any!(Foo<bool, bool, f32>: DistinguishedOwnedMessage);
    static_assertions::assert_not_impl_any!(Foo<X, bool, bool>: DistinguishedOwnedMessage);
    static_assertions::assert_not_impl_any!(Foo<bool, X, bool>: DistinguishedOwnedMessage);
    static_assertions::assert_not_impl_any!(Foo<bool, bool, X>: DistinguishedOwnedMessage);
}

#[test]
fn recursive_messages() {
    #[allow(dead_code)]
    #[derive(PartialEq, Eq, Message)]
    #[bilrost(distinguished)]
    struct Tree {
        #[bilrost(recurses)]
        children: Vec<Tree>,
    }

    static_assertions::assert_impl_all!(Tree: DistinguishedOwnedMessage);
}

// Tests for encoding rigor

#[test]
fn derived_message_field_ordering() {
    #[derive(Clone, Debug, PartialEq, Eq, Oneof)]
    #[bilrost(distinguished)]
    enum A {
        #[bilrost(1)]
        One(bool),
        #[bilrost(10)]
        Ten(bool),
        #[bilrost(20)]
        Twenty(bool),
    }

    #[derive(Clone, Debug, PartialEq, Eq, Oneof)]
    #[bilrost(distinguished)]
    enum B {
        #[bilrost(9)]
        Nine(bool),
        #[bilrost(11)]
        Eleven(bool),
    }

    #[derive(Clone, Debug, PartialEq, Eq, Oneof)]
    #[bilrost(distinguished)]
    enum C {
        #[bilrost(13)]
        Thirteen(bool),
        #[bilrost(16)]
        Sixteen(bool),
        #[bilrost(22)]
        TwentyTwo(bool),
    }

    #[derive(Clone, Debug, PartialEq, Eq, Oneof)]
    #[bilrost(distinguished)]
    enum D {
        #[bilrost(18)]
        Eighteen(bool),
        #[bilrost(19)]
        Nineteen(bool),
    }

    #[derive(Clone, Debug, PartialEq, Eq, Message)]
    #[bilrost(distinguished)]
    struct Struct {
        #[bilrost(0)]
        zero: bool,
        #[bilrost(oneof = "1, 10, 20")]
        a: Option<A>,
        #[bilrost(4)]
        four: bool,
        #[bilrost(5)]
        five: bool,
        #[bilrost(oneof = "9, 11")]
        b: Option<B>,
        // implicitly tagged 12
        twelve: bool,
        #[bilrost(oneof = "13, 16, 22")]
        c: Option<C>,
        #[bilrost(14)]
        fourteen: bool,
        // implicitly tagged 15
        fifteen: bool,
        #[bilrost(17)]
        seventeen: bool,
        #[bilrost(oneof = "18, 19")]
        d: Option<D>,
        #[bilrost(21)]
        twentyone: bool,
        #[bilrost(50)]
        fifty: bool,
    }

    let bools = repeat_n([false, true], 9).multi_cartesian_product();
    let abcd = [None, Some(1), Some(10), Some(20)]
        .into_iter()
        .cartesian_product([None, Some(9), Some(11)])
        .cartesian_product([None, Some(13), Some(16), Some(22)])
        .cartesian_product([None, Some(18), Some(19)]);
    for (bools, oneofs) in bools.cartesian_product(abcd) {
        let field_tags = bools
            .into_iter()
            .zip([0, 4, 5, 12, 14, 15, 17, 21, 50]) // plain bool tags
            .filter_map(|(present, tag)| present.then_some(tag));
        let (((a, b), c), d) = oneofs;
        // Encoding of `true` for each plain field set to true and each oneof field's tag
        let opaque_message = OpaqueMessage::from_iter(
            field_tags
                .chain([a, b, c, d].into_iter().flatten())
                .map(|tag| (tag, OV::bool(true))),
        );
        assert::decodes!(owned distinguished, &opaque_message, Struct::from_opaque(&opaque_message));
    }
}

#[test]
fn field_tag_limits() {
    #[derive(Debug, PartialEq, Eq, Message)]
    #[bilrost(distinguished)]
    struct Foo {
        #[bilrost(0)]
        minimum: Option<bool>,
        #[bilrost(4294967295)]
        maximum: Option<bool>,
    }
    assert::decodes!(
        owned distinguished,
        [(0, OV::bool(false)), (u32::MAX, OV::bool(true))],
        Foo {
            minimum: Some(false),
            maximum: Some(true),
        },
    );
    assert::decodes!(
        owned never decodes Foo,
        [(0, OV::bool(false)), (0, OV::bool(true))],
        UnexpectedlyRepeated,
        "Foo.minimum",
    );
    assert::decodes!(
        owned never decodes Foo,
        [(u32::MAX, OV::bool(false)), (u32::MAX, OV::bool(true))],
        UnexpectedlyRepeated,
        "Foo.maximum",
    );
    assert::decodes!(
        owned non-canonically,
        [
            (0, OV::bool(true)),
            (234234234, OV::string("unknown")), // unknown field
            (u32::MAX, OV::bool(false)),
        ],
        Foo {
            minimum: Some(true),
            maximum: Some(false),
        },
        HasExtensions,
        "",
    );
}

#[test]
fn fieldless_messages() {
    #[derive(Debug, PartialEq, Eq, Message)]
    #[bilrost(distinguished)]
    struct Foo;
    assert::decodes!(owned distinguished, [], Foo);

    #[derive(Debug, PartialEq, Eq, Message)]
    #[bilrost(distinguished)]
    struct Bar {}
    assert::decodes!(owned distinguished, [], Bar{});

    #[derive(Debug, PartialEq, Eq, Message)]
    #[bilrost(distinguished)]
    struct Baz();
    assert::decodes!(owned distinguished, [], Baz());
}

#[test]
fn message_catting_behavior() {
    // We can show that when messages are catted together, the fields stay ascending
    let first = [
        (0, OV::string("zero")),
        (1, OV::string("one")),
        (2, OV::string("two")),
    ]
    .into_opaque_message()
    .encode_to_vec();
    let second = [
        (0, OV::string("zero again")),
        (1, OV::string("one again")),
        (2, OV::string("two again")),
    ]
    .into_opaque_message()
    .encode_to_vec();
    let mut combined = first;
    combined.extend(second);
    assert_eq!(
        OpaqueMessage::decode(combined.as_slice()),
        Ok([
            (0, OV::string("zero")),
            (1, OV::string("one")),
            (2, OV::string("two")),
            // When the messages are concatenated, the second message's tags are offset by the
            // tag of the last field in the prior message, because they are encoded as deltas in
            // the field key.
            (2, OV::string("zero again")),
            (3, OV::string("one again")),
            (4, OV::string("two again")),
        ]
        .into_opaque_message()),
    );
}

#[test]
fn rejects_overflowed_tags() {
    let maximum_tag = [(u32::MAX, OV::bool(true))]
        .into_opaque_message()
        .encode_to_vec();
    let one_more_tag = [(1, OV::string("too much"))]
        .into_opaque_message()
        .encode_to_vec();
    let mut combined = maximum_tag;
    combined.extend(one_more_tag);
    // Nothing should ever be able to decode this message; it's not a valid encoding.
    assert::decodes!(owned always invalid for OpaqueMessage, &combined, TagOverflowed, "");
    assert::decodes!(owned always invalid for (), &combined, TagOverflowed, "");

    let mut first_tag_too_big = Vec::new();
    // This is the first varint that's always an invalid field key.
    encode_varint((u32::MAX as u64 + 1) << 2, &mut first_tag_too_big);
    // Nothing should ever be able to decode this message either; it's not a valid encoding.
    assert::decodes!(owned always invalid for OpaqueMessage, &first_tag_too_big, TagOverflowed, "");
    assert::decodes!(owned always invalid for (), &first_tag_too_big, TagOverflowed, "");
}

#[test]
fn truncated_field_and_tag() {
    #[derive(Debug, PartialEq, Eq, Message)]
    #[bilrost(distinguished)]
    struct Foo(#[bilrost(100)] String, #[bilrost(1_000_000)] u64);

    let mut buf = [(100, OV::string("abc")), (1_000_000, OV::Varint(1))]
        .into_opaque_message()
        .encode_to_vec();
    buf.pop(); // Remove the value from the last field
    assert::decodes!(owned always invalid for (), buf, Truncated, "");
    assert::decodes!(owned always invalid for Foo, buf, Truncated, "Foo.1");
    assert::decodes!(owned always invalid for OpaqueMessage, buf, Truncated, "");
    buf.pop(); // Remove part of the key for the last field as well
    assert::decodes!(owned always invalid for (), buf, Truncated, "");
    assert::decodes!(owned always invalid for Foo, buf, Truncated, "");
    assert::decodes!(owned always invalid for OpaqueMessage, buf, Truncated, "");
}

#[test]
fn ignored_fields() {
    #[derive(Debug, Default, PartialEq, Message)]
    struct FooPlus {
        x: i64,
        y: i64,
        #[bilrost(ignore)]
        also: usize,
    }

    assert::decodes!(
        owned relaxed,
        [(1, OV::i64(1)), (2, OV::i64(-2))],
        FooPlus {
            x: 1,
            y: -2,
            also: 0,
        },
    );

    let mut foo_msg = FooPlus {
        x: 5,
        y: 10,
        also: 123,
    };
    assert::decodes!(
        owned relaxed,
        foo_msg.encode_to_vec(),
        FooPlus {
            x: 5,
            y: 10,
            also: 0,
        },
    );

    foo_msg
        .replace_from(
            [
                (1, OV::i64(6)),
                (2, OV::i64(12)),
                (333, OV::string("unknown")),
            ]
            .into_opaque_message()
            .encode_to_vec()
            .as_slice(),
        )
        .expect("replace failed unexpectedly");
    assert_eq!(
        foo_msg,
        FooPlus {
            x: 6,
            y: 12,
            also: 123,
        }
    );

    assert_eq!(
        foo_msg
            .replace_from(
                [(1, OV::i64(456)), (2, OV::string("wrong wire type"))]
                    .into_opaque_message()
                    .encode_to_vec()
                    .as_slice()
            )
            .expect_err("replace with wrong wire type succeeded unexpectedly")
            .kind(),
        WrongWireType
    );
    // After a failed decode, the message's non-ignored fields should be cleared rather than
    // incompletely populated
    assert_eq!(
        foo_msg,
        FooPlus {
            x: 0,
            y: 0,
            also: 123,
        }
    );
}

#[test]
fn ignored_fields_with_defaults() {
    #[derive(Debug, PartialEq, Message)]
    struct FooPlus {
        x: i64,
        y: i64,
        #[bilrost(ignore)]
        also: usize,
    }

    // Some Default implementation is required when there are ignored fields. It doesn't have
    // to be the derived implementation, and it can have non-empty values for non-ignored
    // fields.
    impl Default for FooPlus {
        fn default() -> Self {
            Self {
                x: 111,
                y: 222,
                also: 12345,
            }
        }
    }

    // The empty value for the message will still have the empty value for all non-ignored
    // fields; the rest will be taken from the `Default` implementation.
    assert_eq!(
        FooPlus::new_empty(),
        FooPlus {
            x: 0,
            y: 0,
            also: 12345,
        }
    );

    assert::decodes!(
        owned relaxed,
        [(1, OV::i64(1))],
        FooPlus {
            x: 1,
            y: 0,
            also: 12345,
        },
    )
}

#[test]
fn ignored_fields_with_per_field_defaults() {
    #[derive(Debug, PartialEq)]
    struct Undecodable(usize);

    impl Default for Undecodable {
        fn default() -> Self {
            Undecodable(12345)
        }
    }

    #[derive(Debug, PartialEq, Message)]
    #[bilrost(default_per_field)]
    struct FooPlus {
        x: i64,
        y: i64,
        #[bilrost(ignore)]
        also: Undecodable,
    }

    // The empty value for the message will still have the empty value for all non-ignored
    // fields; the rest will be taken from the `Default` implementation.
    assert_eq!(
        FooPlus::new_empty(),
        FooPlus {
            x: 0,
            y: 0,
            also: Undecodable(12345),
        }
    );

    assert::decodes!(
        owned relaxed,
        [(1, OV::i64(1))],
        FooPlus {
            x: 1,
            y: 0,
            also: Undecodable(12345),
        },
    );

    #[derive(Message)]
    #[bilrost(default_per_field)]
    struct IgnoredGeneric<T> {
        #[bilrost(ignore)]
        _whatever: T,
    }

    struct _NoDefault;

    static_assertions::assert_not_impl_any!(IgnoredGeneric<_NoDefault>: Message);
    static_assertions::assert_impl_all!(IgnoredGeneric<Undecodable>: Message);
}

#[test]
fn field_clearing() {
    use bilrost::Blob;
    use bytes::Bytes;
    #[cfg(feature = "bytestring")]
    use bytestring::ByteString;
    #[cfg(feature = "smallvec")]
    use smallvec::SmallVec;
    use std::collections::{BTreeMap, BTreeSet};
    #[cfg(feature = "std")]
    use std::collections::{HashMap, HashSet};
    #[cfg(feature = "thin-vec")]
    use thin_vec::ThinVec;
    #[cfg(feature = "tinyvec")]
    use tinyvec::TinyVec;

    #[derive(Clone, Debug, PartialEq, Eq, Enumeration)]
    enum Hmm {
        Nope = 0,
        Maybe = 1,
    }

    #[derive(Debug, PartialEq, Message)]
    struct Nested(u32);

    #[derive(Debug, PartialEq, Message)]
    struct Clearable<'a> {
        #[bilrost(encoding(varint))]
        a: u8,
        #[bilrost(encoding(varint))]
        b: i8,
        c: u16,
        d: i16,
        e: u32,
        f: i32,
        g: u64,
        h: i64,
        i: bool,
        j: f32,
        k: f64,
        string: String,
        blob: Blob,
        #[bilrost(encoding(plainbytes))]
        byte_arr: [u8; 1],
        hmm: Hmm,
        nested: Nested,
        opt: Option<u32>,
        vec: Vec<u32>,
        btmap: BTreeMap<u32, u32>,
        btset: BTreeSet<u32>,
        #[cfg(feature = "std")]
        hashmap: HashMap<u32, u32>,
        #[cfg(feature = "std")]
        hashset: HashSet<u32>,
        bytes: Bytes,
        #[cfg(feature = "bytestring")]
        bytestring: ByteString,
        #[bilrost(encoding(plainbytes))]
        cow_bytes_borrowed: Cow<'a, [u8]>,
        #[bilrost(encoding(plainbytes))]
        cow_bytes_owned: Cow<'a, [u8]>,
        cow_str_borrowed: Cow<'a, str>,
        cow_str_owned: Cow<'a, str>,
        #[cfg(feature = "arrayvec")]
        arrayvec: arrayvec::ArrayVec<u32, 2>,
        #[cfg(feature = "smallvec")]
        smallvec: SmallVec<[u32; 1]>,
        #[cfg(feature = "smallvec")]
        #[bilrost(encoding(plainbytes))]
        smallvec_bytes: SmallVec<[u8; 1]>,
        #[cfg(feature = "thin-vec")]
        thin_vec: ThinVec<u32>,
        #[cfg(feature = "thin-vec")]
        #[bilrost(encoding(plainbytes))]
        thin_vec_bytes: ThinVec<u8>,
        #[cfg(feature = "tinyvec")]
        tinyarrayvec: tinyvec::ArrayVec<[u32; 2]>,
        #[cfg(feature = "tinyvec")]
        tinyvec: TinyVec<[u32; 1]>,
        #[cfg(feature = "tinyvec")]
        #[bilrost(encoding(plainbytes))]
        tinyvec_bytes: TinyVec<[u8; 1]>,
        #[cfg(feature = "hashbrown")]
        hbmap: hashbrown::HashMap<u32, u32>,
        #[cfg(feature = "hashbrown")]
        hbset: hashbrown::HashSet<u32>,
    }

    impl Default for Clearable<'_> {
        fn default() -> Self {
            // Create a default value where every field is non-empty, and every field that may
            // own memory has extra allocated capacity.
            let mut result = Self {
                a: 1,
                b: 1,
                c: 1,
                d: 1,
                e: 1,
                f: 1,
                g: 1,
                h: 1,
                i: true,
                j: 1.0,
                k: 1.0,
                string: String::with_capacity(64),
                blob: Blob::from_vec(Vec::with_capacity(64)),
                byte_arr: [1],
                hmm: Hmm::Maybe,
                nested: Nested(1),
                opt: Some(1),
                vec: Vec::with_capacity(64),
                btmap: [(1, 1)].into(),
                btset: [1].into(),
                #[cfg(feature = "std")]
                hashmap: HashMap::with_capacity(64),
                #[cfg(feature = "std")]
                hashset: HashSet::with_capacity(64),
                bytes: b"foo".as_slice().into(),
                #[cfg(feature = "bytestring")]
                bytestring: "foo".into(),
                cow_bytes_borrowed: Cow::Borrowed(&b"foo"[..]),
                cow_bytes_owned: Vec::with_capacity(64).into(),
                cow_str_borrowed: Cow::Borrowed("foo"),
                cow_str_owned: String::with_capacity(64).into(),
                #[cfg(feature = "arrayvec")]
                arrayvec: arrayvec::ArrayVec::from([1, 2]),
                #[cfg(feature = "smallvec")]
                smallvec: SmallVec::with_capacity(64),
                #[cfg(feature = "smallvec")]
                smallvec_bytes: SmallVec::with_capacity(64),
                #[cfg(feature = "thin-vec")]
                thin_vec: ThinVec::with_capacity(64),
                #[cfg(feature = "thin-vec")]
                thin_vec_bytes: ThinVec::with_capacity(64),
                #[cfg(feature = "tinyvec")]
                tinyarrayvec: tinyvec::ArrayVec::from([1, 2]),
                #[cfg(feature = "tinyvec")]
                tinyvec: TinyVec::with_capacity(64),
                #[cfg(feature = "tinyvec")]
                tinyvec_bytes: TinyVec::with_capacity(64),
                #[cfg(feature = "hashbrown")]
                hbmap: hashbrown::HashMap::with_capacity(64),
                #[cfg(feature = "hashbrown")]
                hbset: hashbrown::HashSet::with_capacity(64),
            };
            result.string.push_str("foo");
            result.blob.push(1);
            result.vec.push(1);
            #[cfg(feature = "std")]
            result.hashmap.insert(1, 1);
            #[cfg(feature = "std")]
            result.hashset.insert(1);
            result.cow_bytes_owned.to_mut().push(1);
            result.cow_str_owned.to_mut().push_str("foo");
            #[cfg(feature = "smallvec")]
            result.smallvec.push(1);
            #[cfg(feature = "smallvec")]
            result.smallvec_bytes.extend(b"abc".iter().cloned());
            #[cfg(feature = "thin-vec")]
            result.thin_vec.push(1);
            #[cfg(feature = "thin-vec")]
            result.thin_vec_bytes.extend(b"abc".iter().cloned());
            #[cfg(feature = "tinyvec")]
            result.tinyvec.push(1);
            #[cfg(feature = "tinyvec")]
            result.tinyvec_bytes.extend(b"abc".iter().cloned());
            #[cfg(feature = "hashbrown")]
            result.hbmap.insert(1, 1);
            #[cfg(feature = "hashbrown")]
            result.hbset.insert(1);
            result
        }
    }

    let mut clearable = Clearable::default();
    assert!(!clearable.message_is_empty());
    clearable.clear_message();
    assert_eq!(clearable, Clearable::new_empty());
    assert!(clearable.message_is_empty());
    assert!(clearable.string.capacity() >= 64);
    assert!(clearable.blob.capacity() >= 64);
    assert!(clearable.vec.capacity() >= 64);
    #[cfg(feature = "std")]
    assert!(clearable.hashmap.capacity() >= 64);
    #[cfg(feature = "std")]
    assert!(clearable.hashset.capacity() >= 64);
    assert!(clearable.cow_bytes_owned.to_mut().capacity() >= 64);
    assert!(clearable.cow_str_owned.to_mut().capacity() >= 64);
    #[cfg(feature = "smallvec")]
    assert!(clearable.smallvec.capacity() >= 64);
    #[cfg(feature = "smallvec")]
    assert!(clearable.smallvec_bytes.capacity() >= 64);
    #[cfg(feature = "thin-vec")]
    assert!(clearable.thin_vec.capacity() >= 64);
    #[cfg(feature = "thin-vec")]
    assert!(clearable.thin_vec_bytes.capacity() >= 64);
    #[cfg(feature = "tinyvec")]
    assert!(clearable.tinyvec.capacity() >= 64);
    #[cfg(feature = "tinyvec")]
    assert!(clearable.tinyvec_bytes.capacity() >= 64);
    #[cfg(feature = "hashbrown")]
    assert!(clearable.hbmap.capacity() >= 64);
    #[cfg(feature = "hashbrown")]
    assert!(clearable.hbset.capacity() >= 64);

    assert::decodes!(owned relaxed, Clearable::default().encode_to_vec(), Clearable::default());
    assert::decodes!(owned relaxed, [], Clearable::new_empty());
}

#[test]
fn generic_encodings() {
    // It's possible to provide the encoding that a field uses generically to the struct type, too!
    // This works perfectly because all usages of that name appear in-scope with the generic.
    #[allow(dead_code)]
    #[derive(Message)]
    #[bilrost(default_per_field)]
    struct Foo<T, E>(#[bilrost(encoding(E))] T, #[bilrost(ignore)] PhantomData<E>);

    static_assertions::assert_impl_all!(Foo<String, General>: OwnedMessage);
    static_assertions::assert_not_impl_any!(Foo<u8, General>: Message);
    static_assertions::assert_impl_all!(Foo<u8, Varint>: OwnedMessage);
}

// Varint tests

#[test]
fn parsing_varints() {
    #[derive(Debug, PartialEq, Eq, Message)]
    #[bilrost(distinguished)]
    struct Foo(
        bool,
        #[bilrost(encoding(varint))] u8,
        #[bilrost(encoding(varint))] i8,
        u16,
        i16,
        u32,
        i32,
        u64,
        i64,
        usize,
        isize,
    );

    assert::decodes!(owned distinguished, [], Foo::new_empty());
    assert::decodes!(
        owned distinguished,
        (0..11).map(|tag| (tag, OV::Varint(1))),
        Foo(true, 1, -1, 1, -1, 1, -1, 1, -1, 1, -1),
    );
    for field in (0..9).cartesian_product([
        // Currently it is not supported to parse fixed-width values into varint fields.
        OV::fixed_u32(1),
        OV::fixed_u64(1),
        // Length-delimited values don't represent integers either.
        OV::string("1"),
    ]) {
        let tag = field.0;
        assert::decodes!(owned never decodes Foo, [field], WrongWireType, &format!("Foo.{tag}"));
    }
    for (tag, out_of_range) in [
        (0, 2),
        (1, 256),
        (2, 256),
        (3, 65536),
        (4, 65536),
        (5, 1 << 32),
        (6, 1 << 32),
        #[cfg(not(target_pointer_width = "64"))]
        (9, (usize::MAX as u64) + 1),
        #[cfg(not(target_pointer_width = "64"))]
        (10, (usize::MAX as u64) + 1),
    ] {
        assert::decodes!(
            owned never decodes Foo,
            [(tag, OV::u64(out_of_range))],
            OutOfDomainValue,
            &format!("Foo.{tag}"),
        );
        let should_fit = [(tag, OV::u64(out_of_range - 1))];
        assert::decodes!(owned distinguished, &should_fit, Foo::from_opaque(&should_fit));
    }
}

#[test]
fn bools() {
    #[derive(Debug, PartialEq, Eq, Message)]
    #[bilrost(distinguished)]
    struct Foo(bool);

    assert_eq!(OV::bool(false), OV::Varint(0));
    assert_eq!(OV::bool(true), OV::Varint(1));

    assert::decodes!(owned distinguished, [], Foo(false));
    assert::decodes!(
        owned non-canonically,
        [(0, OV::bool(false))],
        Foo(false),
        NotCanonical,
        "Foo.0",
    );
    assert::decodes!(owned distinguished, [(0, OV::bool(true))], Foo(true));
    assert::decodes!(owned never decodes Foo, [(0, OV::Varint(2))], OutOfDomainValue, "Foo.0");
}

#[test]
fn truncated_varint() {
    #[derive(Debug, PartialEq, Eq, Message)]
    #[bilrost(distinguished)]
    struct Foo<T>(#[bilrost(encoding(varint))] T);

    let mut buf = [(0, OV::Varint(2000))]
        .into_opaque_message()
        .encode_to_vec();
    buf.pop(); // shorten by 1 byte
    assert::decodes!(owned always invalid for OpaqueMessage, buf, Truncated, "");
    assert::decodes!(owned always invalid for Foo<bool>, buf, Truncated, "Foo.0");
    assert::decodes!(owned always invalid for Foo<u8>, buf, Truncated, "Foo.0");
    assert::decodes!(owned always invalid for Foo<u16>, buf, Truncated, "Foo.0");
    assert::decodes!(owned always invalid for Foo<u32>, buf, Truncated, "Foo.0");
    assert::decodes!(owned always invalid for Foo<u64>, buf, Truncated, "Foo.0");
    assert::decodes!(owned always invalid for Foo<usize>, buf, Truncated, "Foo.0");
    assert::decodes!(owned always invalid for Foo<i8>, buf, Truncated, "Foo.0");
    assert::decodes!(owned always invalid for Foo<i16>, buf, Truncated, "Foo.0");
    assert::decodes!(owned always invalid for Foo<i32>, buf, Truncated, "Foo.0");
    assert::decodes!(owned always invalid for Foo<i64>, buf, Truncated, "Foo.0");
    assert::decodes!(owned always invalid for Foo<isize>, buf, Truncated, "Foo.0");
}

#[test]
fn truncated_nested_varint() {
    #[derive(Debug, PartialEq, Eq, Message)]
    #[bilrost(distinguished)]
    struct Inner {
        val: u64,
    }

    #[derive(Debug, PartialEq, Eq, Message)]
    #[bilrost(distinguished)]
    struct Outer {
        inner: Inner,
    }

    let truncated_inner_invalid =
        // \x05: field 1, length-delimited; \x04: 4 bytes; \x04: field 1, varint;
        // \xff...: data that will be greedily decoded as an invalid varint.
        &b"\x05\x04\x04\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff"[..];
    let truncated_inner_valid =
        // \x05: field 1, length-delimited; \x04: 4 bytes; \x04: field 1, varint;
        // \xff...: data that will be greedily decoded as an valid varint that still runs over.
        &b"\x05\x04\x04\xff\xff\xff\xff\xff\x00\x01\x02\x03\x04\x05\x06\x07\x08\x09"[..];
    let invalid_not_truncated =
        // \x05: field 1, length-delimited; \x0a: 10 bytes; \x04: field 1, varint;
        // \xff...: an invalid varint
        &b"\x05\x0a\x04\xff\xff\xff\xff\xff\xff\xff\xff\xff"[..];

    // The desired result is that we can tell the difference between the inner region being
    // truncated before the varint ends and finding an invalid varint fully inside the inner
    // region.
    assert::decodes!(
        owned always invalid for Outer,
        truncated_inner_invalid,
        Truncated,
        "Outer.inner/Inner.val",
    );
    // When decoding a varint succeeds but runs over, we want to detect that too.
    assert::decodes!(
        owned always invalid for Outer,
        truncated_inner_valid,
        Truncated,
        "Outer.inner",
    );
    // When decoding an inner varint, we do see when it is invalid.
    assert::decodes!(
        owned always invalid for Outer,
        invalid_not_truncated,
        InvalidVarint,
        "Outer.inner/Inner.val",
    );
}

// Fixed width int tests

#[test]
fn parsing_fixed_width_ints() {
    #[derive(Debug, PartialEq, Eq, Message)]
    #[bilrost(distinguished)]
    struct Foo(
        #[bilrost(encoding(fixed))] u32,
        #[bilrost(encoding(fixed))] i32,
        #[bilrost(encoding(fixed))] u64,
        #[bilrost(encoding(fixed))] i64,
    );

    assert::decodes!(owned distinguished, [], Foo::new_empty());
    assert::decodes!(
        owned distinguished,
        [
            (0, OV::fixed_u32(1)),
            (1, OV::fixed_u32(1)),
            (2, OV::fixed_u64(1)),
            (3, OV::fixed_u64(1)),
        ],
        Foo(1, 1, 1, 1),
    );
    for tag in 0..4 {
        // Currently it is not supported to parse varint values into varint fields.
        assert::decodes!(
            owned never decodes Foo,
            [(tag, OV::Varint(1))],
            WrongWireType,
            &format!("Foo.{tag}"),
        );
    }
}

// Floating point tests

#[test]
fn parsing_floats() {
    #[derive(Debug, Message)]
    struct Foo(f32, f64);

    #[derive(Debug, Message)]
    struct Bar(
        #[bilrost(encoding(fixed))] f32,
        #[bilrost(encoding(fixed))] f64,
    );

    for wrong_size_value in [(0, OV::f64(1.0)), (1, OV::f32(2.0))] {
        let tag = wrong_size_value.0;
        let msg = &[wrong_size_value];
        assert::decodes!(owned relaxed errs for Foo, msg, WrongWireType, &format!("Foo.{tag}"));
        assert::decodes!(owned relaxed errs for Bar, msg, WrongWireType, &format!("Bar.{tag}"));
    }
}

#[test]
fn preserves_floating_point_special_values() {
    let present_zeros = [(0, OV::fixed_u32(0)), (1, OV::fixed_u64(0))];
    let negative_zeros = [
        (0, OV::ThirtyTwoBit([0, 0, 0, 0x80])),
        (1, OV::SixtyFourBit([0, 0, 0, 0, 0, 0, 0, 0x80])),
    ];
    let infinities = [(0, OV::f32(f32::INFINITY)), (1, OV::f64(f64::NEG_INFINITY))];
    let nans = [
        (0, OV::fixed_u32(0xffff_4321)),
        (1, OV::fixed_u64(0x7fff_dead_beef_cafe)),
    ];

    #[derive(Debug, PartialEq, Message)]
    struct Foo(f32, f64);

    assert::encodes(Foo(0.0, 0.0), []);
    assert::encodes(Foo(-0.0, -0.0), &negative_zeros);
    let decoded = Foo::from_opaque(&negative_zeros);
    assert_eq!(
        (decoded.0.to_bits(), decoded.1.to_bits()),
        (0x8000_0000, 0x8000_0000_0000_0000)
    );
    assert::encodes(Foo(f32::INFINITY, f64::NEG_INFINITY), &infinities);
    assert::decodes!(owned relaxed, &infinities, Foo(f32::INFINITY, f64::NEG_INFINITY));
    assert::encodes(
        Foo(
            f32::from_bits(0xffff_4321),
            f64::from_bits(0x7fff_dead_beef_cafe),
        ),
        &nans,
    );
    let decoded = Foo::from_opaque(&nans);
    assert_eq!(
        (decoded.0.to_bits(), decoded.1.to_bits()),
        (0xffff_4321, 0x7fff_dead_beef_cafe)
    );
    // Zeros that are encoded anyway still decode without error, because we are
    // necessarily in relaxed mode (floats don't impl `Eq`)
    let decoded = Foo::from_opaque(&present_zeros);
    assert_eq!((decoded.0.to_bits(), decoded.1.to_bits()), (0, 0));

    #[derive(Debug, PartialEq, Message)]
    struct Bar(
        #[bilrost(encoding(fixed))] f32,
        #[bilrost(encoding(fixed))] f64,
    );

    assert::encodes(Bar(0.0, 0.0), []);
    assert::encodes(Bar(-0.0, -0.0), &negative_zeros);
    let decoded = Bar::from_opaque(&negative_zeros);
    assert_eq!(
        (decoded.0.to_bits(), decoded.1.to_bits()),
        (0x8000_0000, 0x8000_0000_0000_0000)
    );
    assert::encodes(Bar(f32::INFINITY, f64::NEG_INFINITY), &infinities);
    assert::decodes!(owned relaxed, &infinities, Bar(f32::INFINITY, f64::NEG_INFINITY));
    assert::encodes(
        Bar(
            f32::from_bits(0xffff_4321),
            f64::from_bits(0x7fff_dead_beef_cafe),
        ),
        &nans,
    );
    let decoded = Bar::from_opaque(&nans);
    assert_eq!(
        (decoded.0.to_bits(), decoded.1.to_bits()),
        (0xffff_4321, 0x7fff_dead_beef_cafe)
    );
    // Zeros that are encoded anyway still decode without error, because we are
    // necessarily in relaxed mode (floats don't impl `Eq`)
    let decoded = Bar::from_opaque(&present_zeros);
    assert_eq!((decoded.0.to_bits(), decoded.1.to_bits()), (0, 0));
}

#[test]
fn floating_point_zero_is_present_nested() {
    #[derive(Debug, Message)]
    struct Inner(#[bilrost(1)] f32);

    #[derive(Debug, Message)]
    struct Outer(#[bilrost(1)] Inner);

    assert!(!Inner(-0.0).message_is_empty());
    assert!(!Outer(Inner(-0.0)).message_is_empty());
    assert::encodes(
        Outer(Inner(-0.0)),
        [(1, OV::message(&[(1, OV::f32(-0.0))].into_opaque_message()))],
    );
    let decoded = Outer::from_opaque(Outer(Inner(-0.0)).encode_to_vec());
    assert_eq!(decoded.0 .0.to_bits(), (-0.0f32).to_bits());
}

#[test]
fn truncated_fixed() {
    #[derive(Debug, PartialEq, Eq, Oneof)]
    #[bilrost(distinguished)]
    enum A<T> {
        Empty,
        #[bilrost(tag(1), encoding(fixed))]
        One(T),
    }

    #[derive(Debug, PartialEq, Eq, Message)]
    #[bilrost(distinguished)]
    struct Foo<T>(
        #[bilrost(oneof(1))] A<T>,
        #[bilrost(tag(2), encoding(fixed))] T,
    );

    #[derive(Debug, PartialEq, Eq, Message)]
    #[bilrost(distinguished)]
    struct Outer<T>(Foo<T>, String);

    macro_rules! check_fixed_truncation {
        ($ty:ty, $val:expr) => {
            let mut direct = [(2, $val.clone())].into_opaque_message().encode_to_vec();
            let mut in_oneof = [(1, $val.clone())].into_opaque_message().encode_to_vec();
            // Truncate by 1 byte
            direct.pop();
            in_oneof.pop();
            assert::decodes!(
                owned always invalid for Foo<$ty>,
                &direct,
                Truncated,
                "Foo.1",
            );
            assert::decodes!(
                owned always invalid for Foo<$ty>,
                &in_oneof,
                Truncated,
                "Foo.0/A.One",
            );
            assert::decodes!(owned always invalid for OpaqueMessage, &direct, Truncated, "");
            assert::decodes!(owned always invalid for OpaqueMessage, &in_oneof, Truncated, "");
            assert::decodes!(owned always invalid for (), &direct, Truncated, "");
            assert::decodes!(owned always invalid for (), &in_oneof, Truncated, "");

            let direct_nested = [
                (0, OV::byte_slice(&direct)),
                (1, OV::string("more data after that")),
            ]
            .into_opaque_message()
            .encode_to_vec();
            let in_oneof_nested = [
                (0, OV::byte_slice(&in_oneof)),
                (1, OV::string("more data after that")),
            ]
            .into_opaque_message()
            .encode_to_vec();
            assert::decodes!(
                owned never decodes Outer<$ty>,
                &direct_nested,
                Truncated,
                "Outer.0/Foo.1",
            );
            assert::decodes!(
                owned never decodes Outer<$ty>,
                &in_oneof_nested,
                Truncated,
                "Outer.0/Foo.0/A.One",
            );
        };
    }

    check_fixed_truncation!(u32, OV::fixed_u32(0x1234abcd));
    check_fixed_truncation!(i32, OV::fixed_u32(0x1234abcd));
    check_fixed_truncation!(u64, OV::fixed_u64(0x1234deadbeefcafe));
    check_fixed_truncation!(i64, OV::fixed_u64(0x1234deadbeefcafe));
}

// String tests

fn bytes_for_surrogate(surrogate_codepoint: u32) -> [u8; 3] {
    assert!((0xd800..=0xdfff).contains(&surrogate_codepoint));
    [
        0b1110_0000 | (0b0000_1111 & (surrogate_codepoint >> 12)) as u8,
        0b10_000000 | (0b00_111111 & (surrogate_codepoint >> 6)) as u8,
        0b10_000000 | (0b00_111111 & surrogate_codepoint) as u8,
    ]
}

#[test]
fn parsing_strings() {
    #[derive(Debug, PartialEq, Eq, Message)]
    #[bilrost(distinguished)]
    struct Foo<T>(T);
    #[derive(Debug, PartialEq, Eq, Message)]
    #[bilrost(distinguished)]
    struct Plain<'a>(#[bilrost(encoding(plainbytes))] &'a [u8]);

    macro_rules! parsing_string_type {
        ($lifetime:ident $ty:ty) => {{
            assert::decodes!(
                $lifetime distinguished,
                [(0, OV::string("hello world"))],
                Foo::<$ty>("hello world".into()),
            );
            let mut invalid_strings = Vec::<Vec<u8>>::from([
                b"bad byte: \xff can't appear in utf-8".as_slice().into(),
                b"non-canonical representation \xc0\x80 of nul byte"
                    .as_slice()
                    .into(),
            ]);

            invalid_strings.extend((0xd800u32..=0xdfff).map(|surrogate_codepoint| {
                let mut invalid_with_surrogate: Vec<u8> = b"string with surrogate: "
                    .as_slice()
                    .into();
                invalid_with_surrogate.extend(bytes_for_surrogate(surrogate_codepoint));
                invalid_with_surrogate.extend(b" isn't valid");
                invalid_with_surrogate
            }));

            let mut surrogate_pair: Vec<u8> = b"surrogate pair: ".as_slice().into();
            surrogate_pair.extend(bytes_for_surrogate(0xd801));
            surrogate_pair.extend(bytes_for_surrogate(0xdc02));
            surrogate_pair.extend(b" is a valid surrogate pair");
            invalid_strings.push(surrogate_pair);

            let mut surrogate_pair: Vec<u8> = b"reversed surrogate pair: ".as_slice().into();
            surrogate_pair.extend(bytes_for_surrogate(0xdc02));
            surrogate_pair.extend(bytes_for_surrogate(0xd801));
            surrogate_pair.extend(b" is a backwards surrogate pair");
            invalid_strings.push(surrogate_pair);

            for invalid_string in invalid_strings {
                assert::decodes!(
                    $lifetime never decodes Foo<$ty>,
                    [(0, OV::byte_slice(&invalid_string))],
                    InvalidValue,
                    "Foo.0",
                );
                assert::decodes!(
                    borrowed distinguished,
                    [(0, OV::byte_slice(&invalid_string))],
                    Plain(invalid_string.as_slice()),
                );
            }
        }};
    }

    parsing_string_type!(owned String);
    parsing_string_type!(owned Cow<str>);
    #[cfg(feature = "bytestring")]
    parsing_string_type!(owned bytestring::ByteString);
    parsing_string_type!(borrowed & str);
}

#[test]
fn owned_empty_cow_str_is_still_empty() {
    let owned_empty = Cow::<str>::Owned(String::with_capacity(32));
    assert!(<() as EmptyState<(), _>>::is_empty(&owned_empty));

    #[derive(Message)]
    struct Foo<'a>(Cow<'a, str>);

    assert::encodes(Foo(Cow::Borrowed("")), []);
    assert::encodes(Foo(owned_empty), []);
}

// Blob tests

#[test]
fn parsing_blob() {
    #[derive(Debug, PartialEq, Eq, Message)]
    #[bilrost(distinguished)]
    struct Foo(bilrost::Blob);
    assert::decodes!(owned distinguished,
        [(0, OV::string("hello world"))],
        Foo(b"hello world"[..].into()),
    );
}

#[test]
fn parsing_vec_blob() {
    #[derive(Debug, PartialEq, Eq, Message)]
    #[bilrost(distinguished)]
    struct Foo(#[bilrost(encoding(plainbytes))] Vec<u8>);
    assert::decodes!(owned distinguished,
        [(0, OV::string("hello world"))],
        Foo(b"hello world"[..].into()),
    );
}

#[test]
fn parsing_cow_blob() {
    #[derive(Debug, PartialEq, Eq, Message)]
    #[bilrost(distinguished)]
    struct Foo<'a>(#[bilrost(encoding(plainbytes))] Cow<'a, [u8]>);
    assert::decodes!(owned distinguished,
        [(0, OV::string("hello world"))],
        Foo(b"hello world"[..].into()),
    );
}

#[test]
fn parsing_bytes_blob() {
    #[derive(Debug, PartialEq, Eq, Message)]
    #[bilrost(distinguished)]
    struct Foo(bytes::Bytes);
    assert::decodes!(owned distinguished,
        [(0, OV::string("hello world"))],
        Foo(b"hello world"[..].into()),
    );
}

#[test]
fn parsing_byte_arrays() {
    #[derive(Debug, PartialEq, Eq, Message)]
    #[bilrost(distinguished)]
    struct Foo<const N: usize>(#[bilrost(tag(1), encoding(plainbytes))] [u8; N]);

    assert::decodes!(owned distinguished, [], Foo([]));
    assert::decodes!(owned non-canonically, [(1, OV::bytes([]))], Foo([]), NotCanonical, "Foo.0");
    assert::decodes!(owned never decodes Foo<0>, [(1, OV::bytes([1]))], InvalidValue, "Foo.0");

    assert::decodes!(owned distinguished, [(1, OV::bytes([1, 2, 3, 4]))], Foo([1, 2, 3, 4]));
    assert::decodes!(
        owned non-canonically,
        [(1, OV::bytes([0; 4]))],
        Foo([0; 4]),
        NotCanonical,
        "Foo.0",
    );
    assert::decodes!(owned never decodes Foo<4>, [(1, OV::bytes([1; 3]))], InvalidValue, "Foo.0");
    assert::decodes!(owned never decodes Foo<4>, [(1, OV::bytes([1; 5]))], InvalidValue, "Foo.0");
    assert::decodes!(owned never decodes Foo<4>, [(1, OV::fixed_u32(1))], WrongWireType, "Foo.0");

    assert::decodes!(owned distinguished, [(1, OV::bytes([13; 13]))], Foo([13; 13]));
    assert::decodes!(
        owned non-canonically,
        [(1, OV::bytes([0; 13]))],
        Foo([0; 13]),
        NotCanonical,
        "Foo.0",
    );

    // Fixed-size wire types are implemented for appropriately sized u8 arrays
    #[derive(Debug, PartialEq, Eq, Message)]
    #[bilrost(distinguished)]
    struct Bar<const N: usize>(#[bilrost(tag(1), encoding(fixed))] [u8; N]);

    static_assertions::assert_not_impl_any!(Bar<0>: Message);
    static_assertions::assert_not_impl_any!(Bar<2>: Message);
    static_assertions::assert_not_impl_any!(Bar<16>: Message);
    assert::decodes!(owned distinguished, [(1, OV::fixed_u32(0x04030201))], Bar([1, 2, 3, 4]));
    assert::decodes!(
        owned non-canonically,
        [(1, OV::fixed_u32(0))], Bar([0; 4]), NotCanonical, "Bar.0");
    assert::decodes!(owned distinguished, [(1, OV::SixtyFourBit([8; 8]))], Bar([8; 8]));
    assert::decodes!(
        owned non-canonically,
        [(1, OV::SixtyFourBit([0; 8]))],
        Bar([0; 8]),
        NotCanonical,
        "Bar.0",
    );
    assert::decodes!(owned never decodes Bar<8>, [(1, OV::bytes([8; 8]))], WrongWireType, "Bar.0");
}

// Repeated field tests

#[test]
fn duplicated_field_decoding() {
    #[derive(Debug, PartialEq, Eq, Message)]
    #[bilrost(distinguished)]
    struct Foo(Option<bool>, bool);

    assert::decodes!(owned distinguished, [(0, OV::bool(false))], Foo(Some(false), false));
    assert::decodes!(
        owned never decodes Foo,
        [(0, OV::bool(false)), (0, OV::bool(true))],
        UnexpectedlyRepeated,
        "Foo.0",
    );
    assert::decodes!(owned distinguished, [(1, OV::bool(true))], Foo(None, true));
    assert::decodes!(
        owned never decodes Foo,
        [(1, OV::bool(true)), (1, OV::bool(false))],
        UnexpectedlyRepeated,
        "Foo.1",
    );
}

#[test]
fn duplicated_packed_decoding() {
    #[derive(Debug, PartialEq, Eq, Message)]
    #[bilrost(distinguished)]
    struct Foo(#[bilrost(encoding = "packed")] Vec<bool>);
    #[derive(Debug, PartialEq, Eq, Message)]
    #[bilrost(distinguished)]
    struct Bar(#[bilrost(encoding = "unpacked")] Vec<bool>);

    assert::decodes!(owned distinguished, [(0, OV::packed([OV::bool(true)]))], Foo(vec![true]));
    assert::decodes!(
        owned non-canonically,
        [(0, OV::packed([OV::bool(true)]))],
        Bar(vec![true]),
        NotCanonical,
        "Bar.0",
    );

    assert::decodes!(owned distinguished,
        [(0, OV::packed([OV::bool(true), OV::bool(false)]))],
        Foo(vec![true, false]),
    );
    assert::decodes!(
        owned non-canonically,
        [(0, OV::packed([OV::bool(true), OV::bool(false)]))],
        Bar(vec![true, false]),
        NotCanonical,
        "Bar.0",
    );

    // Two packed fields should never decode
    assert::decodes!(
        owned never decodes Foo,
        [
            (0, OV::packed([OV::bool(true), OV::bool(false)])),
            (0, OV::packed([OV::bool(false)])),
        ],
        UnexpectedlyRepeated,
        "Foo.0",
    );
    assert::decodes!(
        owned never decodes Bar,
        [
            (0, OV::packed([OV::bool(true), OV::bool(false)])),
            (0, OV::packed([OV::bool(false)])),
        ],
        UnexpectedlyRepeated,
        "Bar.0",
    );

    // Packed followed by unpacked should never decode
    assert::decodes!(
        owned never decodes Foo,
        [
            (0, OV::packed([OV::bool(true), OV::bool(false)])),
            (0, OV::bool(false)),
        ],
        UnexpectedlyRepeated,
        "Foo.0",
    );
    assert::decodes!(
        owned never decodes Bar,
        [
            (0, OV::packed([OV::bool(true), OV::bool(false)])),
            (0, OV::bool(false)),
        ],
        UnexpectedlyRepeated,
        "Bar.0",
    );

    // Unpacked followed by packed should never decode
    assert::decodes!(
        owned never decodes Foo,
        [
            (0, OV::bool(true)),
            (0, OV::bool(false)),
            (0, OV::packed([OV::bool(false)])),
        ],
        WrongWireType,
        "Foo.0",
    );
    assert::decodes!(
        owned never decodes Bar,
        [
            (0, OV::bool(true)),
            (0, OV::bool(false)),
            (0, OV::packed([OV::bool(false)])),
        ],
        WrongWireType,
        "Bar.0",
    );
}

// Map tests

#[test]
fn decoding_maps() {
    #[derive(Debug, PartialEq, Eq, Message)]
    #[bilrost(distinguished)]
    struct Foo<T>(T);

    let valid_map = &[(
        0,
        OV::packed([
            OV::bool(false),
            OV::string("no"),
            OV::bool(true),
            OV::string("yes"),
        ]),
    )];
    let disordered_map = &[(
        0,
        OV::packed([
            OV::bool(true),
            OV::string("yes"),
            OV::bool(false),
            OV::string("no"),
        ]),
    )];
    let repeated_map = &[(
        0,
        OV::packed([
            OV::bool(false),
            OV::string("indecipherable"),
            OV::bool(false),
            OV::string("could mean anything"),
        ]),
    )];

    {
        use std::collections::BTreeMap;
        assert::decodes!(
            owned distinguished,
            valid_map,
            Foo(BTreeMap::from([
                (false, "no".to_string()),
                (true, "yes".to_string()),
            ])),
        );
        assert::decodes!(
            owned non-canonically,
            disordered_map,
            Foo(BTreeMap::from([
                (false, "no".to_string()),
                (true, "yes".to_string()),
            ])),
            NotCanonical,
            "Foo.0",
        );
        assert::decodes!(
            owned never decodes Foo<BTreeMap<bool, String>>,
            repeated_map,
            UnexpectedlyRepeated,
            "Foo.0",
        );
    }
    #[allow(unused_macros)]
    macro_rules! test_hash {
        ($ty:ident) => {
            for map_value in [valid_map, disordered_map] {
                assert::decodes!(
                    owned relaxed,
                    map_value,
                    Foo($ty::from([
                        (false, "no".to_string()),
                        (true, "yes".to_string()),
                    ])),
                );
            }
            assert::decodes!(
                owned relaxed errs for Foo<$ty<bool, String>>,
                repeated_map,
                UnexpectedlyRepeated,
                "Foo.0",
            );
        };
    }
    #[cfg(feature = "std")]
    {
        use std::collections::HashMap;
        test_hash!(HashMap);
    }
    #[cfg(feature = "hashbrown")]
    {
        use hashbrown::HashMap;
        test_hash!(HashMap);
    }
}

#[cfg(feature = "std")]
#[test]
fn custom_hashers_std() {
    use core::hash::{BuildHasher, Hasher};

    #[derive(Default)]
    struct CustomHasher(<std::collections::hash_map::RandomState as BuildHasher>::Hasher);

    impl Hasher for CustomHasher {
        fn finish(&self) -> u64 {
            self.0.finish().wrapping_add(1)
        }

        fn write(&mut self, bytes: &[u8]) {
            self.0.write(bytes);
        }
    }

    type MapType =
        std::collections::HashMap<i32, i32, core::hash::BuildHasherDefault<CustomHasher>>;
    type SetType = std::collections::HashSet<i32, core::hash::BuildHasherDefault<CustomHasher>>;

    #[derive(Debug, PartialEq, Message)]
    struct Foo {
        map: MapType,
        set: SetType,
    }

    assert::decodes!(
        owned relaxed,
        [
            (
                1,
                OV::packed([OV::i32(1), OV::i32(10), OV::i32(2), OV::i32(20)]),
            ),
            (2, OV::i32(2)),
            (2, OV::i32(3)),
            (2, OV::i32(5)),
            (2, OV::i32(8)),
        ],
        Foo {
            map: [(1, 10), (2, 20)].into_iter().collect(),
            set: [2, 3, 5, 8].into_iter().collect(),
        },
    );
}

#[cfg(feature = "hashbrown")]
#[test]
fn custom_hashers_hashbrown() {
    use core::hash::{BuildHasher, Hasher};

    #[derive(Default)]
    struct CustomHashBuilder(hashbrown::DefaultHashBuilder);
    struct CustomHasher(<hashbrown::DefaultHashBuilder as BuildHasher>::Hasher);

    impl BuildHasher for CustomHashBuilder {
        type Hasher = CustomHasher;

        fn build_hasher(&self) -> Self::Hasher {
            CustomHasher(self.0.build_hasher())
        }
    }

    impl Hasher for CustomHasher {
        fn finish(&self) -> u64 {
            self.0.finish().wrapping_add(1)
        }

        fn write(&mut self, bytes: &[u8]) {
            self.0.write(bytes);
        }
    }

    type MapType = hashbrown::HashMap<i32, i32, CustomHashBuilder>;
    type SetType = hashbrown::HashSet<i32, CustomHashBuilder>;

    #[derive(Debug, PartialEq, Message)]
    struct Foo {
        map: MapType,
        set: SetType,
    }

    assert::decodes!(
        owned relaxed,
        [
            (
                1,
                OV::packed([OV::i32(1), OV::i32(10), OV::i32(2), OV::i32(20)]),
            ),
            (2, OV::i32(2)),
            (2, OV::i32(3)),
            (2, OV::i32(5)),
            (2, OV::i32(8)),
        ],
        Foo {
            map: [(1, 10), (2, 20)].into_iter().collect(),
            set: [2, 3, 5, 8].into_iter().collect(),
        },
    );
}

#[test]
fn truncated_map() {
    #[derive(Debug, PartialEq, Eq, Message)]
    #[bilrost(distinguished)]
    struct Foo<T>(T, String);

    let OV::LengthDelimited(map_value) = OV::packed([
        OV::bool(false),
        OV::string("no"),
        OV::bool(true),
        OV::string("yes"),
    ]) else {
        unreachable!()
    };
    let truncated_bool_string_map = &map_value[..map_value.len() - 1];

    let OV::LengthDelimited(map_value) = OV::packed([
        OV::string("zero"),
        OV::u64(0),
        OV::string("lots"),
        OV::u64(999999999999999),
    ]) else {
        unreachable!()
    };
    let truncated_string_int_map = &map_value[..map_value.len() - 1];

    macro_rules! truncated_map_t {
        (relaxed $($ty:tt)*) => {
            truncated_map_t!(($($ty)*) relaxed errs for);
        };
        (distinguished $($ty:tt)*) => {
            truncated_map_t!(($($ty)*) never decodes);
        };
        (($($ty:tt)*) $($mode_words:tt)*) => {
            assert::decodes!(
                owned $($mode_words)* Foo<$($ty)*<bool, String>>,
                [
                    (0, OV::byte_slice(truncated_bool_string_map)),
                    (1, OV::string("another field after that")),
                ],
                Truncated,
                "Foo.0",
            );
            assert::decodes!(
                borrowed $($mode_words)* Foo<$($ty)*<bool, &str>>,
                [
                    (0, OV::byte_slice(truncated_bool_string_map)),
                    (1, OV::string("another field after that")),
                ],
                Truncated,
                "Foo.0",
            );
            assert::decodes!(
                owned $($mode_words)* Foo<$($ty)*<String, u64>>,
                [
                    (0, OV::byte_slice(truncated_string_int_map)),
                    (1, OV::string("another field after that")),
                ],
                Truncated,
                "Foo.0",
            );
            assert::decodes!(
                borrowed $($mode_words)* Foo<$($ty)*<&str, u64>>,
                [
                    (0, OV::byte_slice(truncated_string_int_map)),
                    (1, OV::string("another field after that")),
                ],
                Truncated,
                "Foo.0",
            );
        };
    }

    truncated_map_t!(distinguished std::collections::BTreeMap);
    #[cfg(feature = "std")]
    truncated_map_t!(relaxed std::collections::HashMap);
    #[cfg(feature = "hashbrown")]
    truncated_map_t!(relaxed hashbrown::HashMap);
}

// Vec tests

#[test]
fn decoding_vecs_and_cows() {
    #[derive(Debug, PartialEq, Eq, Message)]
    #[bilrost(distinguished)]
    struct Foo<T> {
        #[bilrost(encoding(packed))]
        packed: T,
        #[bilrost(encoding(unpacked))]
        unpacked: T,
    }

    let values = [
        (vec![OV::string("foo")], vec!["foo"]),
        (
            vec![
                OV::string("bar"),
                OV::string("baz"),
                OV::string("bear"),
                OV::string("wabl"),
            ],
            vec!["bar", "baz", "bear", "wabl"],
        ),
    ];
    for (ref packed, ref unpacked, expected) in values.map(|(items, expected)| {
        (
            // One packed field with all the values
            [(1, OV::packed(items.iter().cloned()))].into_opaque_message(),
            // Unpacked fields for each value
            OpaqueMessage::from_iter(items.iter().map(|item| (2, item.clone()))),
            expected.into_iter().map(str::to_string).collect::<Vec<_>>(),
        )
    }) {
        assert::decodes!(
            owned distinguished,
            packed,
            Foo {
                packed: expected.clone(),
                unpacked: vec![],
            },
        );
        assert::decodes!(
            owned distinguished,
            unpacked,
            Foo {
                packed: vec![],
                unpacked: expected.clone(),
            },
        );
        assert::decodes!(
            owned distinguished,
            packed,
            Foo {
                packed: Cow::Borrowed(expected.as_slice()),
                unpacked: Cow::default(),
            },
        );
        assert::decodes!(
            owned distinguished,
            unpacked,
            Foo {
                packed: Cow::default(),
                unpacked: Cow::Borrowed(expected.as_slice()),
            },
        );
        assert::decodes!(
            owned distinguished,
            packed,
            Foo {
                packed: Cow::Owned(expected.clone()),
                unpacked: Cow::default(),
            },
        );
        assert::decodes!(
            owned distinguished,
            unpacked,
            Foo {
                packed: Cow::default(),
                unpacked: Cow::Owned(expected.clone()),
            },
        );

        // Detour! Test vec of cow also, and check that it decodes in the right borrowingness
        let packed_buf = packed.encode_to_vec();
        let encoded_packed = packed_buf.as_slice();
        let expected_foo = Foo {
            packed: expected.iter().cloned().map(Into::into).collect(),
            unpacked: vec![],
        };

        {
            let owned_cows = Foo::<Vec<Cow<str>>>::decode(encoded_packed)
                .expect("did not decode into vec of cows");
            assert_eq!(owned_cows, expected_foo);
            for cow in owned_cows.packed {
                assert!(matches!(cow, Cow::Owned(..)));
            }
        }

        {
            let owned_cows = Foo::<Vec<Cow<str>>>::decode_canonical(encoded_packed)
                .expect("did not decode canonically into vec of cows");
            assert_eq!(owned_cows, expected_foo);
            for cow in owned_cows.packed {
                assert!(matches!(cow, Cow::Owned(..)));
            }
        }

        {
            let borrowed_cows = Foo::<Vec<Cow<str>>>::decode_borrowed(encoded_packed)
                .expect("did not decode borrowed into vec of cows");
            assert_eq!(borrowed_cows, expected_foo);
            for cow in borrowed_cows.packed {
                assert!(matches!(cow, Cow::Borrowed(..)));
            }
        }

        {
            let borrowed_cows = Foo::<Vec<Cow<str>>>::decode_canonical_borrowed(encoded_packed)
                .expect("did not decode canonically borrowed into vec of cows");
            assert_eq!(borrowed_cows, expected_foo);
            for cow in borrowed_cows.packed {
                assert!(matches!(cow, Cow::Borrowed(..)));
            }
        }

        #[allow(unused_macros)]
        macro_rules! test_vec {
            ($vec_ty:ty) => {
                assert::decodes!(
                    owned distinguished,
                    packed,
                    Foo {
                        packed: expected.iter().cloned().collect(),
                        unpacked: <$vec_ty>::new(),
                    },
                );
                assert::decodes!(
                    owned distinguished,
                    unpacked,
                    Foo {
                        packed: <$vec_ty>::new(),
                        unpacked: expected.iter().cloned().collect(),
                    },
                );
            };
        }
        #[cfg(feature = "arrayvec")]
        test_vec!(arrayvec::ArrayVec<String, 4>);
        #[cfg(feature = "smallvec")]
        test_vec!(smallvec::SmallVec<[String; 2]>);
        #[cfg(feature = "thin-vec")]
        test_vec!(thin_vec::ThinVec<String>);
        #[cfg(feature = "tinyvec")]
        test_vec!(tinyvec::ArrayVec<[String; 4]>);
        #[cfg(feature = "tinyvec")]
        test_vec!(tinyvec::TinyVec<[String; 2]>);
    }
}

#[test]
fn decoding_vecs_with_swapped_packedness() {
    #[derive(Debug, PartialEq, Eq, Message)]
    #[bilrost(distinguished)]
    struct Oof<T> {
        // Fields have swapped packedness from `Foo` above
        #[bilrost(encoding(unpacked))]
        unpacked: T,
        #[bilrost(encoding(packed))]
        packed: T,
    }

    let values = [
        (vec![OV::u32(1)], vec![1u32]),
        (
            vec![
                OV::u32(1),
                OV::u32(1),
                OV::u32(2),
                OV::u32(3),
                OV::u32(5),
                OV::u32(8),
            ],
            vec![1, 1, 2, 3, 5, 8],
        ),
    ];

    // In relaxed mode, packed sets will decode unpacked values and vice versa, but this is
    // only detectable when the values are not length-delimited.
    for (ref packed, ref unpacked, expected) in values.map(|(items, expected)| {
        (
            // One packed field with all the values
            [(1, OV::packed(items.iter().cloned()))].into_opaque_message(),
            // Unpacked fields for each value
            OpaqueMessage::from_iter(items.iter().map(|item| (2, item.clone()))),
            expected,
        )
    }) {
        assert::decodes!(
            owned non-canonically,
            packed,
            Oof {
                unpacked: expected.clone(),
                packed: vec![],
            },
            NotCanonical,
            "Oof.unpacked",
        );
        assert::decodes!(
            owned non-canonically,
            unpacked,
            Oof {
                unpacked: vec![],
                packed: expected.clone(),
            },
            NotCanonical,
            "Oof.packed",
        );
        assert::decodes!(
            owned non-canonically,
            packed,
            Oof {
                unpacked: Cow::Borrowed(expected.as_slice()),
                packed: Cow::default(),
            },
            NotCanonical,
            "Oof.unpacked",
        );
        assert::decodes!(
            owned non-canonically,
            unpacked,
            Oof {
                unpacked: Cow::default(),
                packed: Cow::Borrowed(expected.as_slice()),
            },
            NotCanonical,
            "Oof.packed",
        );
        assert::decodes!(
            owned non-canonically,
            packed,
            Oof {
                unpacked: Cow::Owned(expected.clone()),
                packed: Cow::default(),
            },
            NotCanonical,
            "Oof.unpacked",
        );
        assert::decodes!(
            owned non-canonically,
            unpacked,
            Oof {
                unpacked: Cow::default(),
                packed: Cow::Owned(expected.clone()),
            },
            NotCanonical,
            "Oof.packed",
        );
        #[allow(unused_macros)]
        macro_rules! test_vec {
            ($vec_ty:ty) => {
                assert::decodes!(
                    owned non-canonically,
                    packed,
                    Oof {
                        unpacked: expected.iter().cloned().collect(),
                        packed: <$vec_ty>::new(),
                    },
                    NotCanonical,
                    "Oof.unpacked",
                );
                assert::decodes!(
                    owned non-canonically,
                    unpacked,
                    Oof {
                        unpacked: <$vec_ty>::new(),
                        packed: expected.iter().cloned().collect(),
                    },
                    NotCanonical,
                    "Oof.packed",
                );
            };
        }
        #[cfg(feature = "arrayvec")]
        test_vec!(arrayvec::ArrayVec<u32, 6>);
        #[cfg(feature = "smallvec")]
        test_vec!(smallvec::SmallVec<[u32; 2]>);
        #[cfg(feature = "thin-vec")]
        test_vec!(thin_vec::ThinVec<u32>);
        #[cfg(feature = "tinyvec")]
        test_vec!(tinyvec::ArrayVec<[u32; 6]>);
        #[cfg(feature = "tinyvec")]
        test_vec!(tinyvec::TinyVec<[u32; 2]>);
    }
}

// Fixed-size array tests

fn decoding_arrays_of_size<const N: usize>() {
    #[derive(Debug, PartialEq, Eq, Message)]
    #[bilrost(distinguished)]
    struct Foo<T> {
        #[bilrost(encoding(packed))]
        packed: T,
        #[bilrost(encoding(unpacked))]
        unpacked: T,
    }

    let with_values = ["foo"; N].map(String::from); // with_values all have non-empty values
    let empty = [""; N].map(String::from); // empty are all empty
    let mut almost_empty = [""; N].map(String::from); // almost_empty has 1 non-empty value, last
    almost_empty[N - 1] = "foo".to_string();
    let almost_empty = almost_empty;
    assert::decodes!(
        owned distinguished,
        [(1, OV::packed(vec![OV::str("foo"); N]))],
        Foo {
            packed: with_values.clone(),
            unpacked: empty.clone(),
        },
    );
    assert::decodes!(
        owned distinguished,
        vec![(2, OV::str("foo")); N].as_slice(),
        Foo {
            packed: empty.clone(),
            unpacked: with_values.clone(),
        },
    );
    assert::decodes!(
        owned distinguished,
        [(1, OV::packed(almost_empty.iter().map(|s| OV::str(s))))],
        Foo {
            packed: almost_empty.clone(),
            unpacked: empty.clone(),
        },
    );
    assert::decodes!(
        owned distinguished,
        almost_empty.iter().map(|s| (2, OV::str(s))),
        Foo {
            packed: empty.clone(),
            unpacked: almost_empty.clone(),
        },
    );
}

fn decoding_arrays_with_swapped_packedness_of_size<const N: usize>() {
    #[derive(Debug, PartialEq, Eq, Message)]
    #[bilrost(distinguished)]
    struct Oof<T> {
        #[bilrost(encoding(unpacked))]
        unpacked: T,
        #[bilrost(encoding(packed))]
        packed: T,
    }

    let with_values = [5; N]; // with_values all have non-empty values
    let empty = [0; N]; // empty are all empty
    let mut almost_empty = [0; N]; // almost_empty has 1 non-empty value, last
    almost_empty[N - 1] = 5;
    let almost_empty = almost_empty;

    // In relaxed mode, packed arrays will decode unpacked values and vice versa, but this is
    // only detectable when the values are not length-delimited.
    assert::decodes!(
        owned non-canonically,
        [(1, OV::packed(vec![OV::i32(5); N]))],
        Oof {
            unpacked: with_values,
            packed: empty,
        },
        NotCanonical,
        "Oof.unpacked",
    );
    assert::decodes!(
        owned non-canonically,
        with_values.iter().map(|&i| (2, OV::i32(i))),
        Oof {
            unpacked: empty,
            packed: with_values,
        },
        NotCanonical,
        "Oof.packed",
    );
    assert::decodes!(
        owned non-canonically,
        [(1, OV::packed(almost_empty.iter().map(|&i| OV::i32(i))))],
        Oof {
            unpacked: almost_empty,
            packed: empty,
        },
        NotCanonical,
        "Oof.unpacked",
    );
    assert::decodes!(
        owned non-canonically,
        almost_empty.iter().map(|&i| (2, OV::i32(i))),
        Oof {
            unpacked: empty,
            packed: almost_empty,
        },
        NotCanonical,
        "Oof.packed",
    );
}

#[test]
fn decoding_arrays() {
    decoding_arrays_of_size::<1>();
    decoding_arrays_of_size::<2>();
    decoding_arrays_of_size::<3>();
    decoding_arrays_of_size::<5>();
    decoding_arrays_of_size::<8>();
    decoding_arrays_with_swapped_packedness_of_size::<1>();
    decoding_arrays_with_swapped_packedness_of_size::<2>();
    decoding_arrays_with_swapped_packedness_of_size::<3>();
    decoding_arrays_with_swapped_packedness_of_size::<5>();
    decoding_arrays_with_swapped_packedness_of_size::<8>();

    #[derive(Debug, PartialEq, Eq, Message)]
    #[bilrost(distinguished)]
    struct FooGeneral<T> {
        #[bilrost(encoding(packed))]
        packed: T,
        #[bilrost(encoding(unpacked))]
        unpacked: T,
    }
    #[derive(Debug, PartialEq, Eq, Message)]
    #[bilrost(distinguished)]
    struct FooFixed<T> {
        #[bilrost(encoding(packed<fixed>))]
        packed: T,
        #[bilrost(encoding(unpacked<fixed>))]
        unpacked: T,
    }

    // Packed with wrong numbers of values:
    // Too few values
    assert::decodes!(
        owned never decodes FooGeneral<[String; 2]>,
        [(1, OV::packed([OV::str("foo")]))],
        InvalidValue,
        "FooGeneral.packed",
    );
    assert::decodes!(
        owned never decodes FooGeneral<[i32; 2]>,
        [(1, OV::packed([OV::i32(1)]))],
        InvalidValue,
        "FooGeneral.packed",
    );
    assert::decodes!(
        owned never decodes FooFixed<[u64; 2]>,
        [(1, OV::packed([OV::fixed_u64(1)]))],
        InvalidValue,
        "FooFixed.packed",
    );
    assert::decodes!(
        owned never decodes FooGeneral<Option<[i32; 2]>>,
        [(1, OV::packed([OV::i32(1)]))],
        InvalidValue,
        "FooGeneral.packed",
    );
    // Too many values
    assert::decodes!(
        owned never decodes FooGeneral<[String; 2]>,
        [(
            1,
            OV::packed([OV::str("foo"), OV::str("bar"), OV::str("baz")]),
        )],
        InvalidValue,
        "FooGeneral.packed",
    );
    assert::decodes!(
        owned never decodes FooGeneral<[i32; 2]>,
        [(1, OV::packed([OV::i32(3), OV::i32(4), OV::i32(5)]))],
        InvalidValue,
        "FooGeneral.packed",
    );
    assert::decodes!(
        owned never decodes FooFixed<[u64; 2]>,
        [(
            1,
            OV::packed([OV::fixed_u64(3), OV::fixed_u64(4), OV::fixed_u64(5)]),
        )],
        InvalidValue,
        "FooFixed.packed",
    );
    assert::decodes!(
        owned never decodes FooGeneral<Option<[i32; 2]>>,
        [(1, OV::packed([OV::i32(3), OV::i32(4), OV::i32(5)]))],
        InvalidValue,
        "FooGeneral.packed",
    );
    // Just right
    assert::decodes!(
        owned distinguished,
        [(1, OV::packed([OV::str("foo"), OV::str("bar")]))],
        FooGeneral {
            packed: ["foo".to_string(), "bar".to_string()],
            unpacked: ["".to_string(), "".to_string()],
        },
    );
    assert::decodes!(
        owned distinguished,
        [(1, OV::packed([OV::i32(3), OV::i32(4)]))],
        FooGeneral {
            packed: [3i32, 4],
            unpacked: [0, 0],
        },
    );
    assert::decodes!(
        owned non-canonically,
        [(1, OV::packed([OV::i32(0), OV::i32(0)]))],
        FooGeneral {
            packed: [0i32, 0],
            unpacked: [0, 0],
        },
        NotCanonical,
        "FooGeneral.packed",
    );
    assert::decodes!(
        owned distinguished,
        [(1, OV::packed([OV::fixed_u64(3), OV::fixed_u64(4)]))],
        FooFixed {
            packed: [3u64, 4],
            unpacked: [0, 0],
        },
    );
    assert::decodes!(
        owned distinguished,
        [(1, OV::packed([OV::i32(3), OV::i32(4)]))],
        FooGeneral {
            packed: Some([3i32, 4]),
            unpacked: None,
        },
    );
    assert::decodes!(
        owned distinguished,
        [(1, OV::packed([OV::i32(0), OV::i32(0)]))],
        FooGeneral {
            packed: Some([0i32, 0]),
            unpacked: None,
        },
    );

    // Unpacked with wrong numbers of values:
    // Too few values
    assert::decodes!(
        owned never decodes FooGeneral<[String; 2]>,
        [(2, OV::str("foo"))],
        InvalidValue,
        "FooGeneral.unpacked",
    );
    assert::decodes!(
        owned never decodes FooGeneral<[i32; 2]>,
        [(2, OV::i32(1))],
        InvalidValue,
        "FooGeneral.unpacked",
    );
    assert::decodes!(
        owned never decodes FooFixed<[u64; 2]>,
        [(2, OV::fixed_u64(1))],
        InvalidValue,
        "FooFixed.unpacked",
    );
    assert::decodes!(
        owned never decodes FooGeneral<Option<[i32; 2]>>,
        [(2, OV::i32(1))],
        InvalidValue,
        "FooGeneral.unpacked",
    );
    // Too many values
    assert::decodes!(
        owned never decodes FooGeneral<[String; 2]>,
        [
            (2, OV::str("foo")),
            (2, OV::str("bar")),
            (2, OV::str("baz")),
        ],
        InvalidValue,
        "FooGeneral.unpacked",
    );
    assert::decodes!(
        owned never decodes FooGeneral<[i32; 2]>,
        [(2, OV::i32(3)), (2, OV::i32(4)), (2, OV::i32(5))],
        InvalidValue,
        "FooGeneral.unpacked",
    );
    assert::decodes!(
        owned never decodes FooFixed<[u64; 2]>,
        [
            (2, OV::fixed_u64(3)),
            (2, OV::fixed_u64(4)),
            (2, OV::fixed_u64(5)),
        ],
        InvalidValue,
        "FooFixed.unpacked",
    );
    assert::decodes!(
        owned never decodes FooGeneral<Option<[i32; 2]>>,
        [(2, OV::i32(3)), (2, OV::i32(4)), (2, OV::i32(5))],
        InvalidValue,
        "FooGeneral.unpacked",
    );
    // Just right
    assert::decodes!(
        owned distinguished,
        [(2, OV::str("foo")), (2, OV::str("bar"))],
        FooGeneral {
            packed: ["".to_string(), "".to_string()],
            unpacked: ["foo".to_string(), "bar".to_string()],
        },
    );
    assert::decodes!(
        owned distinguished,
        [(2, OV::i32(3)), (2, OV::i32(4))],
        FooGeneral {
            packed: [0i32, 0],
            unpacked: [3, 4],
        },
    );
    assert::decodes!(
        owned non-canonically,
        [(2, OV::i32(0)), (2, OV::i32(0))],
        FooGeneral {
            packed: [0i32, 0],
            unpacked: [0, 0],
        },
        NotCanonical,
        "FooGeneral.unpacked",
    );
    assert::decodes!(
        owned distinguished,
        [(2, OV::fixed_u64(3)), (2, OV::fixed_u64(4))],
        FooFixed {
            packed: [0u64, 0],
            unpacked: [3, 4],
        },
    );
    assert::decodes!(
        owned distinguished,
        [(2, OV::i32(3)), (2, OV::i32(4))],
        FooGeneral {
            packed: None,
            unpacked: Some([3i32, 4]),
        },
    );
    assert::decodes!(
        owned distinguished,
        [(2, OV::i32(0)), (2, OV::i32(0))],
        FooGeneral {
            packed: None,
            unpacked: Some([0i32, 0]),
        },
    );
}

// Set tests

#[test]
fn decoding_sets() {
    #[derive(Debug, PartialEq, Eq, Message)]
    #[bilrost(distinguished)]
    struct Foo<T> {
        #[bilrost(encoding(packed))]
        packed: T,
        #[bilrost(encoding(unpacked))]
        unpacked: T,
    }

    let valid_set_items = [OV::string("bar"), OV::string("baz"), OV::string("foo")];
    let disordered_set_items = [OV::string("foo"), OV::string("bar"), OV::string("baz")];
    let repeated_set_items = [
        OV::string("a value"),
        OV::string("repeated"),
        OV::string("repeated"),
        OV::string("incorrectly"),
    ];
    // Turn each of these lists of items into a packed field and an unpacked field with those
    // values in the same order.
    let [valid, disordered, repeated] = [
        &valid_set_items[..],
        &disordered_set_items,
        &repeated_set_items,
    ]
    .map(|items| {
        (
            // One packed field with all the values
            [(1, OV::packed(items.iter().cloned()))].into_opaque_message(),
            // Unpacked fields for each value
            OpaqueMessage::from_iter(items.iter().map(|item| (2, item.clone()))),
        )
    });
    let (valid_set_packed, valid_set_unpacked) = &valid;
    let (disordered_set_packed, disordered_set_unpacked) = &disordered;
    let (repeated_set_packed, repeated_set_unpacked) = &repeated;

    let expected_items = ["foo".to_string(), "bar".to_string(), "baz".to_string()];

    {
        use std::collections::BTreeSet;
        assert::decodes!(
            owned distinguished,
            valid_set_packed,
            Foo {
                packed: BTreeSet::from(expected_items.clone()),
                unpacked: BTreeSet::new(),
            },
        );
        assert::decodes!(
            owned distinguished,
            valid_set_unpacked,
            Foo {
                packed: BTreeSet::new(),
                unpacked: BTreeSet::from(expected_items.clone()),
            },
        );
        assert::decodes!(
            owned non-canonically,
            disordered_set_packed,
            Foo {
                packed: BTreeSet::from(expected_items.clone()),
                unpacked: BTreeSet::new(),
            },
            NotCanonical,
            "Foo.packed",
        );
        assert::decodes!(
            owned non-canonically,
            disordered_set_unpacked,
            Foo {
                packed: BTreeSet::new(),
                unpacked: BTreeSet::from(expected_items.clone()),
            },
            NotCanonical,
            "Foo.unpacked",
        );
        assert::decodes!(
            owned never decodes Foo<BTreeSet<String>>,
            &repeated_set_packed,
            UnexpectedlyRepeated,
            "Foo.packed",
        );
        assert::decodes!(
            owned never decodes Foo<BTreeSet<String>>,
            &repeated_set_unpacked,
            UnexpectedlyRepeated,
            "Foo.unpacked",
        );
    }
    #[allow(unused_macros)]
    macro_rules! test_hash {
        ($ty:ident) => {
            for (set_value_packed, set_value_unpacked) in [&valid, &disordered] {
                assert::decodes!(
                    owned relaxed,
                    set_value_packed,
                    Foo {
                        packed: $ty::from(expected_items.clone()),
                        unpacked: $ty::new(),
                    },
                );
                assert::decodes!(
                    owned relaxed,
                    set_value_unpacked,
                    Foo {
                        packed: $ty::new(),
                        unpacked: $ty::from(expected_items.clone()),
                    },
                );
            }
            assert::decodes!(
                owned relaxed errs for Foo<$ty<String>>,
                repeated_set_packed,
                UnexpectedlyRepeated,
                "Foo.packed",
            );
            assert::decodes!(
                owned relaxed errs for Foo<$ty<String>>,
                repeated_set_unpacked,
                UnexpectedlyRepeated,
                "Foo.unpacked",
            );
        };
    }
    #[cfg(feature = "std")]
    {
        use std::collections::HashSet;
        test_hash!(HashSet);
    }
    #[cfg(feature = "hashbrown")]
    {
        use hashbrown::HashSet;
        test_hash!(HashSet);
    }
}

#[test]
fn decoding_sets_with_swapped_packedness() {
    #[derive(Debug, PartialEq, Eq, Message)]
    #[bilrost(distinguished)]
    struct Oof<T> {
        #[bilrost(encoding(unpacked))]
        unpacked: T, // Fields have swapped packedness from `Foo` above
        #[bilrost(encoding(packed))]
        packed: T,
    }

    let valid_set_items = [OV::u32(1), OV::u32(2), OV::u32(3)];
    let disordered_set_items = [OV::u32(2), OV::u32(3), OV::u32(1)];
    let repeated_set_items = [OV::u32(1), OV::u32(2), OV::u32(2), OV::u32(3)];
    // Turn each of these lists of items into a packed field and an unpacked field with those
    // values in the same order.
    let [valid, disordered, repeated] = [
        &valid_set_items[..],
        &disordered_set_items,
        &repeated_set_items,
    ]
    .map(|items| {
        (
            // One packed field with all the values
            [(1, OV::packed(items.iter().cloned()))].into_opaque_message(),
            // Unpacked fields for each value
            OpaqueMessage::from_iter(items.iter().map(|item| (2, item.clone()))),
        )
    });
    let (repeated_set_packed, repeated_set_unpacked) = &repeated;
    let expected_items = [1u32, 2, 3];

    // In relaxed mode, packed sets will decode unpacked values and vice versa, but this is
    // only detectable when the values are not length-delimited.
    {
        use std::collections::BTreeSet;
        for (unmatching_packed, unmatching_unpacked) in [&valid, &disordered] {
            assert::decodes!(
                owned non-canonically,
                unmatching_packed,
                Oof {
                    unpacked: BTreeSet::from(expected_items),
                    packed: BTreeSet::new(),
                },
                NotCanonical,
                "Oof.unpacked",
            );
            assert::decodes!(
                owned non-canonically,
                unmatching_unpacked,
                Oof {
                    unpacked: BTreeSet::new(),
                    packed: BTreeSet::from(expected_items),
                },
                NotCanonical,
                "Oof.packed",
            );
        }
        assert::decodes!(
            owned never decodes Oof<BTreeSet<u32>>,
            &repeated_set_packed,
            UnexpectedlyRepeated,
            "Oof.unpacked",
        );
        assert::decodes!(
            owned never decodes Oof<BTreeSet<u32>>,
            &repeated_set_unpacked,
            UnexpectedlyRepeated,
            "Oof.packed",
        );
    }
    #[allow(unused_macros)]
    macro_rules! test_hash {
        ($ty:ident) => {
            for (set_value_packed, set_value_unpacked) in [&valid, &disordered] {
                assert::decodes!(
                    owned relaxed,
                    set_value_packed,
                    Oof {
                        unpacked: $ty::from(expected_items.clone()),
                        packed: $ty::new(),
                    },
                );
                assert::decodes!(
                    owned relaxed,
                    set_value_unpacked,
                    Oof {
                        unpacked: $ty::new(),
                        packed: $ty::from(expected_items.clone()),
                    },
                );
            }
            assert::decodes!(
                owned relaxed errs for Oof<$ty<u32>>,
                repeated_set_packed,
                UnexpectedlyRepeated,
                "Oof.unpacked",
            );
            assert::decodes!(
                owned relaxed errs for Oof<$ty<u32>>,
                repeated_set_unpacked,
                UnexpectedlyRepeated,
                "Oof.packed",
            );
        };
    }
    #[cfg(feature = "std")]
    {
        use std::collections::HashSet;
        test_hash!(HashSet);
    }
    #[cfg(feature = "hashbrown")]
    {
        use hashbrown::HashSet;
        test_hash!(HashSet);
    }
}

#[test]
fn truncated_packed_collection() {
    #[derive(Debug, PartialEq, Eq, Message)]
    #[bilrost(distinguished)]
    struct Foo<T>(#[bilrost(encoding(packed))] T, String);
    #[derive(Debug, PartialEq, Eq, Message)]
    #[bilrost(distinguished)]
    struct FooPlainbytes<T>(#[bilrost(encoding(packed<plainbytes>))] T, String);

    let OV::LengthDelimited(set_value) = OV::packed([OV::string("fooble"), OV::string("barbaz")])
    else {
        unreachable!()
    };
    let truncated_packed_string_val = &set_value[..set_value.len() - 1];

    let OV::LengthDelimited(set_value) = OV::packed([OV::u64(0), OV::u64(999999999999999)]) else {
        unreachable!()
    };
    let truncated_packed_int_val = &set_value[..set_value.len() - 1];

    macro_rules! truncated_collection_t {
        (
            relaxed,
            owned ($($owned_ty:tt)*),
            borrowed ($($borrowed_ty:tt)*) $(,)?
        ) => {
            truncated_collection_t!(($($owned_ty)*), ($($borrowed_ty)*) relaxed errs for);
        };
        (
            distinguished,
            owned ($($owned_ty:tt)*),
            borrowed ($($borrowed_ty:tt)*) $(,)?
        ) => {
            truncated_collection_t!(($($owned_ty)*), ($($borrowed_ty)*) never decodes);
        };
        (($($owned_ty:tt)*), ($($borrowed_ty:tt)*) $($mode_words:tt)*) => {
            {
                type T = String;
                assert::decodes!(
                    owned $($mode_words)* Foo<$($owned_ty)*>,
                    [
                        (0, OV::byte_slice(truncated_packed_string_val)),
                        (1, OV::string("another field after that")),
                    ],
                    Truncated,
                    "Foo.0",
                );
            }
            {
                type T = [u8];
                assert::decodes!(
                    borrowed $($mode_words)* FooPlainbytes<$($borrowed_ty)*>,
                    [
                        (0, OV::byte_slice(truncated_packed_string_val)),
                        (1, OV::string("another field after that")),
                    ],
                    Truncated,
                    "FooPlainbytes.0",
                );
            }
            #[cfg(feature = "bstr")]
            {
                type T = bstr::BStr;
                assert::decodes!(
                    borrowed $($mode_words)* Foo<$($borrowed_ty)*>,
                    [
                        (0, OV::byte_slice(truncated_packed_string_val)),
                        (1, OV::string("another field after that")),
                    ],
                    Truncated,
                    "Foo.0",
                );
            }
            {
                type T = u64;
                assert::decodes!(
                    owned $($mode_words)* Foo<$($owned_ty)*>,
                    [
                        (0, OV::byte_slice(truncated_packed_int_val)),
                        (1, OV::string("another field after that")),
                    ],
                    Truncated,
                    "Foo.0",
                );
            }
        };
    }

    truncated_collection_t!(distinguished, owned(Vec<T>), borrowed(Vec<&T>),);
    truncated_collection_t!(distinguished, owned(Cow<[T]>), borrowed(Cow<[&T]>),);
    #[cfg(feature = "arrayvec")]
    truncated_collection_t!(
        distinguished,
        owned(arrayvec::ArrayVec<T, 2>),
        borrowed(arrayvec::ArrayVec<&T, 2>),
    );
    #[cfg(feature = "smallvec")]
    truncated_collection_t!(
        distinguished,
        owned(smallvec::SmallVec<[T; 2]>),
        borrowed(smallvec::SmallVec<[&T; 2]>),
    );
    #[cfg(feature = "thin-vec")]
    truncated_collection_t!(
        distinguished,
        owned(thin_vec::ThinVec<T>),
        borrowed(thin_vec::ThinVec<&T>),
    );
    #[cfg(feature = "tinyvec")]
    truncated_collection_t!(
        distinguished,
        owned(tinyvec::ArrayVec<[T; 2]>),
        borrowed(tinyvec::ArrayVec<[&T; 2]>),
    );
    #[cfg(feature = "tinyvec")]
    truncated_collection_t!(
        distinguished,
        owned(tinyvec::TinyVec<[T; 2]>),
        borrowed(tinyvec::TinyVec<[&T; 2]>),
    );
    truncated_collection_t!(
        distinguished,
        owned(std::collections::BTreeSet<T>),
        borrowed(std::collections::BTreeSet<&T>),
    );
    #[cfg(feature = "std")]
    truncated_collection_t!(
        relaxed,
        owned(std::collections::HashSet<T>),
        borrowed(std::collections::HashSet<&T>),
    );
    #[cfg(feature = "hashbrown")]
    truncated_collection_t!(
        relaxed,
        owned(hashbrown::HashSet<T>),
        borrowed(hashbrown::HashSet<&T>),
    );
}

// Oneof tests

// Oneofs can be nested in Box, whether or not they are non-empty typed.
#[test]
fn oneof_field_decoding() {
    #[derive(Debug, PartialEq, Eq, Message)]
    #[bilrost(distinguished)]
    struct Foo<T>(#[bilrost(oneof = "1, 2")] T);

    macro_rules! do_oneof_field_decoding {
        ($ty:ty, $expected_a:expr, $expected_b:expr, $oneof_name:expr $(,)?) => {{
            let oneof_name: &str = $oneof_name;
            assert::decodes!(owned distinguished, [(1, OV::bool(true))], Foo($expected_a));
            assert::decodes!(owned distinguished, [(2, OV::bool(false))], Foo($expected_b));
            assert::decodes!(
                owned never decodes Foo<$ty>,
                [(1, OV::bool(false)), (2, OV::bool(true))],
                ConflictingFields,
                &format!("Foo.0/{oneof_name}.B"),
            );
            assert::decodes!(
                owned never decodes Foo<$ty>,
                [(1, OV::bool(true)), (1, OV::bool(false))],
                UnexpectedlyRepeated,
                &format!("Foo.0/{oneof_name}.A"),
            );
        }};
    }

    // tests for non-empty oneof
    #[derive(Clone, Debug, PartialEq, Eq, Oneof)]
    #[bilrost(distinguished)]
    enum AB {
        #[bilrost(1)]
        A(bool),
        #[bilrost(2)]
        B(bool),
    }

    do_oneof_field_decoding!(Option<AB>, Some(AB::A(true)), Some(AB::B(false)), "AB");
    do_oneof_field_decoding!(
        Option<Box<AB>>,
        Some(Box::new(AB::A(true))),
        Some(Box::new(AB::B(false))),
        "AB",
    );
    do_oneof_field_decoding!(
        Option<Box<Box<AB>>>,
        Some(Box::new(Box::new(AB::A(true)))),
        Some(Box::new(Box::new(AB::B(false)))),
        "AB",
    );
    do_oneof_field_decoding!(
        Box<Option<Box<AB>>>,
        Box::new(Some(Box::new(AB::A(true)))),
        Box::new(Some(Box::new(AB::B(false)))),
        "AB",
    );
    do_oneof_field_decoding!(
        Box<Option<Box<AB>>>,
        Box::new(Some(Box::new(AB::A(true)))),
        Box::new(Some(Box::new(AB::B(false)))),
        "AB",
    );

    // tests for oneof with natural empty state
    #[derive(Clone, Debug, PartialEq, Eq, Oneof)]
    #[bilrost(distinguished)]
    enum ABNone {
        None,
        #[bilrost(1)]
        A(bool),
        #[bilrost(2)]
        B(bool),
    }

    do_oneof_field_decoding!(ABNone, ABNone::A(true), ABNone::B(false), "ABNone");
    do_oneof_field_decoding!(
        Box<ABNone>,
        Box::new(ABNone::A(true)),
        Box::new(ABNone::B(false)),
        "ABNone",
    );
    do_oneof_field_decoding!(
        Box<Box<ABNone>>,
        Box::new(Box::new(ABNone::A(true))),
        Box::new(Box::new(ABNone::B(false))),
        "ABNone",
    );
    do_oneof_field_decoding!(
        Box<Box<Box<ABNone>>>,
        Box::new(Box::new(Box::new(ABNone::A(true)))),
        Box::new(Box::new(Box::new(ABNone::B(false)))),
        "ABNone",
    );
}

#[test]
fn oneof_with_errors_inside() {
    #[derive(Clone, Debug, PartialEq, Eq, Oneof)]
    #[bilrost(distinguished)]
    enum Natural {
        None,
        #[bilrost(1)]
        A(String),
    }
    #[derive(Clone, Debug, PartialEq, Eq, Oneof)]
    #[bilrost(distinguished)]
    enum Optioned {
        #[bilrost(1)]
        B(String),
    }

    #[derive(Clone, Debug, PartialEq, Eq, Message)]
    #[bilrost(distinguished)]
    struct Foo<O> {
        #[bilrost(oneof(1))]
        a: O,
    }

    macro_rules! test_oneof_with_errors_inside {
        ($oneof_ty:ty, $child_field:expr) => {
            {
                let child_field: &str = $child_field;
                assert::decodes!(
                    owned never decodes Foo<$oneof_ty>,
                    [(1, OV::bytes(*b"\xff"))],
                    InvalidValue,
                    &format!("Foo.a/{child_field}"),
                );
                assert::decodes!(
                    owned never decodes Foo<$oneof_ty>,
                    [(1, OV::str("abc")), (1, OV::str("def"))],
                    UnexpectedlyRepeated,
                    &format!("Foo.a/{child_field}"),
                );
            }
        };
    }

    test_oneof_with_errors_inside!(Natural, "Natural.A");
    test_oneof_with_errors_inside!(Option<Optioned>, "Optioned.B");
    test_oneof_with_errors_inside!(Box<Natural>, "Natural.A");
    test_oneof_with_errors_inside!(Box<Box<Box<Natural>>>, "Natural.A");
    test_oneof_with_errors_inside!(Box<Option<Optioned>>, "Optioned.B");
    test_oneof_with_errors_inside!(Option<Box<Optioned>>, "Optioned.B");
    test_oneof_with_errors_inside!(Box<Option<Box<Optioned>>>, "Optioned.B");
}

#[test]
fn oneof_optioned_fields_encode_empty() {
    #[derive(Clone, Debug, PartialEq, Eq, Oneof)]
    #[bilrost(distinguished)]
    enum Abc {
        #[bilrost(1)]
        A(String),
        #[bilrost(2)]
        B { named: u32 },
        #[bilrost(tag = 3, encoding = "packed")]
        C(Vec<bool>),
    }
    use Abc::*;

    #[derive(Clone, Debug, PartialEq, Eq, Message)]
    #[bilrost(distinguished)]
    struct Foo(#[bilrost(oneof(1, 2, 3))] Option<Abc>);

    assert::decodes!(owned distinguished, [], Foo(None));

    for (opaque, value) in &[
        ([(1, OV::string(""))], Foo(Some(A(Default::default())))),
        (
            [(1, OV::string("something"))],
            Foo(Some(A("something".to_string()))),
        ),
        (
            [(2, OV::u32(0))],
            Foo(Some(B {
                named: Default::default(),
            })),
        ),
        ([(2, OV::u32(123))], Foo(Some(B { named: 123 }))),
        ([(3, OV::packed([]))], Foo(Some(C(Default::default())))),
        (
            [(3, OV::packed([OV::bool(false), OV::bool(true)]))],
            Foo(Some(C(vec![false, true]))),
        ),
    ] {
        assert::decodes!(owned distinguished, opaque, value.clone());
    }
}

#[test]
fn oneof_plain_fields_encode_empty() {
    /// Oneofs that have an empty variant
    #[derive(Clone, Debug, PartialEq, Eq, Oneof)]
    #[bilrost(distinguished)]
    enum Abc {
        Empty,
        #[bilrost(1)]
        A(String),
        #[bilrost(2)]
        B {
            named: u32,
        },
        #[bilrost(tag = 3, encoding = "packed")]
        C(Vec<bool>),
    }
    use Abc::*;

    #[derive(Clone, Debug, PartialEq, Eq, Message)]
    #[bilrost(distinguished)]
    struct Foo(#[bilrost(oneof(1, 2, 3))] Abc);

    assert::decodes!(owned distinguished, [], Foo(Empty));

    for (opaque, value) in &[
        ([(1, OV::string(""))], Foo(A(Default::default()))),
        (
            [(1, OV::string("something"))],
            Foo(A("something".to_string())),
        ),
        (
            [(2, OV::u32(0))],
            Foo(B {
                named: Default::default(),
            }),
        ),
        ([(2, OV::u32(123))], Foo(B { named: 123 })),
        ([(3, OV::packed([]))], Foo(C(Default::default()))),
        (
            [(3, OV::packed([OV::bool(false), OV::bool(true)]))],
            Foo(C(vec![false, true])),
        ),
    ] {
        assert::decodes!(owned distinguished, opaque, value.clone());
    }
}

#[test]
fn oneof_as_message() {
    #[derive(Debug, PartialEq, Eq, Oneof, Message)]
    #[bilrost(distinguished)]
    enum AB {
        Nothing,
        #[bilrost(1)]
        A(bool),
        #[bilrost(2)]
        B(bool),
    }

    assert::decodes!(owned distinguished, [], AB::Nothing);
    assert::decodes!(owned distinguished, [(1, OV::bool(false))], AB::A(false));
    assert::decodes!(owned distinguished, [(2, OV::bool(true))], AB::B(true));
    assert::decodes!(
        owned non-canonically,
        [(1, OV::bool(true)), (4, OV::str("extension"))],
        AB::A(true),
        HasExtensions,
        "",
    );
    assert::decodes!(
        owned never decodes AB,
        [(1, OV::bool(false)), (2, OV::bool(true))],
        ConflictingFields,
        "AB.B",
    );
    assert::decodes!(
        owned never decodes AB,
        [(1, OV::bool(true)), (1, OV::bool(false))],
        UnexpectedlyRepeated,
        "AB.A",
    );
    // This test is very important: the second field is an extension that must be skipped correctly.
    // If it is not consumed when it should be skipped and is instead read as if it were a bunch of
    // field keys, it acts as a poison pill that is consumed all at once as an invalid varint,
    // throwing an error instead.
    assert::decodes!(
        owned non-canonically,
        [(1, OV::bool(true)), (99, OV::bytes([0xff; 16]))],
        AB::A(true),
        HasExtensions,
        "",
    );

    // These tags are very different lengths and may use the RuntimeTagMeasurer instead of the
    // TrivialTagMeasurer, so we should test that path.
    #[derive(Debug, PartialEq, Eq, Oneof, Message)]
    #[bilrost(distinguished)]
    enum EarlyLate {
        Nothing,
        #[bilrost(0)]
        Early(bool),
        #[bilrost(999999999)]
        Late(bool),
    }

    assert::decodes!(owned distinguished, [], EarlyLate::Nothing);
    assert::decodes!(owned distinguished, [(0, OV::bool(false))], EarlyLate::Early(false));
    assert::decodes!(owned distinguished, [(999999999, OV::bool(true))], EarlyLate::Late(true));
}

#[test]
fn oneof_as_message_unqualified() {
    #[allow(dead_code)]
    #[derive(Debug, PartialEq, Eq, Oneof, Message)]
    #[bilrost(distinguished)]
    enum Maybe<T> {
        Nothing,
        #[bilrost(1)]
        Something(T),
    }

    static_assertions::assert_impl_all!(
        Maybe<i32>: DistinguishedOneofDecoder, DistinguishedOwnedMessage
    );

    static_assertions::assert_impl_all!(Maybe<f64>: Oneof, OwnedMessage);
    static_assertions::assert_not_impl_any!(
        Maybe<f64>: DistinguishedOneofDecoder, DistinguishedOwnedMessage
    );

    #[allow(dead_code)]
    struct NotEncodable;
    static_assertions::assert_not_impl_any!(
        Maybe<NotEncodable>: DistinguishedOneofDecoder, DistinguishedOwnedMessage
    );
}

#[test]
fn distinguished_oneof_with_nonempty_variant() {
    #[derive(Debug, PartialEq, Eq, Enumeration)]
    enum NonEmptyTy {
        #[bilrost(5)]
        Five,
    }

    #[derive(Debug, PartialEq, Eq, Oneof)]
    #[bilrost(distinguished)]
    enum DistinguishedEnum {
        #[bilrost(1)]
        A(NonEmptyTy),
    }
}

#[test]
fn oneof_named_after_builtin_encoding_alias() {
    #[allow(non_camel_case_types)]
    #[derive(Debug, Oneof)]
    #[bilrost(distinguished)]
    enum general {
        plainbytes,
        #[bilrost(1)]
        varint(usize),
    }
    static_assertions::assert_impl_all!(
        general:
            Oneof,
            OneofDecoder, OneofBorrowDecoder<'static>,
            DistinguishedOneofDecoder, DistinguishedOneofBorrowDecoder<'static>,
    );
}

// Enumeration tests

#[test]
fn enumeration_decoding() {
    #[derive(Clone, Debug, Default, PartialEq, Eq, Enumeration)]
    enum DefaultButNoZero {
        #[default]
        Five = 5,
        Ten = 10,
        Fifteen = 15,
    }
    use DefaultButNoZero::*;

    #[derive(Clone, Debug, PartialEq, Eq, Enumeration)]
    enum HasZero {
        Zero = 0,
        Big = 1000,
        Bigger = 1_000_000,
    }
    use HasZero::*;

    #[derive(Clone, Debug, PartialEq, Eq, Message)]
    #[bilrost(distinguished)]
    struct Foo(Option<DefaultButNoZero>, HasZero);

    assert::decodes!(owned distinguished, [], Foo(None, Zero));
    assert::decodes!(owned distinguished, [(0, OV::u32(5))], Foo(Some(Five), Zero));
    assert::decodes!(owned distinguished, [(0, OV::u32(10))], Foo(Some(Ten), Zero));
    assert::decodes!(owned distinguished, [(0, OV::u32(15))], Foo(Some(Fifteen), Zero));
    assert::decodes!(
        owned non-canonically,
        [(1, OV::u32(0))],
        Foo(None, Zero),
        NotCanonical,
        "Foo.1",
    );
    assert::decodes!(owned distinguished, [(1, OV::u32(1_000))], Foo(None, Big));
    assert::decodes!(owned distinguished, [(1, OV::u32(1_000_000))], Foo(None, Bigger));

    #[allow(dead_code)]
    #[derive(Clone, Debug, PartialEq, Eq, Message)]
    #[bilrost(distinguished)]
    struct Packed<T>(#[bilrost(encoding(packed))] T);
    #[allow(dead_code)]
    #[derive(Clone, Debug, PartialEq, Eq, Message)]
    #[bilrost(distinguished)]
    struct Unpacked<T>(#[bilrost(encoding(unpacked))] T);

    static_assertions::assert_impl_all!(Packed<[HasZero; 5]>: DistinguishedOwnedMessage);
    static_assertions::assert_impl_all!(Unpacked<[HasZero; 5]>: DistinguishedOwnedMessage);
    // Fixed-size arrays of enumeration types that don't impl EmptyState aren't supported, because
    // the array type has no empty state either.
    static_assertions::assert_not_impl_any!(Packed<[DefaultButNoZero; 5]>:
        DistinguishedOwnedMessage);
    static_assertions::assert_not_impl_any!(Unpacked<[DefaultButNoZero; 5]>:
        DistinguishedOwnedMessage);

    static_assertions::assert_impl_all!(Packed<Option<[HasZero; 5]>>: DistinguishedOwnedMessage);
    static_assertions::assert_impl_all!(Unpacked<Option<[HasZero; 5]>>: DistinguishedOwnedMessage);
    static_assertions::assert_impl_all!(Packed<Option<[DefaultButNoZero; 5]>>:
        DistinguishedOwnedMessage);
    static_assertions::assert_impl_all!(Unpacked<Option<[DefaultButNoZero; 5]>>:
        DistinguishedOwnedMessage);

    static_assertions::assert_impl_all!(Packed<Vec<HasZero>>: DistinguishedOwnedMessage);
    static_assertions::assert_impl_all!(Unpacked<Vec<HasZero>>: DistinguishedOwnedMessage);
    // If the empty state of the collection is that it has no items, then we *can* represent a
    // collection of values that don't implement EmptyState themselves.
    static_assertions::assert_impl_all!(Packed<Vec<DefaultButNoZero>>: DistinguishedOwnedMessage);
    static_assertions::assert_impl_all!(Unpacked<Vec<DefaultButNoZero>>: DistinguishedOwnedMessage);
}

#[test]
fn nonempty_enumeration_nesting() {
    #[derive(Clone, Debug, Default, PartialEq, Eq, Enumeration)]
    enum DefaultButNoZero {
        #[default]
        Five = 5,
        Ten = 10,
        Fifteen = 15,
    }
    use DefaultButNoZero::*;

    #[derive(Debug, PartialEq, Eq, Message)]
    #[bilrost(distinguished)]
    struct Foo(#[bilrost(encoding(packed<packed>))] Vec<[DefaultButNoZero; 5]>);

    assert::decodes!(
        owned distinguished,
        [(
            0,
            OV::packed([
                OV::packed([
                    OV::u32(5),
                    OV::u32(10),
                    OV::u32(15),
                    OV::u32(10),
                    OV::u32(5),
                ]),
                OV::packed([OV::u32(5), OV::u32(5), OV::u32(5), OV::u32(5), OV::u32(5)]),
            ]),
        )],
        Foo(vec![
            [Five, Ten, Fifteen, Ten, Five],
            [Five, Five, Five, Five, Five],
        ]),
    );
}

#[test]
fn enumeration_helpers() {
    #[derive(Clone, Debug, PartialEq, Eq, Enumeration)]
    enum E {
        Five = 5,
        Ten = 10,
        Fifteen = 15,
    }

    #[derive(Clone, Debug, PartialEq, Eq, Message)]
    #[bilrost(distinguished)]
    struct HelpedStruct {
        #[bilrost(enumeration(E))]
        regular: u32,
        #[bilrost(enumeration(E))]
        optional: Option<u32>,
    }

    let val = HelpedStruct {
        regular: 5,
        optional: Some(5),
    };
    assert_eq!(val.regular(), Ok(E::Five));
    assert_eq!(val.optional(), Some(Ok(E::Five)));

    let val = HelpedStruct {
        regular: 222,
        optional: Some(222),
    };
    assert::decodes!(owned distinguished, [(1, OV::u32(222)), (2, OV::u32(222))], val.clone());
    val.regular()
        .expect_err("bad enumeration value parsed successfully");
    val.optional()
        .unwrap()
        .expect_err("bad enumeration value parsed successfully");

    let val = HelpedStruct::new_empty();
    assert_eq!(val.optional(), None);

    // Demonstrate that the same errors happen when we decode to a struct with strict
    // enumeration fields, it just happens sooner.
    #[derive(Clone, Debug, PartialEq, Eq, Message)]
    #[bilrost(distinguished)]
    struct StrictStruct {
        first: Option<E>,
        second: Option<E>,
    }

    assert::decodes!(
        owned never decodes StrictStruct,
        HelpedStruct {
            regular: 222,
            optional: None,
        }
        .encode_to_vec(),
        OutOfDomainValue,
        "StrictStruct.first",
    );
    assert::decodes!(
        owned never decodes StrictStruct,
        HelpedStruct {
            regular: 5,
            optional: Some(222),
        }
        .encode_to_vec(),
        OutOfDomainValue,
        "StrictStruct.second",
    );
}

#[test]
fn enumeration_value_limits() {
    const TEN: u32 = 10;
    const ELEVEN_U8: u8 = 11;

    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Enumeration)]
    #[repr(u8)]
    enum Foo {
        A = 0,
        #[bilrost = 5]
        D,
        #[bilrost(TEN)]
        #[default]
        T,
        #[bilrost(11)] // ELEVEN_U8 isn't a u32 value, but we can still specify 11 here
        E = ELEVEN_U8,
        #[bilrost(u32::MAX)] // Attribute values take precedence within bilrost
        Z = 255,
    }
    assert_eq!(size_of::<Foo>(), 1);

    #[derive(Debug, PartialEq, Eq, Message)]
    #[bilrost(distinguished)]
    struct Bar(Foo);

    assert_eq!(Foo::A.to_number(), 0);
    assert_eq!(Foo::Z.to_number(), u32::MAX);
    assert_eq!(Foo::try_from_number(u32::MAX), Ok(Foo::Z));
    assert_eq!(Foo::Z as u8, 255);
    assert::decodes!(owned distinguished, [], Bar(Foo::A));
    assert::decodes!(owned non-canonically, [(0, OV::u32(0))], Bar(Foo::A), NotCanonical, "Bar.0");
    assert::decodes!(owned distinguished, [(0, OV::u32(5))], Bar(Foo::D));
    assert::decodes!(owned distinguished, [(0, OV::u32(10))], Bar(Foo::T));
    assert::decodes!(owned distinguished, [(0, OV::u32(11))], Bar(Foo::E));
    assert::decodes!(owned distinguished, [(0, OV::u32(u32::MAX))], Bar(Foo::Z));
}

// Nested message tests

#[test]
fn directly_included_message() {
    #[derive(Clone, Debug, PartialEq, Eq, Message)]
    #[bilrost(distinguished)]
    struct Inner {
        a: String,
        b: i64,
    }

    #[derive(Clone, Debug, PartialEq, Eq, Message)]
    #[bilrost(distinguished)]
    struct OuterDirect {
        inner: Inner,
        also: String,
    }

    #[derive(Debug, PartialEq, Eq, Message)]
    #[bilrost(distinguished)]
    struct OuterOptional {
        inner: Option<Inner>,
        also: Option<String>,
    }

    // With a directly included inner message field, it should encode only when its value is
    // empty.

    // When the inner message is empty, it doesn't encode.
    assert::decodes!(
        owned distinguished,
        [(2, OV::string("abc"))],
        OuterDirect {
            inner: Inner::new_empty(),
            also: "abc".into(),
        },
    );
    assert::decodes!(
        owned distinguished,
        [(2, OV::string("abc"))],
        OuterOptional {
            inner: None,
            also: Some("abc".into()),
        },
    );

    // When the inner message is present in the encoding but empty, it's only canonical when
    // the field is optioned.
    assert::decodes!(
        owned non-canonically,
        [(1, OV::message(&[].into_opaque_message()))],
        OuterDirect::new_empty(),
        NotCanonical,
        "OuterDirect.inner",
    );
    assert::decodes!(
        owned distinguished,
        [(1, OV::message(&[].into_opaque_message()))],
        OuterOptional {
            inner: Some(Message::new_empty()),
            also: None,
        },
    );

    // The inner message is included when it is not fully empty
    assert::decodes!(
        owned distinguished,
        [
            (
                1,
                OV::message(&[(1, OV::string("def"))].into_opaque_message()),
            ),
            (2, OV::string("abc")),
        ],
        OuterDirect {
            inner: Inner {
                a: "def".into(),
                b: 0,
            },
            also: "abc".into(),
        },
    );
    assert::decodes!(
        owned distinguished,
        [
            (
                1,
                OV::message(&[(1, OV::string("def"))].into_opaque_message()),
            ),
            (2, OV::string("abc")),
        ],
        OuterOptional {
            inner: Some(Inner {
                a: "def".into(),
                b: 0,
            }),
            also: Some("abc".into()),
        },
    );

    assert::decodes!(
        owned never decodes OuterDirect,
        [(1, OV::Varint(1))],
        WrongWireType,
        "OuterDirect.inner",
    );
    assert::decodes!(
        owned never decodes OuterOptional,
        [(1, OV::Varint(1))],
        WrongWireType,
        "OuterOptional.inner",
    );
    assert::decodes!(
        owned never decodes OuterDirect,
        [(1, OV::ThirtyTwoBit([1; 4]))],
        WrongWireType,
        "OuterDirect.inner",
    );
    assert::decodes!(
        owned never decodes OuterOptional,
        [(1, OV::ThirtyTwoBit([1; 4]))],
        WrongWireType,
        "OuterOptional.inner",
    );
    assert::decodes!(
        owned never decodes OuterDirect,
        [(1, OV::SixtyFourBit([1; 8]))],
        WrongWireType,
        "OuterDirect.inner",
    );
    assert::decodes!(
        owned never decodes OuterOptional,
        [(1, OV::SixtyFourBit([1; 8]))],
        WrongWireType,
        "OuterOptional.inner",
    );
}

#[test]
fn truncated_submessage() {
    #[derive(Debug, PartialEq, Eq, Message)]
    #[bilrost(distinguished)]
    struct Nested(String);
    #[derive(Debug, PartialEq, Eq, Message)]
    #[bilrost(distinguished)]
    struct Foo(Nested, String);

    let inner = [(0, OV::string("interrupting cow says"))]
        .into_opaque_message()
        .encode_to_vec();
    assert::decodes!(
        owned never decodes Foo,
        [
            (0, OV::byte_slice(&inner[..inner.len() - 1])),
            (1, OV::string("moo")),
        ],
        Truncated,
        "Foo.0/Nested.0",
    );
}

#[test]
fn tuples() {
    #[derive(Debug, PartialEq, Eq, Message)]
    #[bilrost(distinguished)]
    struct Pair<T, U>(#[bilrost(tag(0), encoding(varint))] T, #[bilrost(1)] U);
    #[derive(Debug, PartialEq, Eq, Message)]
    #[bilrost(distinguished)]
    struct Foo<T, U>(Pair<T, U>);

    #[derive(Debug, PartialEq, Eq, Message)]
    #[bilrost(distinguished)]
    struct FooTuple<T, U>(#[bilrost(encoding((varint, general)))] (T, U));

    static_assertions::assert_impl_all!(FooTuple<bool, bool>: DistinguishedOwnedMessage);
    static_assertions::assert_impl_all!(FooTuple<bool, f32>: OwnedMessage);
    static_assertions::assert_not_impl_any!(FooTuple<bool, f32>: DistinguishedOwnedMessage);
    // f32 doesn't support the "varint" encoding.
    static_assertions::assert_not_impl_any!(FooTuple<f32, f32>: OwnedMessage);

    assert_eq!(
        FooTuple((1i8, "foo".to_string())).encode_to_vec(),
        Foo(Pair(1i8, "foo".to_string())).encode_to_vec(),
    );
    assert::decodes!(
        owned distinguished,
        Foo(Pair(1i8, "foo".to_string())).encode_to_vec(),
        FooTuple((1i8, "foo".to_string())),
    );
    assert::decodes!(
        owned non-canonically,
        [(
            0,
            OV::message(&[(0, OV::i8(0)), (1, OV::str("bar"))].into_opaque_message()),
        )],
        FooTuple((0, "bar".to_string())),
        NotCanonical,
        "FooTuple.0/(2-tuple).0",
    );
    assert::decodes!(
        owned non-canonically,
        [(
            0,
            OV::message(&[(0, OV::i8(2)), (1, OV::str(""))].into_opaque_message()),
        )],
        FooTuple((2, "".to_string())),
        NotCanonical,
        "FooTuple.0/(2-tuple).1",
    );
    assert::decodes!(
        owned non-canonically,
        [(
            0,
            OV::message(
                &[(0, OV::i8(3)), (1, OV::str("bar")), (2, OV::bool(true))].into_opaque_message(),
            ),
        )],
        FooTuple((3, "bar".to_string())),
        HasExtensions,
        "FooTuple.0",
    );
    assert::decodes!(
        owned non-canonically,
        [(0, OV::message(&[(2, OV::bool(true))].into_opaque_message()))],
        FooTuple((0, "".to_string())),
        HasExtensions,
        "FooTuple.0",
    );
    assert::decodes!(
        owned non-canonically,
        [(0, OV::message(&()))],
        FooTuple((0, "".to_string())),
        NotCanonical,
        "FooTuple.0",
    );

    // Errors within tuples
    #[derive(Debug, PartialEq, Eq, Message)]
    #[bilrost(distinguished)]
    struct Wrapper<T>(T);

    assert::decodes!(
        owned never decodes Wrapper<(i64,)>,
        [
            (0, OV::message(&[
                (0, OV::fixed_i64(123)),
            ].into_opaque_message())),
        ],
        WrongWireType,
        "Wrapper.0/(1-tuple).0",
    );
    assert::decodes!(
        owned non-canonically,
        [
            (0, OV::message(&[
                (0, OV::i64(456)),
                (1, OV::message(&[
                    (0, OV::str("hello")),
                    (123, OV::str("extra")),
                ].into_opaque_message())),
            ].into_opaque_message())),
        ],
        Wrapper::<(i64, (String,))>((456_i64, ("hello".to_owned(),))),
        HasExtensions,
        "Wrapper.0/(2-tuple).1",
    );
    assert::decodes!(
        owned never decodes Wrapper<(i64,)>,
        [
            (0, OV::message(&[
                (0, OV::i64(123)),
                (0, OV::i64(123)),
            ].into_opaque_message())),
        ],
        UnexpectedlyRepeated,
        "Wrapper.0/(1-tuple).0",
    );
}

#[test]
fn unknown_fields_distinguished() {
    #[derive(Debug, PartialEq, Eq, Message)]
    #[bilrost(distinguished)]
    struct Nested(#[bilrost(1)] i64);

    #[derive(Debug, PartialEq, Eq, Oneof)]
    #[bilrost(distinguished)]
    enum InnerOneof {
        Empty,
        #[bilrost(3)]
        Three(Nested),
        #[bilrost(5)]
        Five(i32),
        #[bilrost(7)]
        Seven(bool),
    }
    use InnerOneof::*;

    #[derive(Debug, PartialEq, Eq, Message)]
    #[bilrost(distinguished)]
    struct Foo {
        #[bilrost(0)]
        zero: String,
        #[bilrost(1)]
        one: u64,
        #[bilrost(4)]
        four: Option<Nested>,
        #[bilrost(oneof(3, 5, 7))]
        oneof: InnerOneof,
    }

    assert::decodes!(
        owned distinguished,
        [
            (0, OV::string("hello")),
            (3, OV::message(&[(1, OV::i64(301))].into_opaque_message())),
            (4, OV::message(&[(1, OV::i64(555))].into_opaque_message())),
        ],
        Foo {
            zero: "hello".into(),
            four: Some(Nested(555)),
            oneof: Three(Nested(301)),
            ..Message::new_empty()
        },
    );
    assert::decodes!(
        owned non-canonically,
        [
            (0, OV::string("hello")),
            (2, OV::u32(123)), // Unknown field
            (3, OV::message(&[(1, OV::i64(301))].into_opaque_message())),
            (4, OV::message(&[(1, OV::i64(555))].into_opaque_message())),
        ],
        Foo {
            zero: "hello".into(),
            four: Some(Nested(555)),
            oneof: Three(Nested(301)),
            ..Message::new_empty()
        },
        HasExtensions,
        "",
    );
    assert::decodes!(
        owned non-canonically,
        [
            (0, OV::string("hello")),
            (3, OV::message(&[(1, OV::i64(301))].into_opaque_message())),
            (
                4,
                OV::message(
                    &[
                        (0, OV::string("unknown")), // Unknown field
                        (1, OV::i64(555)),
                    ]
                    .into_opaque_message(),
                ),
            ),
        ],
        Foo {
            zero: "hello".into(),
            four: Some(Nested(555)),
            oneof: Three(Nested(301)),
            ..Message::new_empty()
        },
        HasExtensions,
        "Foo.four",
    );
    assert::decodes!(
        owned non-canonically,
        [
            (0, OV::string("hello")),
            (
                3,
                OV::message(
                    &[
                        (0, OV::string("unknown")), // unknown field
                        (1, OV::i64(301)),
                    ]
                    .into_opaque_message(),
                ),
            ),
            (4, OV::message(&[(1, OV::i64(555))].into_opaque_message())),
        ],
        Foo {
            zero: "hello".into(),
            four: Some(Nested(555)),
            oneof: Three(Nested(301)),
            ..Message::new_empty()
        },
        HasExtensions,
        "Foo.oneof/InnerOneof.Three",
    );

    // We should be sensitive to multiple tiers of non-canonicity
    assert::decodes!(
        owned distinguished,
        [
            (1, OV::u64(1)),
            (3, OV::message(&[(1, OV::i64(1))].into_opaque_message())),
        ],
        Foo {
            one: 1,
            oneof: Three(Nested(1)),
            ..Message::new_empty()
        },
    );
    // We can see when there are extensions in both the inner and outer message...
    assert::decodes!(
        owned non-canonically,
        [
            (1, OV::u64(1)),
            (
                3,
                OV::message(&[(1, OV::i64(1)), (2, OV::string("unknown"))].into_opaque_message()),
            ),
        ],
        Foo {
            one: 1,
            oneof: Three(Nested(1)),
            ..Message::new_empty()
        },
        HasExtensions,
        "Foo.oneof/InnerOneof.Three",
    );
    assert::decodes!(
        owned non-canonically,
        [
            (1, OV::u64(1)),
            (2, OV::string("unknown")),
            (3, OV::message(&[(1, OV::i64(1))].into_opaque_message())),
        ],
        Foo {
            one: 1,
            oneof: Three(Nested(1)),
            ..Message::new_empty()
        },
        HasExtensions,
        "",
    );
    // and a non-canonical field that occurs later in the message overrides the canonicity to
    // the worse `NotCanonical`
    assert::decodes!(
        owned non-canonically,
        [
            (1, OV::u64(1)),
            (2, OV::string("unknown")),
            (3, OV::message(&[(1, OV::i64(0))].into_opaque_message())),
        ],
        Foo {
            one: 1,
            oneof: Three(Nested(0)),
            ..Message::new_empty()
        },
        NotCanonical,
        // depending on how constrained a mode we parse in we can get different errors back from
        // restricted mode
        [
            (HasExtensions, ""),
            (NotCanonical, "Foo.oneof/InnerOneof.Three/Nested.0"),
        ],
    );
}

#[test]
fn length_delimited_borrowed_decoding_shortens_input_slices() {
    #[derive(Debug, PartialEq, Eq, Message)]
    #[bilrost(distinguished)]
    struct Foo<'a>(u64, &'a str);

    let originals = [
        Foo(1, "decode relaxed"),
        Foo(2, "replace relaxed"),
        Foo(3, "decode distinguished"),
        Foo(4, "replace distinguished"),
        Foo(5, "decode restricted"),
        Foo(6, "replace restricted"),
        Foo(7, "decode canonical"),
        Foo(8, "replace canonical"),
    ];

    // build length-delimited set of messages in one buffer
    let mut buf = vec![];
    for m in &originals {
        m.encode_length_delimited(&mut buf).unwrap();
    }

    let mut slice = buf.as_slice();

    // We expect the messages come out in the same order, because each of these methods requires
    // `&mut &[u8]` and will mutate the slice
    let mut expectations = originals.into_iter();
    let mut expected_next = || {
        expectations
            .next()
            .ok_or(bilrost::DecodeError::new(DecodeErrorKind::Other))
    };

    let mut replaceable = Foo::new_empty();

    assert_eq!(
        Foo::decode_borrowed_length_delimited(&mut slice),
        expected_next()
    );

    assert_eq!(
        (
            replaceable.replace_borrowed_from_length_delimited(&mut slice),
            &replaceable
        ),
        (Ok(()), &expected_next().unwrap())
    );

    assert_eq!(
        Foo::decode_distinguished_borrowed_length_delimited(&mut slice),
        expected_next().map(|foo| (foo, Canonical))
    );

    assert_eq!(
        (
            replaceable.replace_distinguished_borrowed_from_length_delimited(&mut slice),
            &replaceable
        ),
        (Ok(Canonical), &expected_next().unwrap())
    );

    assert_eq!(
        Foo::decode_restricted_borrowed_length_delimited(&mut slice, Canonical),
        expected_next().map(|foo| (foo, Canonical))
    );

    assert_eq!(
        (
            replaceable.replace_restricted_borrowed_from_length_delimited(&mut slice, Canonical),
            &replaceable
        ),
        (Ok(Canonical), &expected_next().unwrap())
    );

    assert_eq!(
        Foo::decode_canonical_borrowed_length_delimited(&mut slice),
        expected_next()
    );

    assert_eq!(
        (
            replaceable.replace_canonical_borrowed_from_length_delimited(&mut slice),
            &replaceable
        ),
        (Ok(()), &expected_next().unwrap())
    );

    assert!(slice.is_empty());
}

#[test]
fn regression_test_proxied_canonical_type_checks_restriction() {
    #[derive(Debug, PartialEq, Eq, Message)]
    #[bilrost(distinguished)]
    struct Foo {
        proxied: core::time::Duration,
    }

    #[derive(Message)]
    struct FooEquivalent {
        #[bilrost(encoding(packed))]
        proxy_repr: Vec<u64>,
    }

    let thousand_seconds = FooEquivalent {
        proxy_repr: vec![1000, 0],
    };

    assert::decodes!(
        owned non-canonically,
        thousand_seconds.encode_to_vec(),
        Foo { proxied: core::time::Duration::from_secs(1000)},
        NotCanonical,
        "Foo.proxied",
    );
}

#[cfg(feature = "chrono")]
#[test]
fn proxied_underived_error_propagation() {
    #[derive(Debug, PartialEq, Eq, Message)]
    #[bilrost(distinguished)]
    struct Foo {
        proxied: chrono::TimeDelta,
    }

    assert::decodes!(
        owned non-canonically,
        [
            (1, OV::message(&[
                (1, OV::i64(123)),
                (2, OV::fixed_i32(456)),
                (3, OV::str("what")),
            ].into_opaque_message())),
        ],
        Foo {
            proxied: chrono::TimeDelta::new(123, 456).unwrap(),
        },
        HasExtensions,
        "Foo.proxied",
    );

    // This test actually shows the message name from underived's error info
    assert::decodes!(
        owned never decodes Foo,
        [
            (1, OV::message(&[
                (1, OV::i64(123)),
                (2, OV::str("string instead of nanos")),
            ].into_opaque_message())),
        ],
        WrongWireType,
        "Foo.proxied/TimeDelta.nanos",
    );

    assert::decodes!(
        owned never decodes Foo,
        [
            (1, OV::message(&[
                (1, OV::i64(123)),
                (1, OV::i64(123)),
            ].into_opaque_message())),
        ],
        UnexpectedlyRepeated,
        "Foo.proxied/TimeDelta.secs",
    );
}

#[test]
fn borrow_traits_function_through_option() {
    #[derive(PartialEq, Eq, Message)]
    #[bilrost(distinguished)]
    struct Foo<'a>(
        #[bilrost(encoding(plainbytes))] Option<&'a [u8]>,
        Option<&'a str>,
    );

    let buf: &[u8] = &[];

    Foo::decode_borrowed(buf).unwrap();
    Foo::decode_canonical_borrowed(buf).unwrap();
}

#[test]
fn implicit_encoding_ergonomics() {
    // As of 0.1013, fields with no annotated encodings should *by default* have a packed encoding
    // when they're placed in a oneof variant. Previously they needed to be explicitly annotated,
    // otherwise there would be a pretty confusing error.
    use std::collections::{BTreeMap, BTreeSet};

    {
        #[derive(Oneof)]
        enum OneofWithRepeatedVariants<'a> {
            #[bilrost(1)]
            A(Vec<u32>),
            #[bilrost(2)]
            B(Cow<'a, [u32]>),
            #[bilrost(3)]
            C(BTreeSet<u32>),
        }
        #[derive(Message)]
        struct MessageWithNestedRepeateds<'a> {
            #[bilrost(encoding(packed))]
            a1: Vec<Vec<u32>>,
            a2: Vec<Vec<u32>>,
            a3: BTreeMap<Vec<u32>, Vec<u32>>,
            #[bilrost(encoding(packed))]
            b1: Vec<Cow<'a, [u32]>>,
            b2: Vec<Cow<'a, [u32]>>,
            b3: BTreeMap<Cow<'a, [u32]>, Cow<'a, [u32]>>,
            #[bilrost(encoding(packed))]
            c1: Vec<BTreeSet<u32>>,
            c2: Vec<BTreeSet<u32>>,
            c3: BTreeMap<BTreeSet<u32>, BTreeSet<u32>>,
        }
    }
    #[cfg(feature = "std")]
    {
        #[derive(Oneof)]
        enum OneofWithRepeatedVariants {
            #[bilrost(1)]
            A(std::collections::HashSet<u32>),
        }
        #[derive(Message)]
        struct MessageWithNestedRepeateds {
            #[bilrost(encoding(packed))]
            a1: Vec<std::collections::HashSet<u32>>,
            a2: Vec<std::collections::HashSet<u32>>,
            a3: BTreeMap<u32, std::collections::HashSet<u32>>,
        }
    }
    #[cfg(feature = "arrayvec")]
    {
        #[derive(Oneof)]
        enum OneofWithRepeatedVariants {
            #[bilrost(1)]
            A(arrayvec::ArrayVec<u32, 10>),
        }
        #[derive(Message)]
        struct MessageWithNestedRepeateds {
            #[bilrost(encoding(packed))]
            a1: Vec<arrayvec::ArrayVec<u32, 10>>,
            a2: Vec<arrayvec::ArrayVec<u32, 10>>,
            a3: BTreeMap<arrayvec::ArrayVec<u32, 10>, arrayvec::ArrayVec<u32, 10>>,
        }
    }
    #[cfg(feature = "hashbrown")]
    {
        #[derive(Oneof)]
        enum OneofWithRepeatedVariants {
            #[bilrost(1)]
            A(hashbrown::HashSet<u32>),
        }
        #[derive(Message)]
        struct MessageWithNestedRepeateds {
            #[bilrost(encoding(packed))]
            a1: Vec<hashbrown::HashSet<u32>>,
            a2: Vec<hashbrown::HashSet<u32>>,
            a3: BTreeMap<u32, hashbrown::HashSet<u32>>,
        }
    }
    #[cfg(feature = "smallvec")]
    {
        #[derive(Oneof)]
        enum OneofWithRepeatedVariants {
            #[bilrost(1)]
            A(smallvec::SmallVec<[u32; 10]>),
        }
        #[derive(Message)]
        struct MessageWithNestedRepeateds {
            #[bilrost(encoding(packed))]
            a1: Vec<smallvec::SmallVec<[u32; 10]>>,
            a2: Vec<smallvec::SmallVec<[u32; 10]>>,
            a3: BTreeMap<smallvec::SmallVec<[u32; 10]>, smallvec::SmallVec<[u32; 10]>>,
        }
    }
    #[cfg(feature = "thin-vec")]
    {
        #[derive(Oneof)]
        enum OneofWithRepeatedVariants {
            #[bilrost(1)]
            A(thin_vec::ThinVec<u32>),
        }
        #[derive(Message)]
        struct MessageWithNestedRepeateds {
            #[bilrost(encoding(packed))]
            a1: Vec<thin_vec::ThinVec<u32>>,
            a2: Vec<thin_vec::ThinVec<u32>>,
            a3: BTreeMap<thin_vec::ThinVec<u32>, thin_vec::ThinVec<u32>>,
        }
    }
    #[cfg(feature = "tinyvec")]
    {
        #[derive(Oneof)]
        enum OneofWithRepeatedVariants {
            #[bilrost(1)]
            A(tinyvec::ArrayVec<[u32; 10]>),
            #[bilrost(2)]
            B(tinyvec::TinyVec<[u32; 10]>),
        }
        #[derive(Message)]
        struct MessageWithNestedRepeateds {
            #[bilrost(encoding(packed))]
            a1: Vec<tinyvec::ArrayVec<[u32; 10]>>,
            a2: Vec<tinyvec::ArrayVec<[u32; 10]>>,
            a3: BTreeMap<tinyvec::ArrayVec<[u32; 10]>, tinyvec::ArrayVec<[u32; 10]>>,
            #[bilrost(encoding(packed))]
            b1: Vec<tinyvec::TinyVec<[u32; 10]>>,
            b2: Vec<tinyvec::TinyVec<[u32; 10]>>,
            b3: BTreeMap<tinyvec::TinyVec<[u32; 10]>, tinyvec::TinyVec<[u32; 10]>>,
        }
    }
}
