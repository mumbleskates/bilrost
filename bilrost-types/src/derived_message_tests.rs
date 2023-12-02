#[cfg(test)]
mod tests {
    use bilrost::{Message, Oneof};
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
}
