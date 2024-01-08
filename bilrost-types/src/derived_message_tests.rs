#[cfg(test)]
mod tests {
    extern crate alloc;

    use alloc::vec::Vec;
    use bilrost::{
        alloc::{string::ToString, vec},
        Message, Oneof,
    };
    use core::default::Default;
    use itertools::{repeat_n, Itertools};

    #[derive(Clone, PartialEq, Oneof)]
    enum A {
        #[bilrost(bool, tag = 1)]
        One(bool),
        #[bilrost(bool, tag = 10)]
        Ten(bool),
        #[bilrost(bool, tag = 20)]
        Twenty(bool),
    }

    #[derive(Clone, PartialEq, Oneof)]
    enum B {
        #[bilrost(bool, tag = 9)]
        Nine(bool),
        #[bilrost(bool, tag = 11)]
        Eleven(bool),
    }

    #[derive(Clone, PartialEq, Oneof)]
    enum C {
        #[bilrost(bool, tag = 13)]
        Thirteen(bool),
        #[bilrost(bool, tag = 16)]
        Sixteen(bool),
        #[bilrost(bool, tag = 22)]
        TwentyTwo(bool),
    }

    #[derive(Clone, PartialEq, Oneof)]
    enum D {
        #[bilrost(bool, tag = 18)]
        Eighteen(bool),
        #[bilrost(bool, tag = 19)]
        Nineteen(bool),
    }

    use A::*;
    use B::*;
    use C::*;
    use D::*;

    #[derive(Clone, PartialEq, Message)]
    struct Struct {
        #[bilrost(bool, tag = 0)]
        zero: bool,
        #[bilrost(oneof = "A", tags = "1, 10, 20")]
        a: Option<A>,
        #[bilrost(bool, tag = 4)]
        four: bool,
        #[bilrost(bool, tag = 5)]
        five: bool,
        #[bilrost(oneof = "B", tags = "9, 11")]
        b: Option<B>,
        #[bilrost(bool)] // implicitly tagged 12
        twelve: bool,
        #[bilrost(oneof = "C", tags = "13, 16, 22")]
        c: Option<C>,
        #[bilrost(bool, tag = 14)]
        fourteen: bool,
        #[bilrost(bool)] // implicitly tagged 15
        fifteen: bool,
        #[bilrost(bool, tag = 17)]
        seventeen: bool,
        #[bilrost(oneof = "D", tags = "18, 19")]
        d: Option<D>,
        #[bilrost(bool, tag = 21)]
        twentyone: bool,
        #[bilrost(bool, tag = 50)]
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
            #[bilrost(bool, repeated, packed = false, tag = 1)]
            a: Vec<bool>,
            #[bilrost(bool, repeated, packed = false, tag = 2)]
            b: Vec<bool>,
        }

        #[derive(PartialEq, Message)]
        struct Bar {
            #[bilrost(bool, optional, tag = 1)]
            a: Option<bool>,
            #[bilrost(bool, tag = 2)]
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
            #[bilrost(bytes = "vec", repeated, tag = 1)]
            a: Vec<Vec<u8>>,
        }

        #[derive(PartialEq, Message)]
        struct Bar {
            #[bilrost(bool, repeated, packed = true, tag = 1)]
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
            #[bilrost(bool, optional, tag = 1)]
            a: Option<bool>,
            #[bilrost(bool, optional, tag = 2)]
            b: Option<bool>,
        }

        #[derive(PartialEq, Oneof)]
        enum AB {
            #[bilrost(bool, tag = 1)]
            A(bool),
            #[bilrost(bool, tag = 2)]
            B(bool),
        }
        use AB::*;

        #[derive(PartialEq, Message)]
        struct Bar {
            #[bilrost(oneof = "AB", tags = "1, 2")]
            ab: Option<AB>,
        }

        let a_only = Foo {
            a: Some(true),
            b: None,
        }
        .encode_to_vec();
        let b_only = Foo {
            a: None,
            b: Some(true),
        }
        .encode_to_vec();
        let both = Foo {
            a: Some(true),
            b: Some(true),
        }
        .encode_to_vec();

        assert_eq!(Bar::decode(&a_only[..]), Ok(Bar { ab: Some(A(true)) }));
        assert_eq!(Bar::decode(&b_only[..]), Ok(Bar { ab: Some(B(true)) }));
        assert_eq!(
            Bar::decode(&both[..]).unwrap_err().to_string(),
            "failed to decode Bilrost message: Bar.ab: conflicting or repeating fields in oneof"
        );
    }
}
