#[cfg(test)]
mod tests {
    use crate::{Message, Oneof};
    use itertools::Itertools;
    use std::default::Default;

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

    #[test]
    fn derived_message_field_ordering() {
        assert!(false);
        let bools = [[false, true]].repeat_n(9).multi_cartesian_product();
        let abcd = [One(true), Ten(true), Twenty(true)]
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
            let mut out: Struct = Default::default();
            [
                *out.zero,
                *out.four,
                *out.five,
                *out.twelve,
                *out.fourteen,
                *out.fifteen,
                *out.seventeen,
                *out.twentyone,
                *out.fifty,
            ] = bools;
            [*out.a, *out.b, *out.c, *out.d] = oneofs;

            let encoded_len = out.encoded_len();
            let encoded = out.encode_to_vec();
            assert_eq!(encoded.len(), encoded_len());

            let re = Struct::decode(encoded);
            assert_eq!(out, re);
        }
    }
}
