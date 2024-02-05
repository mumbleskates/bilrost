//! This file should contain most of the specific tests for the observed behavior and available
//! types of bilrost messages and their fields. If there's an observed behavior in a type of message
//! or field that we implement, we want to demonstrate it here.

fn main() {
    println!("This file is meant to contain tests, so we can use the proc macros within it.")
}

#[cfg(test)]
mod derived_message_tests {
    extern crate alloc;

    use alloc::borrow::Cow;
    use alloc::string::{String, ToString};
    use alloc::vec;
    use alloc::vec::Vec;
    use core::default::Default;
    use core::fmt::Debug;

    use itertools::{repeat_n, Itertools};

    use bilrost::encoding::opaque::{OpaqueMessage, OpaqueValue as OV};
    use bilrost::encoding::{
        encode_varint, Collection, DistinguishedEncoder, DistinguishedOneof,
        DistinguishedValueEncoder, Encoder, General, HasEmptyState, Mapping, Oneof, Packed,
        ValueEncoder,
    };
    use bilrost::DecodeErrorKind::{
        ConflictingFields, InvalidValue, NotCanonical, OutOfDomainValue, TagOverflowed, Truncated,
        UnexpectedlyRepeated, UnknownField, WrongWireType,
    };
    use bilrost::{DecodeErrorKind, DistinguishedMessage, Enumeration, Message, Oneof};
    use bilrost_derive::DistinguishedOneof;

    trait IntoOpaqueMessage {
        fn into_opaque_message(self) -> OpaqueMessage;
    }

    impl<T> IntoOpaqueMessage for &T
    where
        T: Clone + IntoOpaqueMessage,
    {
        fn into_opaque_message(self) -> OpaqueMessage {
            self.clone().into_opaque_message()
        }
    }

    impl<const N: usize> IntoOpaqueMessage for [(u32, OV); N] {
        fn into_opaque_message(self) -> OpaqueMessage {
            OpaqueMessage::from_iter(self)
        }
    }

    impl IntoOpaqueMessage for &[(u32, OV)] {
        fn into_opaque_message(self) -> OpaqueMessage {
            OpaqueMessage::from_iter(self.iter().cloned())
        }
    }

    impl IntoOpaqueMessage for Vec<u8> {
        fn into_opaque_message(self) -> OpaqueMessage {
            OpaqueMessage::decode(self.as_slice()).expect("did not decode")
        }
    }

    impl IntoOpaqueMessage for OpaqueMessage {
        fn into_opaque_message(self) -> OpaqueMessage {
            self
        }
    }

    trait FromOpaque {
        fn from_opaque(from: impl IntoOpaqueMessage) -> Self;
    }

    impl<T: Message> FromOpaque for T {
        fn from_opaque(from: impl IntoOpaqueMessage) -> Self {
            Self::decode(&*from.into_opaque_message().encode_to_vec()).expect("failed to decode")
        }
    }

    mod assert {
        use super::*;

        pub(super) fn decodes<M>(from: impl IntoOpaqueMessage, into: M)
        where
            M: Message + Debug + PartialEq,
        {
            let encoded = from.into_opaque_message().encode_to_vec();
            assert_eq!(M::decode(encoded.as_slice()), Ok(into));
        }

        pub(super) fn doesnt_decode<M>(from: impl IntoOpaqueMessage, err: DecodeErrorKind)
        where
            M: Message + Debug,
        {
            let encoded = from.into_opaque_message().encode_to_vec();
            assert_eq!(
                M::decode(encoded.as_slice())
                    .expect_err("unexpectedly decoded without error")
                    .kind(),
                err
            );
        }

        pub(super) fn doesnt_decode_distinguished<M>(
            from: impl IntoOpaqueMessage,
            err: DecodeErrorKind,
        ) where
            M: DistinguishedMessage + Debug,
        {
            let encoded = from.into_opaque_message().encode_to_vec();
            assert_eq!(
                M::decode_distinguished(encoded.as_slice())
                    .expect_err("unexpectedly decoded without error")
                    .kind(),
                err
            );
        }

        pub(super) fn decodes_distinguished<M>(from: impl IntoOpaqueMessage, into: M)
        where
            M: DistinguishedMessage + Debug + Eq,
        {
            let encoded = from.into_opaque_message().encode_to_vec();
            assert_eq!(M::decode(encoded.as_slice()).as_ref(), Ok(&into));
            assert_eq!(
                M::decode_distinguished(encoded.as_slice()).as_ref(),
                Ok(&into)
            );
            assert_eq!(
                encoded,
                into.encode_to_vec(),
                "distinguished encoding does not round trip"
            );
            assert_eq!(into.encoded_len(), encoded.len(), "encoded_len was wrong");
        }

