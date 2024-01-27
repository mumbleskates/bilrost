#![cfg(test)]
extern crate alloc;

use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use core::default::Default;
use core::fmt::Debug;

use itertools::{repeat_n, Itertools};

use bilrost::encoding::opaque::{OpaqueMessage, OpaqueValue as OV};
use bilrost::DecodeErrorKind::*;
use bilrost::{DecodeErrorKind, DistinguishedMessage, Enumeration, Message, Oneof};

trait IntoOpaqueMessage {
    fn into_opaque_message(self) -> OpaqueMessage;
}

impl<T> IntoOpaqueMessage for &T
where T: Clone + IntoOpaqueMessage {
    fn into_opaque_message(self) -> OpaqueMessage {
        self.clone().into_opaque_message()
    }
}

impl<const N: usize> IntoOpaqueMessage for [(u32, OV); N]
{
    fn into_opaque_message(self) -> OpaqueMessage {
        OpaqueMessage::from_iter(self)
    }
}

impl IntoOpaqueMessage for OpaqueMessage
{
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

    pub(super) fn decodes<M>(from: impl IntoOpaqueMessage<>, into: M)
    where
        M: Message + Debug + PartialEq,
    {
        let encoded = from.into_opaque_message().encode_to_vec();
        assert_eq!(M::decode(encoded.as_slice()), Ok(into));
    }

    pub(super) fn doesnt_decode<M>(from: impl IntoOpaqueMessage, err: DecodeErrorKind)
    where
        M: Message + Debug + PartialEq,
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
        assert_ne!(
            encoded,
            into.encode_to_vec(),
            "encoding round tripped, but did not decode distinguished"
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
    }
}

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
        let re = Struct::decode(&encoded[..]).unwrap();
        assert_eq!(out, re);
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
    assert::encodes(
        Foo(f32::INFINITY, f64::NEG_INFINITY),
        &infinities,
    );
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

#[test]
fn oneof_field_decoding() {
    #[derive(Message)]
    struct Foo {
        #[bilrost(1)]
        a: Option<bool>,
        #[bilrost(2)]
        b: Option<bool>,
    }

    #[derive(Debug, PartialEq, Oneof)]
    enum AB {
        #[bilrost(1)]
        A(bool),
        #[bilrost(2)]
        B(bool),
    }
    use AB::*;

    #[derive(Debug, PartialEq, Message)]
    struct Bar {
        #[bilrost(oneof = "1, 2")]
        ab: Option<AB>,
    }

    let a_only = Foo {
        a: Some(true),
        b: None,
    }
    .encode_to_vec();
    let b_only = Foo {
        a: None,
        b: Some(false),
    }
    .encode_to_vec();
    let both = Foo {
        a: Some(false),
        b: Some(true),
    }
    .encode_to_vec();

    assert_eq!(Bar::decode(&a_only[..]), Ok(Bar { ab: Some(A(true)) }));
    assert_eq!(Bar::decode(&b_only[..]), Ok(Bar { ab: Some(B(false)) }));
    assert_eq!(
        Bar::decode(&both[..]).unwrap_err().kind(),
        ConflictingFields
    );
}

#[test]
fn oneof_optioned_fields_encode_empty() {
    #[derive(Debug, PartialEq, Oneof)]
    enum Abc {
        #[bilrost(1)]
        A(String),
        #[bilrost(2)]
        B { named: u32 },
        #[bilrost(tag = 3, encoder = "packed")]
        C(Vec<bool>),
    }
    use Abc::*;

    #[derive(Debug, PartialEq, Message)]
    struct Foo {
        #[bilrost(oneof(1, 2, 3))]
        abc: Option<Abc>,
    }

    for value in [
        Foo { abc: None },
        Foo {
            abc: Some(A(Default::default())),
        },
        Foo {
            abc: Some(A("something".to_string())),
        },
        Foo {
            abc: Some(B {
                named: Default::default(),
            }),
        },
        Foo {
            abc: Some(B { named: 123 }),
        },
        Foo {
            abc: Some(C(Default::default())),
        },
        Foo {
            abc: Some(C(vec![false])),
        },
    ] {
        let encoded = value.encode_to_vec();
        let decoded = Foo::decode(encoded.as_slice()).unwrap();
        assert_eq!(value, decoded);
    }
}

