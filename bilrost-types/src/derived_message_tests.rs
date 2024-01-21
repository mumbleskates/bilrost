#[cfg(test)]
mod tests {
    extern crate alloc;

    use alloc::string::{String, ToString};
    use alloc::vec;
    use alloc::vec::Vec;
    use bilrost::{Enumeration, Message, Oneof};
    use core::default::Default;
    use itertools::{repeat_n, Itertools};

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

    #[test]
    fn derived_message_field_ordering() {
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
    fn duplicated_field_decoding() {
        #[derive(Message)]
        struct Foo {
            #[bilrost(1)]
            a: Vec<bool>,
            #[bilrost(2)]
            b: Vec<bool>,
        }

        #[derive(Debug, PartialEq, Message)]
        struct Bar {
            #[bilrost(1)]
            a: Option<bool>,
            #[bilrost(2)]
            b: bool,
        }

        let a_single = Foo {
            a: vec![true],
            b: vec![],
        }
        .encode_to_vec();
        let a_duplicated = Foo {
            a: vec![true, false],
            b: vec![],
        }
        .encode_to_vec();
        let b_single = Foo {
            a: vec![],
            b: vec![true],
        }
        .encode_to_vec();
        let b_duplicated = Foo {
            a: vec![],
            b: vec![true, false],
        }
        .encode_to_vec();

        assert_eq!(
            Bar::decode(&a_single[..]),
            Ok(Bar {
                a: Some(true),
                b: false
            })
        );
        assert_eq!(
            Bar::decode(&a_duplicated[..]).unwrap_err().to_string(),
            "failed to decode Bilrost message: \
            Bar.a: multiple occurrences of non-repeated field"
        );
        assert_eq!(Bar::decode(&b_single[..]), Ok(Bar { a: None, b: true }));
        assert_eq!(
            Bar::decode(&b_duplicated[..]).unwrap_err().to_string(),
            "failed to decode Bilrost message: \
            Bar.b: multiple occurrences of non-repeated field"
        );
    }

    #[test]
    fn duplicated_packed_decoding() {
        #[derive(Message)]
        struct Foo {
            #[bilrost(tag = 1, encoder = "unpacked<vecblob>")]
            a: Vec<Vec<u8>>,
        }

        #[derive(Debug, PartialEq, Message)]
        struct Bar {
            #[bilrost(tag = 1, encoder = "packed")]
            a: Vec<bool>,
        }

        let single = Foo { a: vec![vec![1]] }.encode_to_vec();
        let multiple = Foo {
            a: vec![vec![1, 0]],
        }
        .encode_to_vec();
        let duplicated = Foo {
            a: vec![vec![1], vec![0]],
        }
        .encode_to_vec();

        assert_eq!(Bar::decode(&single[..]), Ok(Bar { a: vec![true] }));
        assert_eq!(
            Bar::decode(&multiple[..]),
            Ok(Bar {
                a: vec![true, false]
            })
        );
        assert_eq!(
            Bar::decode(&duplicated[..]).unwrap_err().to_string(),
            "failed to decode Bilrost message: \
            Bar.a: multiple occurrences of packed repeated field"
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
            Bar::decode(&both[..]).unwrap_err().to_string(),
            "failed to decode Bilrost message: Bar.ab: conflicting fields in oneof"
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

        #[derive(Clone, Debug, PartialEq, Message)]
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
            // TODO(widders): enable this
            // let re_distinguished = EnumStruct::decode_distinguished(encoded.as_slice()).unwrap();
            // assert_eq!(out, re_distinguished);
        }
    }
}
