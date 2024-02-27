//! This file should contain most of the specific tests for the observed behavior and available
//! types of bilrost messages and their fields. If there's an observed behavior in a type of message
//! or field that we implement, we want to demonstrate it here.

fn main() {
    println!("This file is meant to contain tests, so we can use the proc macros within it.")
}

#[cfg(test)]
mod derived_message_tests {
    use std::borrow::Cow;
    use std::default::Default;
    use std::fmt::Debug;
    use std::iter;

    use itertools::{repeat_n, Itertools};

    use bilrost::encoding::opaque::{OpaqueMessage, OpaqueValue as OV};
    use bilrost::encoding::{
        self, encode_varint, Collection, DistinguishedOneof, EmptyState, Fixed, General, Mapping,
        Oneof, Packed,
    };
    use bilrost::Canonicity::{HasExtensions, NotCanonical};
    use bilrost::DecodeErrorKind::{
        ConflictingFields, InvalidValue, OutOfDomainValue, TagOverflowed, Truncated,
        UnexpectedlyRepeated, WrongWireType,
    };
    use bilrost::{DecodeErrorKind, DistinguishedMessage, Enumeration, Message, Oneof};
    use bilrost_derive::DistinguishedOneof;

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
            <() as Message>::decode(self.as_slice()).expect("did not decode with ignore unit");
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