#[test]
fn oneof_plain_fields_encode_empty() {
    /// Oneofs that have an empty variant
    #[derive(Debug, PartialEq, Oneof)]
    enum Abc {
        /// No fields
        Empty,
        #[bilrost(1)]
        A(String),
        #[bilrost(2)]
        B { named: u32 },
        #[bilrost(tag = 3, encoder = "packed")]
        C(Vec<bool>),
    }
    use Abc::*;

    #[derive(Debug, PartialEq, Message)]
    struct Foo {
        #[bilrost(oneof(1, 2, 3))]
        abc: Abc,
    }

    for value in [
        Foo { abc: Empty },
        Foo {
            abc: A(Default::default()),
        },
        Foo {
            abc: A("something".to_string()),
        },
        Foo {
            abc: B {
                named: Default::default(),
            },
        },
        Foo {
            abc: B { named: 123 },
        },
        Foo {
            abc: C(Default::default()),
        },
        Foo {
            abc: C(vec![false]),
        },
    ] {
        let encoded = value.encode_to_vec();
        let decoded = Foo::decode(encoded.as_slice()).unwrap();
        assert_eq!(value, decoded);
    }
}

#[test]
fn enumeration_decoding() {
    #[derive(Clone, Debug, PartialEq, Eq, Enumeration)]
    enum E {
        Five = 5,
        Ten = 10,
        Fifteen = 15,
    }

    #[derive(Clone, Debug, Default, PartialEq, Eq, Enumeration)]
    enum F {
        #[default]
        Big = 1000,
        Bigger = 1_000_000,
        Biggest = 1_000_000_000,
    }

    #[derive(Clone, Debug, PartialEq, Eq, Message, DistinguishedMessage)]
    struct EnumStruct {
        no_default: Option<E>,
        has_default: F,
    }

    for (no_default, has_default) in [None, Some(E::Five), Some(E::Ten), Some(E::Fifteen)]
        .into_iter()
        .cartesian_product([F::Big, F::Bigger, F::Biggest])
    {
        let out = EnumStruct {
            no_default,
            has_default,
        };
        let encoded_len = out.encoded_len();
        let encoded = out.encode_to_vec();
        assert_eq!(encoded.len(), encoded_len);
        let re = EnumStruct::decode(encoded.as_slice()).unwrap();
        assert_eq!(out, re);
        let re_distinguished = EnumStruct::decode_distinguished(encoded.as_slice()).unwrap();
        assert_eq!(out, re_distinguished);
    }
}

#[test]
fn enumeration_helpers() {
    #[derive(Clone, Debug, PartialEq, Eq, Enumeration)]
    enum E {
        Five = 5,
        Ten = 10,
        Fifteen = 15,
    }

    #[derive(Clone, Debug, PartialEq, Message)]
    struct HelpedStruct {
        #[bilrost(enumeration(E))]
        regular: u32,
        #[bilrost(enumeration(E))]
        optional: Option<u32>,
    }

    #[derive(Clone, Debug, PartialEq, Message)]
    struct StrictStruct {
        regular: Option<E>,
        optional: Option<E>,
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
        let encoded = val.encode_to_vec();
        let decoded = StrictStruct::decode(encoded.as_slice());
        assert_eq!(
            decoded
                .expect_err("decoded an invalid enumeration value without error")
                .kind(),
            OutOfDomainValue
        );
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
    let val = OuterDirect {
        inner: Default::default(),
        also: "abc".into(),
    };
    let encoded = val.encode_to_vec();
    assert_eq!(OuterDirect::decode(encoded.as_slice()), Ok(val.clone()));
    assert_eq!(
        OuterDirect::decode_distinguished(encoded.as_slice()),
        Ok(val.clone())
    );
    assert_eq!(
        OuterOptional::decode(encoded.as_slice()),
        Ok(OuterOptional {
            inner: None,
            also: Some("abc".into())
        })
    );
    assert_eq!(
        OuterOptional::decode_distinguished(encoded.as_slice()),
        Ok(OuterOptional {
            inner: None,
            also: Some("abc".into())
        })
    );

    // The inner message is included when it is not fully defaulted
    let val = OuterDirect {
        inner: Inner {
            a: "def".into(),
            b: 0,
        },
        also: "abc".into(),
    };
    let encoded = val.encode_to_vec();
    assert_eq!(OuterDirect::decode(encoded.as_slice()), Ok(val.clone()),);
    assert_eq!(
        OuterDirect::decode_distinguished(encoded.as_slice()),
        Ok(val.clone())
    );
    assert_eq!(
        OuterOptional::decode(encoded.as_slice()),
        Ok(OuterOptional {
            inner: Some(Inner {
                a: "def".into(),
                b: 0
            }),
            also: Some("abc".into())
        })
    );
    assert_eq!(
        OuterOptional::decode_distinguished(encoded.as_slice()),
        Ok(OuterOptional {
            inner: Some(Inner {
                a: "def".into(),
                b: 0
            }),
            also: Some("abc".into())
        })
    );
}