        pub(super) fn decodes_only_expedient<M>(
            from: impl IntoOpaqueMessage,
            into: M,
            err: DecodeErrorKind,
        ) where
            M: DistinguishedMessage + Debug + Eq,
        {
            let encoded = from.into_opaque_message().encode_to_vec();
            assert_eq!(M::decode(encoded.as_slice()).as_ref(), Ok(&into));
            assert_eq!(
                M::decode_distinguished(encoded.as_slice())
                    .expect_err("unexpectedly decoded in distinguished mode without error")
                    .kind(),
                err
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

        pub(super) fn never_decodes<M>(from: impl IntoOpaqueMessage, err: DecodeErrorKind)
        where
            M: DistinguishedMessage + Debug,
        {
            let encoded = from.into_opaque_message().encode_to_vec();
            assert_eq!(
                M::decode(encoded.as_slice())
                    .expect_err("unepectedly decoded in expedient mode without error")
                    .kind(),
                err
            );
            assert_eq!(
                M::decode_distinguished(encoded.as_slice())
                    .expect_err("unexpectedly decoded in distinguished mode without error")
                    .kind(),
                err
            );
        }

        pub(super) fn encodes<M: Message>(value: M, becomes: impl IntoOpaqueMessage) {
            let encoded = value.encode_to_vec();
            assert_eq!(
                OpaqueMessage::decode(&*encoded),
                Ok(becomes.into_opaque_message())
            );
            assert_eq!(value.encoded_len(), encoded.len(), "encoded_len was wrong");
        }

        pub(super) fn is_invalid<M: Message + Debug>(
            value: impl AsRef<[u8]>,
            err: DecodeErrorKind,
        ) {
            assert_eq!(
                M::decode(&mut value.as_ref())
                    .expect_err("decoded without error")
                    .kind(),
                err
            );
        }

        pub(super) fn is_invalid_distinguished<M: DistinguishedMessage + Debug>(
            value: impl AsRef<[u8]>,
            err: DecodeErrorKind,
        ) {
            assert_eq!(
                M::decode_distinguished(&mut value.as_ref())
                    .expect_err("decoded without error")
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
        assert::decodes_only_expedient(
            [
                (0, OV::bool(true)),
                (234234234, OV::string("unknown")), // unknown field
                (u32::MAX, OV::bool(false)),
            ],
            Foo {
                minimum: Some(true),
                maximum: Some(false),
            },
            UnknownField,
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

    // Varint tests

    #[test]
    fn parsing_varints() {
        #[derive(Debug, PartialEq, Eq, Message, DistinguishedMessage)]
        struct Foo(u32, i32, u64, i64);

        assert::decodes_distinguished([], Foo::default());
        assert::decodes_distinguished(
            [
                (1, OV::Varint(1)),
                (2, OV::Varint(1)),
                (3, OV::Varint(1)),
                (4, OV::Varint(1)),
            ],
            Foo(1, -1, 1, -1),
        );
        for fixed_value in [
            // Currently it is not supported to parse fixed-width values into varint fields.
            [(1, OV::fixed_u32(1))],
            [(2, OV::fixed_u32(1))],
            [(3, OV::fixed_u32(1))],
            [(4, OV::fixed_u32(1))],
            [(1, OV::fixed_u64(1))],
            [(2, OV::fixed_u64(1))],
            [(3, OV::fixed_u64(1))],
            [(4, OV::fixed_u64(1))],
            // Length-delimited values don't represent integers either.
            [(1, OV::string("1"))],
            [(2, OV::string("1"))],
            [(3, OV::string("1"))],
            [(4, OV::string("1"))],
        ] {
            assert::never_decodes::<Foo>(fixed_value, WrongWireType);
        }
        for out_of_range in [u32::MAX as u64 + 1, 1_000_000_000_000, u64::MAX] {
            assert::never_decodes::<Foo>([(1, OV::u64(out_of_range))], OutOfDomainValue);
            assert::never_decodes::<Foo>([(2, OV::u64(out_of_range))], OutOfDomainValue);
        }
    }

    #[test]
    fn bools() {
        #[derive(Debug, PartialEq, Eq, Message, DistinguishedMessage)]
        struct Foo(bool);

        assert_eq!(OV::bool(false), OV::Varint(0));
        assert_eq!(OV::bool(true), OV::Varint(1));

        assert::decodes_distinguished([], Foo(false));
        assert::decodes_only_expedient([(1, OV::bool(false))], Foo(false), NotCanonical);
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
            #[bilrost(encoder(fixed))] u32,
            #[bilrost(encoder(fixed))] i32,
            #[bilrost(encoder(fixed))] u64,
            #[bilrost(encoder(fixed))] i64,
        );

        assert::decodes_distinguished([], Foo::default());
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
            #[bilrost(encoder(fixed))] f32,
            #[bilrost(encoder(fixed))] f64,
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
            nans.clone(),
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
            #[bilrost(encoder(fixed))] f32,
            #[bilrost(encoder(fixed))] f64,
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
            #[bilrost(tag(1), encoder(fixed))]
            One(T),
        }

        #[derive(Debug, PartialEq, Eq, Message, DistinguishedMessage)]
        struct Foo<T>(#[bilrost(oneof(1))] A<T>, #[bilrost(encoder(fixed))] T);

        fn check_fixed_truncation<T>(val: OV)
        where
            T: Debug + Default + Eq + HasEmptyState,
            bilrost::encoding::Fixed:
                DistinguishedEncoder<T> + ValueEncoder<T> + DistinguishedValueEncoder<T>,
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
                (1, OV::blob(direct)),
                (2, OV::string("more data after that")),
            ]
            .into_opaque_message()
            .encode_to_vec();
            let in_oneof_nested = [
                (1, OV::blob(in_oneof)),
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
        T: 'a + Debug + Default + Eq + From<&'a str> + HasEmptyState,
        General: DistinguishedEncoder<T>,
    {
        #[derive(Debug, PartialEq, Eq, Message, DistinguishedMessage)]
        struct Foo<T>(T);

        assert::decodes_distinguished([(1, OV::string("hello world"))], Foo("hello world".into()));
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
            assert::never_decodes::<Foo<T>>([(1, OV::blob(&*invalid_string))], InvalidValue);
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
        struct Foo(#[bilrost(encoder(vecblob))] Vec<u8>);
        assert::decodes_distinguished(
            [(1, OV::string("hello world"))],
            Foo(b"hello world"[..].into()),
        );
    }

    #[test]
    fn parsing_cow_blob() {
        #[derive(Debug, PartialEq, Eq, Message, DistinguishedMessage)]
        struct Foo<'a>(#[bilrost(encoder(vecblob))] Cow<'a, [u8]>);
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
        struct Foo(#[bilrost(encoder = "packed")] Vec<bool>);

        assert::decodes_distinguished([(1, OV::packed([OV::bool(true)]))], Foo(vec![true]));
        assert::decodes_distinguished(
            [(1, OV::packed([OV::bool(true), OV::bool(false)]))],
            Foo(vec![true, false]),
        );
        assert::never_decodes::<Foo>(
            [
                (1, OV::packed([OV::bool(true), OV::bool(false)])),
                (1, OV::packed([OV::bool(false)])),
            ],
            UnexpectedlyRepeated,
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
            use alloc::collections::BTreeMap;
            assert::decodes_distinguished(
                valid_map,
                Foo(BTreeMap::from([
                    (false, "no".to_string()),
                    (true, "yes".to_string()),
                ])),
            );
            assert::decodes_only_expedient(
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
        T: Debug + Default + HasEmptyState + Mapping<Key = bool, Value = String>,
        General: Encoder<T>,
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
                (1, OV::blob(&map_value[..map_value.len() - 1])),
                (2, OV::string("another field after that")),
            ],
            Truncated,
        );
    }

    fn truncated_string_int_map<T>()
    where
        T: Debug + Default + HasEmptyState + Mapping<Key = String, Value = u64>,
        General: Encoder<T>,
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
                (1, OV::blob(&map_value[..map_value.len() - 1])),
                (2, OV::string("another field after that")),
            ],
            Truncated,
        );
    }

    #[test]
    fn truncated_map() {
        {
            use alloc::collections::BTreeMap;
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
            #[bilrost(encoder(packed))] T,
            #[bilrost(encoder(unpacked))] T,
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
                }
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
            #[bilrost(encoder(unpacked))] T, // Fields have swapped packedness from `Foo` above
            #[bilrost(encoder(packed))] T,
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
            assert::decodes_only_expedient(packed, Oof(expected.clone(), vec![]), WrongWireType);
            assert::decodes_only_expedient(unpacked, Oof(vec![], expected.clone()), WrongWireType);
            assert::decodes_only_expedient(
                packed,
                Oof(Cow::Borrowed(expected.as_slice()), Cow::default()),
                WrongWireType,
            );
            assert::decodes_only_expedient(
                unpacked,
                Oof(Cow::default(), Cow::Borrowed(expected.as_slice())),
                WrongWireType,
            );
            assert::decodes_only_expedient(
                packed,
                Oof(Cow::Owned(expected.clone()), Cow::default()),
                WrongWireType,
            );
            assert::decodes_only_expedient(
                unpacked,
                Oof(Cow::default(), Cow::Owned(expected.clone())),
                WrongWireType,
            );
            #[allow(unused_macros)]
            macro_rules! test_vec {
                ($vec_ty:ty) => {
                    assert::decodes_only_expedient(
                        packed,
                        Oof(expected.iter().cloned().collect(), <$vec_ty>::new()),
                        WrongWireType,
                    );
                    assert::decodes_only_expedient(
                        unpacked,
                        Oof(<$vec_ty>::new(), expected.iter().cloned().collect()),
                        WrongWireType,
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
            #[bilrost(encoder(packed))] T,
            #[bilrost(encoder(unpacked))] T,
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
            use alloc::collections::BTreeSet;
            assert::decodes_distinguished(
                valid_set_packed,
                Foo(BTreeSet::from(expected_items.clone()), BTreeSet::new()),
            );
            assert::decodes_distinguished(
                valid_set_unpacked,
                Foo(BTreeSet::new(), BTreeSet::from(expected_items.clone())),
            );
            assert::decodes_only_expedient(
                disordered_set_packed,
                Foo(BTreeSet::from(expected_items.clone()), BTreeSet::new()),
                NotCanonical,
            );
            assert::decodes_only_expedient(
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
            #[bilrost(encoder(unpacked))] T, // Fields have swapped packedness from `Foo` above
            #[bilrost(encoder(packed))] T,
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
            use alloc::collections::BTreeSet;
            for (unmatching_packed, unmatching_unpacked) in [&valid, &disordered] {
                assert::decodes_only_expedient(
                    unmatching_packed,
                    Oof(BTreeSet::from(expected_items), BTreeSet::new()),
                    WrongWireType,
                );
                assert::decodes_only_expedient(
                    unmatching_unpacked,
                    Oof(BTreeSet::new(), BTreeSet::from(expected_items)),
                    WrongWireType,
                );
            }
            assert::doesnt_decode::<Oof<BTreeSet<u32>>>(&repeated_set_packed, UnexpectedlyRepeated);
            assert::doesnt_decode_distinguished::<Oof<BTreeSet<u32>>>(
                &repeated_set_packed,
                WrongWireType,
            );
            assert::doesnt_decode::<Oof<BTreeSet<u32>>>(
                &repeated_set_unpacked,
                UnexpectedlyRepeated,
            );
            assert::doesnt_decode_distinguished::<Oof<BTreeSet<u32>>>(
                &repeated_set_unpacked,
                WrongWireType,
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
        T: Debug + Default + HasEmptyState + Collection<Item = String>,
        General: Encoder<T>,
        Packed: Encoder<T>,
    {
        #[derive(Debug, PartialEq, Message)]
        struct Foo<T>(#[bilrost(encoder(packed))] T, String);

        let OV::LengthDelimited(set_value) =
            OV::packed([OV::string("fooble"), OV::string("barbaz")])
        else {
            unreachable!()
        };
        assert::doesnt_decode::<Foo<T>>(
            [
                (1, OV::blob(&set_value[..set_value.len() - 1])),
                (2, OV::string("another field after that")),
            ],
            Truncated,
        );
    }

    fn truncated_packed_int<T>()
    where
        T: Debug + Default + HasEmptyState + Collection<Item = u64>,
        General: Encoder<T>,
    {
        #[derive(Debug, PartialEq, Message)]
        struct Foo<T>(T, String);

        let OV::LengthDelimited(map_value) = OV::packed([OV::u64(0), OV::u64(999999999999999)])
        else {
            unreachable!()
        };
        assert::doesnt_decode::<Foo<T>>(
            [
                (1, OV::blob(&map_value[..map_value.len() - 1])),
                (2, OV::string("another field after that")),
            ],
            Truncated,
        );
    }

    #[test]
    fn truncated_packed_collection() {
        {
            use alloc::vec::Vec;
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
            use alloc::collections::BTreeSet;
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
            #[bilrost(tag = 3, encoder = "packed")]
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
            #[bilrost(tag = 3, encoder = "packed")]
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
        #[derive(Clone, Debug, PartialEq, Eq, Enumeration)]
        enum NoDefault {
            Five = 5,
            Ten = 10,
            Fifteen = 15,
        }
        use NoDefault::*;

        #[derive(Clone, Debug, Default, PartialEq, Eq, Enumeration)]
        enum HasDefault {
            #[default]
            Big = 1000,
            Bigger = 1_000_000,
            Biggest = 1_000_000_000,
        }
        use HasDefault::*;

        #[derive(Clone, Debug, PartialEq, Eq, Message, DistinguishedMessage)]
        struct Foo(Option<NoDefault>, HasDefault);

        assert::decodes_distinguished([], Foo(None, Big));
        assert::decodes_distinguished([(1, OV::u32(5))], Foo(Some(Five), Big));
        assert::decodes_distinguished([(1, OV::u32(10))], Foo(Some(Ten), Big));
        assert::decodes_distinguished([(1, OV::u32(15))], Foo(Some(Fifteen), Big));
        assert::decodes_only_expedient([(2, OV::u32(1000))], Foo(None, Big), NotCanonical);
        assert::decodes_distinguished([(2, OV::u32(1_000_000))], Foo(None, Bigger));
        assert::decodes_distinguished([(2, OV::u32(1_000_000_000))], Foo(None, Biggest));
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
        assert_eq!(
            val.regular()
                .expect_err("bad enumeration value parsed successfully")
                .kind(),
            OutOfDomainValue,
        );
        assert_eq!(
            val.optional()
                .unwrap()
                .expect_err("bad enumeration value parsed successfully")
                .kind(),
            OutOfDomainValue,
        );

        let val = HelpedStruct::default();
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

        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Enumeration)]
        #[repr(u8)]
        enum Foo {
            A = 0,
            #[bilrost = 5]
            D,
            #[bilrost(TEN)]
            #[default]
            T,
            #[bilrost(u32::MAX)]
            Z,
        }
        assert_eq!(core::mem::size_of::<Foo>(), 1);

        #[derive(Debug, PartialEq, Eq, Message, DistinguishedMessage)]
        struct Bar(Foo);

        assert_eq!(u32::from(Foo::A), 0);
        assert_eq!(u32::from(Foo::Z), u32::MAX);
        assert::encodes(Bar(Foo::A), [(1, OV::u32(0))]);
        assert::encodes(Bar(Foo::D), [(1, OV::u32(5))]);
        assert::encodes(Bar(Foo::T), []);
        assert::encodes(Bar(Foo::Z), [(1, OV::u32(u32::MAX))]);
        assert::decodes_distinguished([(1, OV::u32(0))], Bar(Foo::A));
        assert::decodes_distinguished([(1, OV::u32(5))], Bar(Foo::D));
        assert::decodes_distinguished([], Bar(Foo::T));
        assert::decodes_only_expedient([(1, OV::u32(10))], Bar(Foo::T), NotCanonical);
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
        // defaulted.

        // When the inner message is default, it doesn't encode.
        assert::decodes_distinguished(
            [(2, OV::string("abc"))],
            OuterDirect {
                inner: Default::default(),
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

        // When the inner message is present in the encoding but defaulted, it's only canonical when
        // the field is optioned.
        assert::decodes_only_expedient(
            [(1, OV::message(&[].into_opaque_message()))],
            OuterDirect::default(),
            NotCanonical,
        );
        assert::decodes_distinguished(
            [(1, OV::message(&[].into_opaque_message()))],
            OuterOptional {
                inner: Some(Default::default()),
                also: None,
            },
        );

        // The inner message is included when it is not fully defaulted
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
                (1, OV::blob(&inner[..inner.len() - 1])),
                (2, OV::string("moo")),
            ],
            Truncated,
        );
    }

    #[test]
    fn reject_unknown_fields_distinguished() {
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
                ..Default::default()
            },
        );
        assert::decodes_only_expedient(
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
                ..Default::default()
            },
            UnknownField,
        );
        assert::decodes_only_expedient(
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
                ..Default::default()
            },
            UnknownField,
        );
        assert::decodes_only_expedient(
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
                ..Default::default()
            },
            UnknownField,
        );
    }
}