    impl<T: Message> FromOpaque for T {
        fn from_opaque<'a>(from: impl IntoOpaqueMessage<'a>) -> Self {
            Self::decode(&*from.into_opaque_message().encode_to_vec()).expect("failed to decode")
        }
    }

    mod assert {
        use super::*;
        use bilrost::Canonicity;
        use bilrost::Canonicity::Canonical;

        pub(super) fn decodes<'a, M>(from: impl IntoOpaqueMessage<'a>, into: M)
        where
            M: Message + Debug + PartialEq + EmptyState,
        {
            let encoded = from.into_opaque_message().encode_to_vec();
            assert_eq!(M::decode(encoded.as_slice()).as_ref(), Ok(&into));
            let mut to_replace = M::empty();
            to_replace.replace_from(encoded.as_slice()).unwrap();
            assert_eq!(&to_replace, &into);
        }

        pub(super) fn doesnt_decode<'a, M>(from: impl IntoOpaqueMessage<'a>, err: DecodeErrorKind)
        where
            M: Message + Debug + EmptyState,
        {
            let encoded = from.into_opaque_message().encode_to_vec();
            assert_eq!(
                M::decode(encoded.as_slice())
                    .expect_err("unexpectedly decoded without error")
                    .kind(),
                err
            );
            let mut to_replace = M::empty();
            assert_eq!(
                to_replace
                    .replace_from(encoded.as_slice())
                    .expect_err("unexpectedly replaced without error")
                    .kind(),
                err
            );
        }

        pub(super) fn decodes_distinguished<'a, M>(from: impl IntoOpaqueMessage<'a>, into: M)
        where
            M: DistinguishedMessage + Debug + Eq + EmptyState,
        {
            let encoded = from.into_opaque_message().encode_to_vec();
            assert_eq!(M::decode(encoded.as_slice()).as_ref(), Ok(&into));
            let (decoded, canon) =
                M::decode_distinguished(encoded.as_slice()).expect("distinguished decoding failed");
            assert_eq!(&decoded, &into, "distinguished decoded doesn't match");
            assert_eq!(canon, Canonical);
            let mut to_replace = M::empty();
            to_replace.replace_from(encoded.as_slice()).unwrap();
            assert_eq!(&to_replace, &into, "doesn't match after expedient replace");
            to_replace = M::empty();
            assert_eq!(
                to_replace.replace_distinguished_from(encoded.as_slice()),
                Ok(Canonical)
            );
            assert_eq!(
                &to_replace, &into,
                "doesn't match after distinguished replace"
            );
            assert_eq!(
                encoded,
                into.encode_to_vec(),
                "distinguished encoding does not round trip"
            );
            assert_eq!(into.encoded_len(), encoded.len(), "encoded_len was wrong");
        }

        pub(super) fn decodes_non_canonically<'a, M>(
            from: impl IntoOpaqueMessage<'a>,
            into: M,
            expected_canon: Canonicity,
        ) where
            M: DistinguishedMessage + Debug + Eq + EmptyState,
        {
            assert_ne!(expected_canon, Canonical); // otherwise why call this function
            let encoded = from.into_opaque_message().encode_to_vec();
            assert_eq!(M::decode(encoded.as_slice()).as_ref(), Ok(&into));
            let mut to_replace = M::empty();
            to_replace.replace_from(encoded.as_slice()).unwrap();
            assert_eq!(&to_replace, &into);
            let (decoded, canon) = M::decode_distinguished(encoded.as_slice())
                .expect("error decoding in distinguished mode with non-canonical data");
            assert_eq!(&decoded, &into, "distinguished decoded doesn't match");
            assert_eq!(canon, expected_canon);
            let mut to_replace = M::empty();
            assert_eq!(
                to_replace
                    .replace_distinguished_from(encoded.as_slice())
                    .expect("error replacing in distinguished mode with non-canonical data"),
                expected_canon
            );
            assert_eq!(
                &to_replace, &into,
                "doesn't match after distinguished replace"
            );
            let round_tripped = into.encode_to_vec();
            assert_ne!(
                encoded, round_tripped,
                "encoding round tripped, but did not decode distinguished"
            );
            assert_eq!(
                into.encoded_len(),
                round_tripped.len(),
                "encoded_len was wrong"
            );
        }

        pub(super) fn never_decodes<'a, M>(from: impl IntoOpaqueMessage<'a>, err: DecodeErrorKind)
        where
            M: DistinguishedMessage + Debug + EmptyState,
        {
            let encoded = from.into_opaque_message().encode_to_vec();
            assert_eq!(
                M::decode(encoded.as_slice())
                    .expect_err("unepectedly decoded in expedient mode without error")
                    .kind(),
                err
            );
            let mut to_replace = M::empty();
            assert_eq!(
                to_replace
                    .replace_from(encoded.as_slice())
                    .expect_err("unexpectedly replaced in expedient mode without error")
                    .kind(),
                err
            );
            assert_eq!(
                M::decode_distinguished(encoded.as_slice())
                    .expect_err("unexpectedly decoded in distinguished mode without error")
                    .kind(),
                err
            );
            let mut to_replace = M::empty();
            assert_eq!(
                to_replace
                    .replace_distinguished_from(encoded.as_slice())
                    .expect_err("unexpectedly replaced in distinguished mode without error")
                    .kind(),
                err
            );
        }

        pub(super) fn encodes<'a, M: Message>(value: M, becomes: impl IntoOpaqueMessage<'a>) {
            let encoded = value.encode_to_vec();
            assert_eq!(
                OpaqueMessage::decode(&*encoded),
                Ok(becomes.into_opaque_message())
            );
            assert_eq!(value.encoded_len(), encoded.len(), "encoded_len was wrong");
        }

        pub(super) fn is_invalid<M>(value: impl AsRef<[u8]>, err: DecodeErrorKind)
        where
            M: Message + Debug + EmptyState,
        {
            assert_eq!(
                M::decode(value.as_ref())
                    .expect_err("decoded without error")
                    .kind(),
                err
            );
            let mut to_replace = M::empty();
            assert_eq!(
                to_replace
                    .replace_from(value.as_ref())
                    .expect_err("replaced without error")
                    .kind(),
                err
            );
        }

        pub(super) fn is_invalid_distinguished<M>(value: impl AsRef<[u8]>, err: DecodeErrorKind)
        where
            M: DistinguishedMessage + Debug + EmptyState,
        {
            assert_eq!(
                M::decode_distinguished(value.as_ref())
                    .expect_err("decoded without error")
                    .kind(),
                err
            );
            let mut to_replace = M::empty();
            assert_eq!(
                to_replace
                    .replace_distinguished_from(value.as_ref())
                    .expect_err("replaced without error")
                    .kind(),
                err
            );
        }
    }

    // Tests for derived trait bounds

    #[test]
    fn derived_trait_bounds() {
        #[allow(dead_code)]
        struct X; // Not encodable

        #[derive(PartialEq, Eq, Oneof, DistinguishedOneof)]
        enum A<T> {
            Empty,
            #[bilrost(1)]
            One(bool),
            #[bilrost(2)]
            Two(T),
        }
        static_assertions::assert_impl_all!(A<bool>: Oneof, DistinguishedOneof);
        static_assertions::assert_impl_all!(A<f32>: Oneof);
        static_assertions::assert_not_impl_any!(A<f32>: DistinguishedOneof);
        static_assertions::assert_not_impl_any!(A<X>: Oneof, DistinguishedOneof);

        #[derive(PartialEq, Eq, Message, DistinguishedMessage)]
        struct Inner<U>(U);
        static_assertions::assert_impl_all!(Inner<bool>: Message, DistinguishedMessage);
        static_assertions::assert_impl_all!(Inner<f32>: Message);
        static_assertions::assert_not_impl_any!(Inner<f32>: DistinguishedMessage);
        static_assertions::assert_not_impl_any!(Inner<X>: Message, DistinguishedMessage);

        #[derive(PartialEq, Eq, Message, DistinguishedMessage)]
        struct Foo<T, U, V>(#[bilrost(oneof(1, 2))] A<T>, Inner<U>, V);
        static_assertions::assert_impl_all!(Foo<bool, bool, bool>: Message, DistinguishedMessage);
        static_assertions::assert_impl_all!(Foo<f32, bool, bool>: Message);
        static_assertions::assert_impl_all!(Foo<bool, f32, bool>: Message);
        static_assertions::assert_impl_all!(Foo<bool, bool, f32>: Message);
        static_assertions::assert_not_impl_any!(Foo<f32, bool, bool>: DistinguishedMessage);
        static_assertions::assert_not_impl_any!(Foo<bool, f32, bool>: DistinguishedMessage);
        static_assertions::assert_not_impl_any!(Foo<bool, bool, f32>: DistinguishedMessage);
        static_assertions::assert_not_impl_any!(Foo<X, bool, bool>: Message, DistinguishedMessage);
        static_assertions::assert_not_impl_any!(Foo<bool, X, bool>: Message, DistinguishedMessage);
        static_assertions::assert_not_impl_any!(Foo<bool, bool, X>: Message, DistinguishedMessage);
    }

    #[test]
    fn recursive_messages() {
        #[derive(PartialEq, Eq, Message, DistinguishedMessage)]
        struct Tree {
            #[bilrost(recurses)]
            children: Vec<Tree>,
        }

        static_assertions::assert_impl_all!(Tree: Message, DistinguishedMessage);
    }

    // Tests for encoding rigor

    #[test]
    fn derived_message_field_ordering() {
        #[derive(Clone, Debug, PartialEq, Eq, Oneof, DistinguishedOneof)]
        enum A {
            #[bilrost(1)]
            One(bool),
            #[bilrost(10)]
            Ten(bool),
            #[bilrost(20)]
            Twenty(bool),
        }

        #[derive(Clone, Debug, PartialEq, Eq, Oneof, DistinguishedOneof)]
        enum B {
            #[bilrost(9)]
            Nine(bool),
            #[bilrost(11)]
            Eleven(bool),
        }

        #[derive(Clone, Debug, PartialEq, Eq, Oneof, DistinguishedOneof)]
        enum C {
            #[bilrost(13)]
            Thirteen(bool),
            #[bilrost(16)]
            Sixteen(bool),
            #[bilrost(22)]
            TwentyTwo(bool),
        }

        #[derive(Clone, Debug, PartialEq, Eq, Oneof, DistinguishedOneof)]
        enum D {
            #[bilrost(18)]
            Eighteen(bool),
            #[bilrost(19)]
            Nineteen(bool),
        }

        #[derive(Clone, Debug, PartialEq, Eq, Message, DistinguishedMessage)]
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
            assert::decodes_distinguished(&opaque_message, Struct::from_opaque(&opaque_message));
        }
    }

    #[test]
    fn field_tag_limits() {
        #[derive(Debug, PartialEq, Eq, Message, DistinguishedMessage)]
        struct Foo {
            #[bilrost(0)]
            minimum: Option<bool>,
            #[bilrost(4294967295)]
            maximum: Option<bool>,
        }
        assert::decodes_distinguished(
            [(0, OV::bool(false)), (u32::MAX, OV::bool(true))],
            Foo {
                minimum: Some(false),
                maximum: Some(true),
            },
        );
        assert::never_decodes::<Foo>(
            [(0, OV::bool(false)), (0, OV::bool(true))],
            UnexpectedlyRepeated,
        );
        assert::never_decodes::<Foo>(
            [(u32::MAX, OV::bool(false)), (u32::MAX, OV::bool(true))],
            UnexpectedlyRepeated,
        );
        assert::decodes_non_canonically(
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
        );
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
        assert::decodes_distinguished(
            combined,
            [
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
            .into_opaque_message(),
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
        assert::is_invalid::<OpaqueMessage>(&combined, TagOverflowed);
        assert::is_invalid::<()>(&combined, TagOverflowed);

        let mut first_tag_too_big = Vec::new();
        // This is the first varint that's always an invalid field key.
        encode_varint((u32::MAX as u64 + 1) << 2, &mut first_tag_too_big);
        // Nothing should ever be able to decode this message either; it's not a valid encoding.
        assert::is_invalid::<OpaqueMessage>(&first_tag_too_big, TagOverflowed);
        assert::is_invalid_distinguished::<OpaqueMessage>(&first_tag_too_big, TagOverflowed);
        assert::is_invalid::<()>(&first_tag_too_big, TagOverflowed);
        assert::is_invalid_distinguished::<()>(&first_tag_too_big, TagOverflowed);
    }

    #[test]
    fn truncated_field_and_tag() {
        #[derive(Debug, PartialEq, Eq, Message, DistinguishedMessage)]
        struct Foo(#[bilrost(100)] String, #[bilrost(1_000_000)] u64);

        let buf = [(100, OV::string("abc")), (1_000_000, OV::Varint(1))]
            .into_opaque_message()
            .encode_to_vec();
        // Remove the last field's value and part of its key
        assert::is_invalid::<Foo>(&buf[..buf.len() - 2], Truncated);
        assert::is_invalid::<OpaqueMessage>(&buf[..buf.len() - 2], Truncated);
        assert::is_invalid::<()>(&buf[..buf.len() - 2], Truncated);
        assert::is_invalid_distinguished::<Foo>(&buf[..buf.len() - 2], Truncated);
        assert::is_invalid_distinguished::<OpaqueMessage>(&buf[..buf.len() - 2], Truncated);
        // Just remove the value from the last field
        assert::is_invalid::<Foo>(&buf[..buf.len() - 1], Truncated);
        assert::is_invalid::<OpaqueMessage>(&buf[..buf.len() - 1], Truncated);
        assert::is_invalid::<()>(&buf[..buf.len() - 1], Truncated);
        assert::is_invalid_distinguished::<Foo>(&buf[..buf.len() - 1], Truncated);
        assert::is_invalid_distinguished::<OpaqueMessage>(&buf[..buf.len() - 1], Truncated);
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

        assert::decodes(
            [(1, OV::i64(1)), (2, OV::i64(-2))],
            FooPlus {
                x: 1,
                y: -2,
                also: 0,
            },
        );

        let mut foo = FooPlus {
            x: 5,
            y: 10,
            also: 123,
        };
        assert::decodes(
            foo.encode_to_vec(),
            FooPlus {
                x: 5,
                y: 10,
                also: 0,
            },
        );

        foo.replace_from(
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
            foo,
            FooPlus {
                x: 6,
                y: 12,
                also: 123,
            }
        );

        assert_eq!(
            foo.replace_from(
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
            foo,
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
            FooPlus::empty(),
            FooPlus {
                x: 0,
                y: 0,
                also: 12345,
            }
        );

        assert::decodes(
            [(1, OV::i64(1))],
            FooPlus {
                x: 1,
                y: 0,
                also: 12345,
            },
        )
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
            #[cfg(feature = "smallvec")]
            smallvec: SmallVec<[u32; 1]>,
            #[cfg(feature = "thin-vec")]
            thin_vec: ThinVec<u32>,
            #[cfg(feature = "tinyvec")]
            tinyvec: TinyVec<[u32; 1]>,
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
                    #[cfg(feature = "smallvec")]
                    smallvec: SmallVec::with_capacity(64),
                    #[cfg(feature = "thin-vec")]
                    thin_vec: ThinVec::with_capacity(64),
                    #[cfg(feature = "tinyvec")]
                    tinyvec: TinyVec::with_capacity(64),
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
                #[cfg(feature = "thin-vec")]
                result.thin_vec.push(1);
                #[cfg(feature = "tinyvec")]
                result.tinyvec.push(1);
                #[cfg(feature = "hashbrown")]
                result.hbmap.insert(1, 1);
                #[cfg(feature = "hashbrown")]
                result.hbset.insert(1);
                result
            }
        }

        let mut clearable = Clearable::default();
        assert!(!clearable.is_empty());
        clearable.clear();
        assert_eq!(clearable, Clearable::empty());
        assert!(clearable.is_empty());
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
        #[cfg(feature = "thin-vec")]
        assert!(clearable.thin_vec.capacity() >= 64);
        #[cfg(feature = "tinyvec")]
        assert!(clearable.tinyvec.capacity() >= 64);
        #[cfg(feature = "hashbrown")]
        assert!(clearable.hbmap.capacity() >= 64);
        #[cfg(feature = "hashbrown")]
        assert!(clearable.hbset.capacity() >= 64);

        assert::decodes(Clearable::default().encode_to_vec(), Clearable::default());
        assert::decodes([], Clearable::empty());
    }

    // Varint tests

    #[test]
    fn parsing_varints() {
        #[derive(Debug, PartialEq, Eq, Message, DistinguishedMessage)]
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
        );

        assert::decodes_distinguished([], Foo::empty());
        assert::decodes_distinguished(
            (1..=9).map(|tag| (tag, OV::Varint(1))),
            Foo(true, 1, -1, 1, -1, 1, -1, 1, -1),
        );
        for field in (1..=9).cartesian_product([
            // Currently it is not supported to parse fixed-width values into varint fields.
            OV::fixed_u32(1),
            OV::fixed_u64(1),
            // Length-delimited values don't represent integers either.
            OV::string("1"),
        ]) {
            assert::never_decodes::<Foo>([field], WrongWireType);
        }
        for (tag, out_of_range) in [
            (1, 2),
            (2, 256),
            (3, 256),
            (4, 65536),
            (5, 65536),
            (6, 1 << 32),
            (7, 1 << 32),
        ] {
            assert::never_decodes::<Foo>([(tag, OV::u64(out_of_range))], OutOfDomainValue);
            let should_fit = [(tag, OV::u64(out_of_range - 1))];
            assert::decodes_distinguished(&should_fit, Foo::from_opaque(&should_fit));
        }
    }

    #[test]
    fn bools() {
        #[derive(Debug, PartialEq, Eq, Message, DistinguishedMessage)]
        struct Foo(bool);

        assert_eq!(OV::bool(false), OV::Varint(0));
        assert_eq!(OV::bool(true), OV::Varint(1));

        assert::decodes_distinguished([], Foo(false));
        assert::decodes_non_canonically([(1, OV::bool(false))], Foo(false), NotCanonical);
        assert::decodes_distinguished([(1, OV::bool(true))], Foo(true));
        assert::never_decodes::<Foo>([(1, OV::Varint(2))], OutOfDomainValue);
    }

    #[test]
    fn truncated_varint() {
        #[derive(Debug, PartialEq, Eq, Message, DistinguishedMessage)]
        struct Foo<T>(T);

        let buf = [(1, OV::Varint(1_000_000))]
            .into_opaque_message()
            .encode_to_vec();
        assert::is_invalid::<OpaqueMessage>(&buf[..buf.len() - 1], Truncated);
        assert::is_invalid::<Foo<u32>>(&buf[..buf.len() - 1], Truncated);
        assert::is_invalid::<Foo<u64>>(&buf[..buf.len() - 1], Truncated);
        assert::is_invalid::<Foo<i32>>(&buf[..buf.len() - 1], Truncated);
        assert::is_invalid::<Foo<i64>>(&buf[..buf.len() - 1], Truncated);
        assert::is_invalid::<Foo<bool>>(&buf[..buf.len() - 1], Truncated);
    }

    #[test]
    fn truncated_nested_varint() {
        #[derive(Debug, PartialEq, Eq, Message, DistinguishedMessage)]
        struct Inner(u64);

        #[derive(Debug, PartialEq, Eq, Message, DistinguishedMessage)]
        struct Outer(Inner);

        let truncated_inner_invalid =
            // \x05: field 1, length-delimited; \x04: 4 bytes; \x04: field 1, varint;
            // \xff...: data that will be greedily decoded as an invalid varint.
            b"\x05\x04\x04\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff";
        let truncated_inner_valid =
            // \x05: field 1, length-delimited; \x04: 4 bytes; \x04: field 1, varint;
            // \xff...: data that will be greedily decoded as an valid varint that still runs over.
            b"\x05\x04\x04\xff\xff\xff\xff\xff\x00\x01\x02\x03\x04\x05\x06\x07\x08\x09";

        // The desired result is that we can tell the difference between the inner region being
        // truncated before the varint ends and finding an invalid varint fully inside the inner
        // region.
        assert::is_invalid::<Outer>(truncated_inner_invalid, Truncated);
        assert::is_invalid_distinguished::<Outer>(truncated_inner_invalid, Truncated);
        // When decoding a varint succeeds but runs over, we want to detect that too.
        assert::is_invalid::<Outer>(truncated_inner_valid, Truncated);
        assert::is_invalid_distinguished::<Outer>(truncated_inner_valid, Truncated);
    }

    // Fixed width int tests

    #[test]
    fn parsing_fixed_width_ints() {
        #[derive(Debug, PartialEq, Eq, Message, DistinguishedMessage)]
        struct Foo(
            #[bilrost(encoding(fixed))] u32,
            #[bilrost(encoding(fixed))] i32,
            #[bilrost(encoding(fixed))] u64,
            #[bilrost(encoding(fixed))] i64,
        );

        assert::decodes_distinguished([], Foo::empty());
        assert::decodes_distinguished(
            [
                (1, OV::fixed_u32(1)),
                (2, OV::fixed_u32(1)),
                (3, OV::fixed_u64(1)),
                (4, OV::fixed_u64(1)),
            ],
            Foo(1, 1, 1, 1),
        );
        for varint_value in [
            [(1, OV::Varint(1))],
            [(2, OV::Varint(1))],
            [(3, OV::Varint(1))],
            [(4, OV::Varint(1))],
        ] {
            // Currently it is not supported to parse varint values into varint fields.
            assert::never_decodes::<Foo>(varint_value, WrongWireType);
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

        for wrong_size_value in &[[(1, OV::f64(1.0))], [(2, OV::f32(2.0))]] {
            assert::doesnt_decode::<Foo>(wrong_size_value, WrongWireType);
            assert::doesnt_decode::<Bar>(wrong_size_value, WrongWireType);
        }
    }

    #[test]
    fn preserves_floating_point_special_values() {
        let present_zeros = [(1, OV::fixed_u32(0)), (2, OV::fixed_u64(0))];
        let negative_zeros = [
            (1, OV::ThirtyTwoBit([0, 0, 0, 0x80])),
            (2, OV::SixtyFourBit([0, 0, 0, 0, 0, 0, 0, 0x80])),
        ];
        let infinities = [(1, OV::f32(f32::INFINITY)), (2, OV::f64(f64::NEG_INFINITY))];
        let nans = [
            (1, OV::fixed_u32(0xffff_4321)),
            (2, OV::fixed_u64(0x7fff_dead_beef_cafe)),
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
        assert::decodes(&infinities, Foo(f32::INFINITY, f64::NEG_INFINITY));
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
        // necessarily in expedient mode (floats don't impl `Eq`)
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
        assert::decodes(&infinities, Bar(f32::INFINITY, f64::NEG_INFINITY));
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
        // necessarily in expedient mode (floats don't impl `Eq`)
        let decoded = Bar::from_opaque(&present_zeros);
        assert_eq!((decoded.0.to_bits(), decoded.1.to_bits()), (0, 0));
    }

    #[test]
    fn floating_point_zero_is_present_nested() {
        #[derive(Debug, Message)]
        struct Inner(f32);

        #[derive(Debug, Message)]
        struct Outer(Inner);

        assert!(!Inner(-0.0).is_empty());
        assert!(!Outer(Inner(-0.0)).is_empty());
        assert::encodes(
            Outer(Inner(-0.0)),
            [(1, OV::message(&[(1, OV::f32(-0.0))].into_opaque_message()))],
        );
        let decoded = Outer::from_opaque(Outer(Inner(-0.0)).encode_to_vec());
        assert_eq!(decoded.0 .0.to_bits(), (-0.0f32).to_bits());
    }

    #[test]
    fn truncated_fixed() {
        #[derive(Debug, PartialEq, Eq, Oneof, DistinguishedOneof)]
        enum A<T> {
            Empty,
            #[bilrost(tag(1), encoding(fixed))]
            One(T),
        }

        #[derive(Debug, PartialEq, Eq, Message, DistinguishedMessage)]
        struct Foo<T>(#[bilrost(oneof(1))] A<T>, #[bilrost(encoding(fixed))] T);

        fn check_fixed_truncation<T>(val: OV)
        where
            T: Debug
                + Eq
                + EmptyState
                + encoding::DistinguishedEncoder<Fixed>
                + encoding::DistinguishedValueEncoder<Fixed>
                + encoding::ValueEncoder<Fixed>,
        {
            let mut direct = [(1, val.clone())].into_opaque_message().encode_to_vec();
            let mut in_oneof = [(2, val.clone())].into_opaque_message().encode_to_vec();
            // Truncate by 1 byte
            direct.pop();
            in_oneof.pop();
            assert::is_invalid::<Foo<T>>(&direct, Truncated);
            assert::is_invalid::<Foo<T>>(&in_oneof, Truncated);
            assert::is_invalid_distinguished::<Foo<T>>(&direct, Truncated);
            assert::is_invalid_distinguished::<Foo<T>>(&in_oneof, Truncated);
            assert::is_invalid::<OpaqueMessage>(&direct, Truncated);
            assert::is_invalid::<OpaqueMessage>(&in_oneof, Truncated);
            assert::is_invalid_distinguished::<OpaqueMessage>(&direct, Truncated);
            assert::is_invalid_distinguished::<OpaqueMessage>(&in_oneof, Truncated);
            assert::is_invalid::<()>(&direct, Truncated);
            assert::is_invalid::<()>(&in_oneof, Truncated);

            #[derive(Debug, PartialEq, Eq, Message, DistinguishedMessage)]
            struct Outer<T>(Foo<T>, String);

            let direct_nested = [
                (1, OV::byte_slice(&direct)),
                (2, OV::string("more data after that")),
            ]
            .into_opaque_message()
            .encode_to_vec();
            let in_oneof_nested = [
                (1, OV::byte_slice(&in_oneof)),
                (2, OV::string("more data after that")),
            ]
            .into_opaque_message()
            .encode_to_vec();
            assert::is_invalid::<Outer<T>>(&direct_nested, Truncated);
            assert::is_invalid::<Outer<T>>(&in_oneof_nested, Truncated);
            assert::is_invalid_distinguished::<Outer<T>>(&direct_nested, Truncated);
            assert::is_invalid_distinguished::<Outer<T>>(&in_oneof_nested, Truncated);
        }

        check_fixed_truncation::<u32>(OV::fixed_u32(0x1234abcd));
        check_fixed_truncation::<i32>(OV::fixed_u32(0x1234abcd));
        check_fixed_truncation::<u64>(OV::fixed_u64(0x1234deadbeefcafe));
        check_fixed_truncation::<i64>(OV::fixed_u64(0x1234deadbeefcafe));
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

    fn parsing_string_type<'a, T>()
    where
        T: 'a + Debug + Eq + From<&'a str> + EmptyState + encoding::DistinguishedEncoder<General>,
    {
        #[derive(Debug, PartialEq, Eq, Message, DistinguishedMessage)]
        struct Foo<T>(T);

        assert::decodes_distinguished(
            [(1, OV::string("hello world"))],
            Foo::<T>("hello world".into()),
        );
        let mut invalid_strings = Vec::<Vec<u8>>::from([
            b"bad byte: \xff can't appear in utf-8".as_slice().into(),
            b"non-canonical representation \xc0\x80 of nul byte"
                .as_slice()
                .into(),
        ]);

        invalid_strings.extend((0xd800u32..=0xdfff).map(|surrogate_codepoint| {
            let mut invalid_with_surrogate: Vec<u8> = b"string with surrogate: ".as_slice().into();
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
            assert::never_decodes::<Foo<T>>([(1, OV::byte_slice(&*invalid_string))], InvalidValue);
        }
    }

    #[test]
    fn parsing_strings() {
        parsing_string_type::<String>();
        parsing_string_type::<Cow<str>>();
        #[cfg(feature = "bytestring")]
        parsing_string_type::<bytestring::ByteString>();
    }

    #[test]
    fn owned_empty_cow_str_is_still_empty() {
        let owned_empty = Cow::<str>::Owned(String::with_capacity(32));
        assert!(owned_empty.is_empty());

        #[derive(Message)]
        struct Foo<'a>(Cow<'a, str>);

        assert::encodes(Foo(Cow::Borrowed("")), []);
        assert::encodes(Foo(owned_empty), []);
    }

    // Blob tests

    #[test]
    fn parsing_blob() {
        #[derive(Debug, PartialEq, Eq, Message, DistinguishedMessage)]
        struct Foo(bilrost::Blob);
        assert::decodes_distinguished(
            [(1, OV::string("hello world"))],
            Foo(b"hello world"[..].into()),
        );
    }

    #[test]
    fn parsing_vec_blob() {
        #[derive(Debug, PartialEq, Eq, Message, DistinguishedMessage)]
        struct Foo(#[bilrost(encoding(plainbytes))] Vec<u8>);
        assert::decodes_distinguished(
            [(1, OV::string("hello world"))],
            Foo(b"hello world"[..].into()),
        );
    }

    #[test]
    fn parsing_cow_blob() {
        #[derive(Debug, PartialEq, Eq, Message, DistinguishedMessage)]
        struct Foo<'a>(#[bilrost(encoding(plainbytes))] Cow<'a, [u8]>);
        assert::decodes_distinguished(
            [(1, OV::string("hello world"))],
            Foo(b"hello world"[..].into()),
        );
    }

    #[test]
    fn parsing_bytes_blob() {
        #[derive(Debug, PartialEq, Eq, Message, DistinguishedMessage)]
        struct Foo(bytes::Bytes);
        assert::decodes_distinguished(
            [(1, OV::string("hello world"))],
            Foo(b"hello world"[..].into()),
        );
    }

    #[test]
    fn parsing_byte_arrays() {
        #[derive(Debug, PartialEq, Eq, Message, DistinguishedMessage)]
        struct Foo<const N: usize>(#[bilrost(encoding(plainbytes))] [u8; N]);

        assert::decodes_distinguished([], Foo([]));
        assert::decodes_non_canonically([(1, OV::bytes([]))], Foo([]), NotCanonical);
        assert::never_decodes::<Foo<0>>([(1, OV::bytes([1]))], InvalidValue);

        assert::decodes_distinguished([(1, OV::bytes([1, 2, 3, 4]))], Foo([1, 2, 3, 4]));
        assert::decodes_non_canonically([(1, OV::bytes([0; 4]))], Foo([0; 4]), NotCanonical);
        assert::never_decodes::<Foo<4>>([(1, OV::bytes([1; 3]))], InvalidValue);
        assert::never_decodes::<Foo<4>>([(1, OV::bytes([1; 5]))], InvalidValue);
        assert::never_decodes::<Foo<4>>([(1, OV::fixed_u32(1))], WrongWireType);

        assert::decodes_distinguished([(1, OV::bytes([13; 13]))], Foo([13; 13]));
        assert::decodes_non_canonically([(1, OV::bytes([0; 13]))], Foo([0; 13]), NotCanonical);

        // Fixed-size wire types are implemented for appropriately sized u8 arrays
        #[derive(Debug, PartialEq, Eq, Message, DistinguishedMessage)]
        struct Bar<const N: usize>(#[bilrost(encoding(fixed))] [u8; N]);

        static_assertions::assert_not_impl_any!(Bar<0>: Message, DistinguishedMessage);
        static_assertions::assert_not_impl_any!(Bar<2>: Message, DistinguishedMessage);
        static_assertions::assert_not_impl_any!(Bar<16>: Message, DistinguishedMessage);
        assert::decodes_distinguished([(1, OV::fixed_u32(0x04030201))], Bar([1, 2, 3, 4]));
        assert::decodes_non_canonically([(1, OV::fixed_u32(0))], Bar([0; 4]), NotCanonical);
        assert::decodes_distinguished([(1, OV::SixtyFourBit([8; 8]))], Bar([8; 8]));
        assert::decodes_non_canonically([(1, OV::SixtyFourBit([0; 8]))], Bar([0; 8]), NotCanonical);
        assert::never_decodes::<Bar<8>>([(1, OV::bytes([8; 8]))], WrongWireType);
    }

    // Repeated field tests

    #[test]
    fn duplicated_field_decoding() {
        #[derive(Debug, PartialEq, Eq, Message, DistinguishedMessage)]
        struct Foo(Option<bool>, bool);

        assert::decodes_distinguished([(1, OV::bool(false))], Foo(Some(false), false));
        assert::never_decodes::<Foo>(
            [(1, OV::bool(false)), (1, OV::bool(true))],
            UnexpectedlyRepeated,
        );
        assert::decodes_distinguished([(2, OV::bool(true))], Foo(None, true));
        assert::never_decodes::<Foo>(
            [(2, OV::bool(true)), (2, OV::bool(false))],
            UnexpectedlyRepeated,
        );
    }

    #[test]
    fn duplicated_packed_decoding() {
        #[derive(Debug, PartialEq, Eq, Message, DistinguishedMessage)]
        struct Foo(#[bilrost(encoding = "packed")] Vec<bool>);
        #[derive(Debug, PartialEq, Eq, Message, DistinguishedMessage)]
        struct Bar(#[bilrost(encoding = "unpacked")] Vec<bool>);

        assert::decodes_distinguished([(1, OV::packed([OV::bool(true)]))], Foo(vec![true]));
        assert::decodes_non_canonically(
            [(1, OV::packed([OV::bool(true)]))],
            Bar(vec![true]),
            NotCanonical,
        );

        assert::decodes_distinguished(
            [(1, OV::packed([OV::bool(true), OV::bool(false)]))],
            Foo(vec![true, false]),
        );
        assert::decodes_non_canonically(
            [(1, OV::packed([OV::bool(true), OV::bool(false)]))],
            Bar(vec![true, false]),
            NotCanonical,
        );

        // Two packed fields should never decode
        assert::never_decodes::<Foo>(
            [
                (1, OV::packed([OV::bool(true), OV::bool(false)])),
                (1, OV::packed([OV::bool(false)])),
            ],
            UnexpectedlyRepeated,
        );
        assert::never_decodes::<Bar>(
            [
                (1, OV::packed([OV::bool(true), OV::bool(false)])),
                (1, OV::packed([OV::bool(false)])),
            ],
            UnexpectedlyRepeated,
        );

        // Packed followed by unpacked should never decode
        assert::never_decodes::<Foo>(
            [
                (1, OV::packed([OV::bool(true), OV::bool(false)])),
                (1, OV::bool(false)),
            ],
            UnexpectedlyRepeated,
        );
        assert::never_decodes::<Bar>(
            [
                (1, OV::packed([OV::bool(true), OV::bool(false)])),
                (1, OV::bool(false)),
            ],
            UnexpectedlyRepeated,
        );

        // Unpacked followed by packed should never decode
        assert::never_decodes::<Foo>(
            [
                (1, OV::bool(true)),
                (1, OV::bool(false)),
                (1, OV::packed([OV::bool(false)])),
            ],
            WrongWireType,
        );
        assert::never_decodes::<Bar>(
            [
                (1, OV::bool(true)),
                (1, OV::bool(false)),
                (1, OV::packed([OV::bool(false)])),
            ],
            WrongWireType,
        );
    }

    // Map tests

    #[test]
    fn decoding_maps() {
        #[derive(Debug, PartialEq, Eq, Message, DistinguishedMessage)]
        struct Foo<T>(T);

        let valid_map = &[(
            1,
            OV::packed([
                OV::bool(false),
                OV::string("no"),
                OV::bool(true),
                OV::string("yes"),
            ]),
        )];
        let disordered_map = &[(
            1,
            OV::packed([
                OV::bool(true),
                OV::string("yes"),
                OV::bool(false),
                OV::string("no"),
            ]),
        )];
        let repeated_map = &[(
            1,
            OV::packed([
                OV::bool(false),
                OV::string("indecipherable"),
                OV::bool(false),
                OV::string("could mean anything"),
            ]),
        )];

        {
            use std::collections::BTreeMap;
            assert::decodes_distinguished(
                valid_map,
                Foo(BTreeMap::from([
                    (false, "no".to_string()),
                    (true, "yes".to_string()),
                ])),
            );
            assert::decodes_non_canonically(
                disordered_map,
                Foo(BTreeMap::from([
                    (false, "no".to_string()),
                    (true, "yes".to_string()),
                ])),
                NotCanonical,
            );
            assert::never_decodes::<Foo<BTreeMap<bool, String>>>(
                repeated_map,
                UnexpectedlyRepeated,
            );
        }
        #[allow(unused_macros)]
        macro_rules! test_hash {
            ($ty:ident) => {
                for map_value in [valid_map, disordered_map] {
                    assert::decodes(
                        map_value,
                        Foo($ty::from([
                            (false, "no".to_string()),
                            (true, "yes".to_string()),
                        ])),
                    );
                }
                assert::doesnt_decode::<Foo<$ty<bool, String>>>(repeated_map, UnexpectedlyRepeated);
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

    fn truncated_bool_string_map<T>()
    where
        T: Debug + EmptyState + Mapping<Key = bool, Value = String> + encoding::Encoder<General>,
    {
        #[derive(Debug, PartialEq, Message)]
        struct Foo<T>(T, String);

        let OV::LengthDelimited(map_value) = OV::packed([
            OV::bool(false),
            OV::string("no"),
            OV::bool(true),
            OV::string("yes"),
        ]) else {
            unreachable!()
        };
        assert::doesnt_decode::<Foo<T>>(
            [
                (1, OV::byte_slice(&map_value[..map_value.len() - 1])),
                (2, OV::string("another field after that")),
            ],
            Truncated,
        );
    }

    fn truncated_string_int_map<T>()
    where
        T: Debug + EmptyState + Mapping<Key = String, Value = u64> + encoding::Encoder<General>,
    {
        #[derive(Debug, PartialEq, Message)]
        struct Foo<T>(T, String);

        let OV::LengthDelimited(map_value) = OV::packed([
            OV::string("zero"),
            OV::u64(0),
            OV::string("lots"),
            OV::u64(999999999999999),
        ]) else {
            unreachable!()
        };
        assert::doesnt_decode::<Foo<T>>(
            [
                (1, OV::byte_slice(&map_value[..map_value.len() - 1])),
                (2, OV::string("another field after that")),
            ],
            Truncated,
        );
    }

    #[test]
    fn truncated_map() {
        {
            use std::collections::BTreeMap;
            truncated_bool_string_map::<BTreeMap<bool, String>>();
            truncated_string_int_map::<BTreeMap<String, u64>>();
        }
        #[cfg(feature = "std")]
        {
            use std::collections::HashMap;
            truncated_bool_string_map::<HashMap<bool, String>>();
            truncated_string_int_map::<HashMap<String, u64>>();
        }
        #[cfg(feature = "hashbrown")]
        {
            use hashbrown::HashMap;
            truncated_bool_string_map::<HashMap<bool, String>>();
            truncated_string_int_map::<HashMap<String, u64>>();
        }
    }

    // Vec tests

    #[test]
    fn decoding_vecs() {
        #[derive(Debug, PartialEq, Eq, Message, DistinguishedMessage)]
        struct Foo<T>(
            #[bilrost(encoding(packed))] T,
            #[bilrost(encoding(unpacked))] T,
        );

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
            assert::decodes_distinguished(packed, Foo(expected.clone(), vec![]));
            assert::decodes_distinguished(unpacked, Foo(vec![], expected.clone()));
            assert::decodes_distinguished(
                packed,
                Foo(Cow::Borrowed(expected.as_slice()), Cow::default()),
            );
            assert::decodes_distinguished(
                unpacked,
                Foo(Cow::default(), Cow::Borrowed(expected.as_slice())),
            );
            assert::decodes_distinguished(
                packed,
                Foo(Cow::Owned(expected.clone()), Cow::default()),
            );
            assert::decodes_distinguished(
                unpacked,
                Foo(Cow::default(), Cow::Owned(expected.clone())),
            );
            #[allow(unused_macros)]
            macro_rules! test_vec {
                ($vec_ty:ty) => {
                    assert::decodes_distinguished(
                        packed,
                        Foo(expected.iter().cloned().collect(), <$vec_ty>::new()),
                    );
                    assert::decodes_distinguished(
                        unpacked,
                        Foo(<$vec_ty>::new(), expected.iter().cloned().collect()),
                    );
                };
            }
            #[cfg(feature = "smallvec")]
            test_vec!(smallvec::SmallVec<[String; 2]>);
            #[cfg(feature = "thin-vec")]
            test_vec!(thin_vec::ThinVec<String>);
            #[cfg(feature = "tinyvec")]
            test_vec!(tinyvec::TinyVec<[String; 2]>);
        }
    }

    #[test]
    fn decoding_vecs_with_swapped_packedness() {
        #[derive(Debug, PartialEq, Eq, Message, DistinguishedMessage)]
        struct Oof<T>(
            #[bilrost(encoding(unpacked))] T, // Fields have swapped packedness from `Foo` above
            #[bilrost(encoding(packed))] T,
        );

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

        // In expedient mode, packed sets will decode unpacked values and vice versa, but this is
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
            assert::decodes_non_canonically(packed, Oof(expected.clone(), vec![]), NotCanonical);
            assert::decodes_non_canonically(unpacked, Oof(vec![], expected.clone()), NotCanonical);
            assert::decodes_non_canonically(
                packed,
                Oof(Cow::Borrowed(expected.as_slice()), Cow::default()),
                NotCanonical,
            );
            assert::decodes_non_canonically(
                unpacked,
                Oof(Cow::default(), Cow::Borrowed(expected.as_slice())),
                NotCanonical,
            );
            assert::decodes_non_canonically(
                packed,
                Oof(Cow::Owned(expected.clone()), Cow::default()),
                NotCanonical,
            );
            assert::decodes_non_canonically(
                unpacked,
                Oof(Cow::default(), Cow::Owned(expected.clone())),
                NotCanonical,
            );
            #[allow(unused_macros)]
            macro_rules! test_vec {
                ($vec_ty:ty) => {
                    assert::decodes_non_canonically(
                        packed,
                        Oof(expected.iter().cloned().collect(), <$vec_ty>::new()),
                        NotCanonical,
                    );
                    assert::decodes_non_canonically(
                        unpacked,
                        Oof(<$vec_ty>::new(), expected.iter().cloned().collect()),
                        NotCanonical,
                    );
                };
            }
            #[cfg(feature = "smallvec")]
            test_vec!(smallvec::SmallVec<[u32; 2]>);
            #[cfg(feature = "thin-vec")]
            test_vec!(thin_vec::ThinVec<u32>);
            #[cfg(feature = "tinyvec")]
            test_vec!(tinyvec::TinyVec<[u32; 2]>);
        }
    }

    // Set tests

    #[test]
    fn decoding_sets() {
        #[derive(Debug, PartialEq, Eq, Message, DistinguishedMessage)]
        struct Foo<T>(
            #[bilrost(encoding(packed))] T,
            #[bilrost(encoding(unpacked))] T,
        );

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
            assert::decodes_distinguished(
                valid_set_packed,
                Foo(BTreeSet::from(expected_items.clone()), BTreeSet::new()),
            );
            assert::decodes_distinguished(
                valid_set_unpacked,
                Foo(BTreeSet::new(), BTreeSet::from(expected_items.clone())),
            );
            assert::decodes_non_canonically(
                disordered_set_packed,
                Foo(BTreeSet::from(expected_items.clone()), BTreeSet::new()),
                NotCanonical,
            );
            assert::decodes_non_canonically(
                disordered_set_unpacked,
                Foo(BTreeSet::new(), BTreeSet::from(expected_items.clone())),
                NotCanonical,
            );
            assert::never_decodes::<Foo<BTreeSet<String>>>(
                &repeated_set_packed,
                UnexpectedlyRepeated,
            );
            assert::never_decodes::<Foo<BTreeSet<String>>>(
                &repeated_set_unpacked,
                UnexpectedlyRepeated,
            );
        }
        #[allow(unused_macros)]
        macro_rules! test_hash {
            ($ty:ident) => {
                for (set_value_packed, set_value_unpacked) in [&valid, &disordered] {
                    assert::decodes(
                        set_value_packed,
                        Foo($ty::from(expected_items.clone()), $ty::new()),
                    );
                    assert::decodes(
                        set_value_unpacked,
                        Foo($ty::new(), $ty::from(expected_items.clone())),
                    );
                }
                assert::doesnt_decode::<Foo<$ty<String>>>(
                    repeated_set_packed,
                    UnexpectedlyRepeated,
                );
                assert::doesnt_decode::<Foo<$ty<String>>>(
                    repeated_set_unpacked,
                    UnexpectedlyRepeated,
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
        #[derive(Debug, PartialEq, Eq, Message, DistinguishedMessage)]
        struct Oof<T>(
            #[bilrost(encoding(unpacked))] T, // Fields have swapped packedness from `Foo` above
            #[bilrost(encoding(packed))] T,
        );

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

        // In expedient mode, packed sets will decode unpacked values and vice versa, but this is
        // only detectable when the values are not length-delimited.
        {
            use std::collections::BTreeSet;
            for (unmatching_packed, unmatching_unpacked) in [&valid, &disordered] {
                assert::decodes_non_canonically(
                    unmatching_packed,
                    Oof(BTreeSet::from(expected_items), BTreeSet::new()),
                    NotCanonical,
                );
                assert::decodes_non_canonically(
                    unmatching_unpacked,
                    Oof(BTreeSet::new(), BTreeSet::from(expected_items)),
                    NotCanonical,
                );
            }
            assert::never_decodes::<Oof<BTreeSet<u32>>>(&repeated_set_packed, UnexpectedlyRepeated);
            assert::never_decodes::<Oof<BTreeSet<u32>>>(
                &repeated_set_unpacked,
                UnexpectedlyRepeated,
            );
        }
        #[allow(unused_macros)]
        macro_rules! test_hash {
            ($ty:ident) => {
                for (set_value_packed, set_value_unpacked) in [&valid, &disordered] {
                    assert::decodes(
                        set_value_packed,
                        Oof($ty::from(expected_items.clone()), $ty::new()),
                    );
                    assert::decodes(
                        set_value_unpacked,
                        Oof($ty::new(), $ty::from(expected_items.clone())),
                    );
                }
                assert::doesnt_decode::<Oof<$ty<u32>>>(repeated_set_packed, UnexpectedlyRepeated);
                assert::doesnt_decode::<Oof<$ty<u32>>>(repeated_set_unpacked, UnexpectedlyRepeated);
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

    fn truncated_packed_string<T>()
    where
        T: Debug
            + EmptyState
            + Collection<Item = String>
            + encoding::Encoder<General>
            + encoding::Encoder<Packed>,
    {
        #[derive(Debug, PartialEq, Message)]
        struct Foo<T>(#[bilrost(encoding(packed))] T, String);

        let OV::LengthDelimited(set_value) =
            OV::packed([OV::string("fooble"), OV::string("barbaz")])
        else {
            unreachable!()
        };
        assert::doesnt_decode::<Foo<T>>(
            [
                (1, OV::byte_slice(&set_value[..set_value.len() - 1])),
                (2, OV::string("another field after that")),
            ],
            Truncated,
        );
    }

    fn truncated_packed_int<T>()
    where
        T: Debug + EmptyState + Collection<Item = u64> + encoding::Encoder<General>,
    {
        #[derive(Debug, PartialEq, Message)]
        struct Foo<T>(T, String);

        let packed = OV::packed([OV::u64(0), OV::u64(999999999999999)]);
        let OV::LengthDelimited(map_value) = packed else {
            unreachable!()
        };
        assert::doesnt_decode::<Foo<T>>(
            [
                (1, OV::byte_slice(&map_value[..map_value.len() - 1])),
                (2, OV::string("another field after that")),
            ],
            Truncated,
        );
    }

    #[test]
    fn truncated_packed_collection() {
        {
            use std::vec::Vec;
            truncated_packed_string::<Vec<String>>();
            truncated_packed_int::<Vec<u64>>();
        }
        {
            truncated_packed_string::<Cow<[String]>>();
            truncated_packed_int::<Cow<[u64]>>();
        }
        #[cfg(feature = "smallvec")]
        {
            use smallvec::SmallVec;
            truncated_packed_string::<SmallVec<[String; 2]>>();
            truncated_packed_int::<SmallVec<[u64; 2]>>();
        }
        #[cfg(feature = "thin-vec")]
        {
            use thin_vec::ThinVec;
            truncated_packed_string::<ThinVec<String>>();
            truncated_packed_int::<ThinVec<u64>>();
        }
        #[cfg(feature = "tinyvec")]
        {
            use tinyvec::TinyVec;
            truncated_packed_string::<TinyVec<[String; 2]>>();
            truncated_packed_int::<TinyVec<[u64; 2]>>();
        }
        {
            use std::collections::BTreeSet;
            truncated_packed_string::<BTreeSet<String>>();
            truncated_packed_int::<BTreeSet<u64>>();
        }
        #[cfg(feature = "std")]
        {
            use std::collections::HashSet;
            truncated_packed_string::<HashSet<String>>();
            truncated_packed_int::<HashSet<u64>>();
        }
        #[cfg(feature = "hashbrown")]
        {
            use hashbrown::HashSet;
            truncated_packed_string::<HashSet<String>>();
            truncated_packed_int::<HashSet<u64>>();
        }
    }

    // Oneof tests

    #[test]
    fn oneof_field_decoding() {
        #[derive(Debug, PartialEq, Eq, Oneof, DistinguishedOneof)]
        enum AB {
            #[bilrost(1)]
            A(bool),
            #[bilrost(2)]
            B(bool),
        }
        use AB::*;

        #[derive(Debug, PartialEq, Eq, Message, DistinguishedMessage)]
        struct Foo(#[bilrost(oneof = "1, 2")] Option<AB>);

        assert::decodes_distinguished([(1, OV::bool(true))], Foo(Some(A(true))));
        assert::decodes_distinguished([(2, OV::bool(false))], Foo(Some(B(false))));
        assert::never_decodes::<Foo>(
            [(1, OV::bool(false)), (2, OV::bool(true))],
            ConflictingFields,
        );
    }

    #[test]
    fn oneof_optioned_fields_encode_empty() {
        #[derive(Clone, Debug, PartialEq, Eq, Oneof, DistinguishedOneof)]
        enum Abc {
            #[bilrost(1)]
            A(String),
            #[bilrost(2)]
            B { named: u32 },
            #[bilrost(tag = 3, encoding = "packed")]
            C(Vec<bool>),
        }
        use Abc::*;

        #[derive(Clone, Debug, PartialEq, Eq, Message, DistinguishedMessage)]
        struct Foo(#[bilrost(oneof(1, 2, 3))] Option<Abc>);

        assert::decodes_distinguished([], Foo(None));

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
            assert::decodes_distinguished(opaque, value.clone());
        }
    }

    #[test]
    fn oneof_plain_fields_encode_empty() {
        /// Oneofs that have an empty variant
        #[derive(Clone, Debug, PartialEq, Eq, Oneof, DistinguishedOneof)]
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

        #[derive(Clone, Debug, PartialEq, Eq, Message, DistinguishedMessage)]
        struct Foo(#[bilrost(oneof(1, 2, 3))] Abc);

        assert::decodes_distinguished([], Foo(Empty));

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
            assert::decodes_distinguished(opaque, value.clone());
        }
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

        #[derive(Clone, Debug, PartialEq, Eq, Message, DistinguishedMessage)]
        struct Foo(Option<DefaultButNoZero>, HasZero);

        assert::decodes_distinguished([], Foo(None, Zero));
        assert::decodes_distinguished([(1, OV::u32(5))], Foo(Some(Five), Zero));
        assert::decodes_distinguished([(1, OV::u32(10))], Foo(Some(Ten), Zero));
        assert::decodes_distinguished([(1, OV::u32(15))], Foo(Some(Fifteen), Zero));
        assert::decodes_non_canonically([(2, OV::u32(0))], Foo(None, Zero), NotCanonical);
        assert::decodes_distinguished([(2, OV::u32(1_000))], Foo(None, Big));
        assert::decodes_distinguished([(2, OV::u32(1_000_000))], Foo(None, Bigger));
    }

    #[test]
    fn enumeration_helpers() {
        #[derive(Clone, Debug, PartialEq, Eq, Enumeration)]
        enum E {
            Five = 5,
            Ten = 10,
            Fifteen = 15,
        }

        #[derive(Clone, Debug, PartialEq, Eq, Message, DistinguishedMessage)]
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
        assert::decodes_distinguished([(1, OV::u32(222)), (2, OV::u32(222))], val.clone());
        val.regular()
            .expect_err("bad enumeration value parsed successfully");
        val.optional()
            .unwrap()
            .expect_err("bad enumeration value parsed successfully");

        let val = HelpedStruct::empty();
        assert_eq!(val.optional(), None);

        // Demonstrate that the same errors happen when we decode to a struct with strict
        // enumeration fields, it just happens sooner.
        #[derive(Clone, Debug, PartialEq, Eq, Message, DistinguishedMessage)]
        struct StrictStruct(Option<E>, Option<E>);

        for val in [
            HelpedStruct {
                regular: 222,
                optional: None,
            },
            HelpedStruct {
                regular: 5,
                optional: Some(222),
            },
        ] {
            assert::never_decodes::<StrictStruct>(val.encode_to_vec(), OutOfDomainValue);
        }
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
        assert_eq!(core::mem::size_of::<Foo>(), 1);

        #[derive(Debug, PartialEq, Eq, Message, DistinguishedMessage)]
        struct Bar(Foo);

        assert_eq!(Foo::A.to_number(), 0);
        assert_eq!(Foo::Z.to_number(), u32::MAX);
        assert_eq!(Foo::try_from_number(u32::MAX), Ok(Foo::Z));
        assert_eq!(Foo::Z as u8, 255);
        assert::decodes_distinguished([], Bar(Foo::A));
        assert::decodes_non_canonically([(1, OV::u32(0))], Bar(Foo::A), NotCanonical);
        assert::decodes_distinguished([(1, OV::u32(5))], Bar(Foo::D));
        assert::decodes_distinguished([(1, OV::u32(10))], Bar(Foo::T));
        assert::decodes_distinguished([(1, OV::u32(11))], Bar(Foo::E));
        assert::decodes_distinguished([(1, OV::u32(u32::MAX))], Bar(Foo::Z));
    }

    // Nested message tests

    #[test]
    fn directly_included_message() {
        #[derive(Clone, Debug, PartialEq, Eq, Message, DistinguishedMessage)]
        struct Inner {
            a: String,
            b: i64,
        }

        #[derive(Clone, Debug, PartialEq, Eq, Message, DistinguishedMessage)]
        struct OuterDirect {
            inner: Inner,
            also: String,
        }

        #[derive(Debug, PartialEq, Eq, Message, DistinguishedMessage)]
        struct OuterOptional {
            inner: Option<Inner>,
            also: Option<String>,
        }

        // With a directly included inner message field, it should encode only when its value is
        // empty.

        // When the inner message is empty, it doesn't encode.
        assert::decodes_distinguished(
            [(2, OV::string("abc"))],
            OuterDirect {
                inner: EmptyState::empty(),
                also: "abc".into(),
            },
        );
        assert::decodes_distinguished(
            [(2, OV::string("abc"))],
            OuterOptional {
                inner: None,
                also: Some("abc".into()),
            },
        );

        // When the inner message is present in the encoding but empty, it's only canonical when
        // the field is optioned.
        assert::decodes_non_canonically(
            [(1, OV::message(&[].into_opaque_message()))],
            OuterDirect::empty(),
            NotCanonical,
        );
        assert::decodes_distinguished(
            [(1, OV::message(&[].into_opaque_message()))],
            OuterOptional {
                inner: Some(EmptyState::empty()),
                also: None,
            },
        );

        // The inner message is included when it is not fully empty
        assert::decodes_distinguished(
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
        assert::decodes_distinguished(
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

        assert::never_decodes::<OuterDirect>([(1, OV::Varint(1))], WrongWireType);
        assert::never_decodes::<OuterOptional>([(1, OV::Varint(1))], WrongWireType);
        assert::never_decodes::<OuterDirect>([(1, OV::ThirtyTwoBit([1; 4]))], WrongWireType);
        assert::never_decodes::<OuterOptional>([(1, OV::ThirtyTwoBit([1; 4]))], WrongWireType);
        assert::never_decodes::<OuterDirect>([(1, OV::SixtyFourBit([1; 8]))], WrongWireType);
        assert::never_decodes::<OuterOptional>([(1, OV::SixtyFourBit([1; 8]))], WrongWireType);
    }

    #[test]
    fn truncated_submessage() {
        #[derive(Debug, PartialEq, Eq, Message, DistinguishedMessage)]
        struct Nested(String);
        #[derive(Debug, PartialEq, Eq, Message, DistinguishedMessage)]
        struct Foo(Nested, String);

        let inner = [(1, OV::string("interrupting cow says"))]
            .into_opaque_message()
            .encode_to_vec();
        assert::never_decodes::<Foo>(
            [
                (1, OV::byte_slice(&inner[..inner.len() - 1])),
                (2, OV::string("moo")),
            ],
            Truncated,
        );
    }

    #[test]
    fn unknown_fields_distinguished() {
        #[derive(Debug, PartialEq, Eq, Message, DistinguishedMessage)]
        struct Nested(i64);

        #[derive(Debug, PartialEq, Eq, Oneof, DistinguishedOneof)]
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

        #[derive(Debug, PartialEq, Eq, Message, DistinguishedMessage)]
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

        assert::decodes_distinguished(
            [
                (0, OV::string("hello")),
                (3, OV::message(&[(1, OV::i64(301))].into_opaque_message())),
                (4, OV::message(&[(1, OV::i64(555))].into_opaque_message())),
            ],
            Foo {
                zero: "hello".into(),
                four: Some(Nested(555)),
                oneof: Three(Nested(301)),
                ..EmptyState::empty()
            },
        );
        assert::decodes_non_canonically(
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
                ..EmptyState::empty()
            },
            HasExtensions,
        );
        assert::decodes_non_canonically(
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
                ..EmptyState::empty()
            },
            HasExtensions,
        );
        assert::decodes_non_canonically(
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
                ..EmptyState::empty()
            },
            HasExtensions,
        );

        // We should be sensitive to multiple tiers of non-canonicity
        assert::decodes_distinguished(
            [
                (1, OV::u64(1)),
                (3, OV::message(&[(1, OV::i64(1))].into_opaque_message())),
            ],
            Foo {
                one: 1,
                oneof: Three(Nested(1)),
                ..EmptyState::empty()
            },
        );
        // We can see when there are extensions in both the inner and outer message...
        assert::decodes_non_canonically(
            [
                (1, OV::u64(1)),
                (
                    3,
                    OV::message(
                        &[(1, OV::i64(1)), (2, OV::string("unknown"))].into_opaque_message(),
                    ),
                ),
            ],
            Foo {
                one: 1,
                oneof: Three(Nested(1)),
                ..EmptyState::empty()
            },
            HasExtensions,
        );
        assert::decodes_non_canonically(
            [
                (1, OV::u64(1)),
                (2, OV::string("unknown")),
                (3, OV::message(&[(1, OV::i64(1))].into_opaque_message())),
            ],
            Foo {
                one: 1,
                oneof: Three(Nested(1)),
                ..EmptyState::empty()
            },
            HasExtensions,
        );
        // and a non-canonical field that occurs later in the message overrides the canonicity to
        // the worse `NotCanonical`
        assert::decodes_non_canonically(
            [
                (1, OV::u64(1)),
                (2, OV::string("unknown")),
                (3, OV::message(&[(1, OV::i64(0))].into_opaque_message())),
            ],
            Foo {
                one: 1,
                oneof: Three(Nested(0)),
                ..EmptyState::empty()
            },
            NotCanonical,
        );
    }
}
