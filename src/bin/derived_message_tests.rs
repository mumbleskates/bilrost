//! This file should contain most of the specific tests for the observed behavior and available
//! types of bilrost messages and their fields. If there's an observed behavior in a type of message
//! or field that we implement, we want to demonstrate it here.

fn main() {
    println!("This file is meant to contain tests, so we can use the proc macros within it.")
}

#[cfg(test)]
mod derived_message_tests {
    extern crate alloc;

    use alloc::string::{String, ToString};
    use alloc::vec;
    use alloc::vec::Vec;
    use core::default::Default;
    use core::fmt::Debug;

    use itertools::{repeat_n, Itertools};

    use bilrost::encoding::opaque::{OpaqueMessage, OpaqueValue as OV};
    use bilrost::encoding::{encode_varint, DistinguishedOneof, HasEmptyState, Oneof};
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

    // TODO(widders): test coverage for completed features:
    //  * truncated values and nested messages

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
        #[derive(Clone, Debug, PartialEq, Oneof)]
        enum A {
            #[bilrost(1)]
            One(bool),
            #[bilrost(10)]
            Ten(bool),
            #[bilrost(20)]
            Twenty(bool),
        }

        #[derive(Clone, Debug, PartialEq, Oneof)]
        enum B {
            #[bilrost(9)]
            Nine(bool),
            #[bilrost(11)]
            Eleven(bool),
        }

        #[derive(Clone, Debug, PartialEq, Oneof)]
        enum C {
            #[bilrost(13)]
            Thirteen(bool),
            #[bilrost(16)]
            Sixteen(bool),
            #[bilrost(22)]
            TwentyTwo(bool),
        }

        #[derive(Clone, Debug, PartialEq, Oneof)]
        enum D {
            #[bilrost(18)]
            Eighteen(bool),
            #[bilrost(19)]
            Nineteen(bool),
        }

        use A::*;
        use B::*;
        use C::*;
        use D::*;

        #[derive(Clone, Debug, PartialEq, Message)]
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

        impl TryFrom<Vec<bool>> for Struct {
            type Error = ();

            fn try_from(value: Vec<bool>) -> Result<Self, ()> {
                match value[..] {
                    [zero, four, five, twelve, fourteen, fifteen, seventeen, twentyone, fifty] => {
                        Ok(Self {
                            zero,
                            four,
                            five,
                            twelve,
                            fourteen,
                            fifteen,
                            seventeen,
                            twentyone,
                            fifty,
                            ..Default::default()
                        })
                    }
                    _ => Err(()),
                }
            }
        }

        // This must be the same as the number of fields we're putting into the struct...
        let bools = repeat_n([false, true], 9).multi_cartesian_product();
        let abcd = [None, Some(One(true)), Some(Ten(true)), Some(Twenty(true))]
            .into_iter()
            .cartesian_product([None, Some(Nine(true)), Some(Eleven(true))])
            .cartesian_product([
                None,
                Some(Thirteen(true)),
                Some(Sixteen(true)),
                Some(TwentyTwo(true)),
            ])
            .cartesian_product([None, Some(Eighteen(true)), Some(Nineteen(true))]);
        for (bools, oneofs) in bools.cartesian_product(abcd) {
            let mut out: Struct = bools.try_into().unwrap();
            (((out.a, out.b), out.c), out.d) = oneofs;

            let encoded_len = out.encoded_len();
            let encoded = out.encode_to_vec();
            assert_eq!(encoded.len(), encoded_len);
            let re = Struct::decode(encoded.as_slice()).unwrap();
            assert_eq!(out, re);
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
        struct Foo<T>(#[bilrost(1_000_000)] T);

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

    // TODO(widders): string tests (including InvalidValue)
    // TODO(widders): bytes tests

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

    // TODO(widders): map tests
    //  * map keys must never recur
    //  * map keys and set values must be ascending in distinguished decoding
    // TODO(widders): collection tests -- vec, sets
    //  * set values must never recur
    //  * repeated fields must have matching packed-ness in distinguished decoding

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
