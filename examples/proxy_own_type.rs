use bilrost::{Message, OwnedMessage};
use std::collections::BTreeMap;

struct CustomEncoding;
struct Tag;

// When creating a custom encoding, it is almost always desirable to implement the `Encoder` traits
// exclusively in terms of when `ValueEncoder` traits are implemented. This is needed for the
// overwhelming majority of types that are encoded, including all proxy-encoded types; only
// encodings that may take different decoding paths depending on what they encounter need to act
// otherwise, and those use cases should hopefully be covered by `Packed` and `Unpacked`.
bilrost::encoding_implemented_via_value_encoding!(CustomEncoding);

// Unless it's necessary to implement encodings for types that our crate does not own, it makes the
// most sense to always use the "base empty state" and delegate encoding to that, so we can use our
// types with all of `bilrost`'s built-in encodings as much as possible.
bilrost::encoding_uses_base_empty_state!(CustomEncoding);

mod crate_defined_structs {
    #[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
    pub(crate) struct AlwaysOdd(u64);

    impl AlwaysOdd {
        pub fn new(value: u64) -> Result<Self, &'static str> {
            if value % 2 == 1 {
                Ok(Self(value))
            } else {
                Err("value is not odd")
            }
        }

        pub fn value(&self) -> u64 {
            self.0
        }
    }

    impl TryFrom<u64> for AlwaysOdd {
        type Error = &'static str;

        fn try_from(value: u64) -> Result<Self, Self::Error> {
            Self::new(value)
        }
    }

    #[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
    pub(crate) struct AlwaysEven(u64);

    impl AlwaysEven {
        pub fn new(value: u64) -> Result<Self, &'static str> {
            if value % 2 == 0 {
                Ok(Self(value))
            } else {
                Err("value is not even")
            }
        }

        pub fn value(&self) -> u64 {
            self.0
        }
    }

    impl TryFrom<u64> for AlwaysEven {
        type Error = &'static str;

        fn try_from(value: u64) -> Result<Self, Self::Error> {
            Self::new(value)
        }
    }
}

mod implement_encoding_for_those_structs {
    use crate::crate_defined_structs::{AlwaysEven, AlwaysOdd};
    use crate::Tag;
    use bilrost::encoding::{DistinguishedProxiable, EmptyState, ForOverwrite, Proxiable};
    use bilrost::Canonicity::Canonical;
    use bilrost::{Canonicity, DecodeErrorKind};

    impl Proxiable for AlwaysEven {
        type Proxy = u64;

        fn encode_proxy(&self) -> Self::Proxy {
            self.value()
        }

        fn decode_proxy(&mut self, proxy: Self::Proxy) -> Result<(), DecodeErrorKind> {
            *self = Self::new(proxy).map_err(|_| DecodeErrorKind::InvalidValue)?;
            Ok(())
        }
    }

    impl DistinguishedProxiable for AlwaysEven {
        fn decode_proxy_distinguished(
            &mut self,
            proxy: Self::Proxy,
        ) -> Result<Canonicity, DecodeErrorKind> {
            self.decode_proxy(proxy)?;
            Ok(Canonical)
        }
    }

    // By implementing these traits for `AlwaysOdd` with a sealed tag struct, we can prevent other
    // crates from accessing different representations by using the `Proxied` encoding since they
    // will not be able to name the private tag.
    impl Proxiable<Tag> for AlwaysOdd {
        type Proxy = u64;

        fn encode_proxy(&self) -> Self::Proxy {
            self.value()
        }

        fn decode_proxy(&mut self, proxy: Self::Proxy) -> Result<(), DecodeErrorKind> {
            *self = Self::new(proxy).map_err(|_| DecodeErrorKind::InvalidValue)?;
            Ok(())
        }
    }

    // Reminder: implementing the "distinguished" traits is not required. If distinguished decoding
    // isn't needed, it doesn't do anything.
    impl DistinguishedProxiable<Tag> for AlwaysOdd {
        fn decode_proxy_distinguished(
            &mut self,
            proxy: Self::Proxy,
        ) -> Result<Canonicity, DecodeErrorKind> {
            self.decode_proxy(proxy)?;
            Ok(Canonical)
        }
    }

    // We implement the "base empty state" traits for our own types that we are encoding. Currently
    // when we implement them like this with `()` in the first parameter, they apply as the empty
    // state implementation for almost every encoding inside the `bilrost` crate.

    impl ForOverwrite<(), AlwaysEven> for () {
        fn for_overwrite() -> AlwaysEven {
            AlwaysEven::new(0).unwrap()
        }
    }

    impl EmptyState<(), AlwaysEven> for () {
        fn is_empty(val: &AlwaysEven) -> bool {
            val.value() == 0
        }

        fn clear(val: &mut AlwaysEven) {
            *val = AlwaysEven::new(0).unwrap();
        }
    }

    impl ForOverwrite<(), AlwaysOdd> for () {
        fn for_overwrite() -> AlwaysOdd {
            AlwaysOdd::new(1).unwrap()
        }
    }

    // Since we are representing `AlwaysOdd` as its plain integer value, it won't have an "empty"
    // state; we're just pretending it doesn't have a sane default for the sake of example. As a
    // result we just won't implement `EmptyState` for this type. To include it in a message, it
    // will always need to be wrapped in another type like an `Option`, a `Vec`, or a variant of a
    // oneof.
    //
    // Again, this isn't a hard requirement. If we pick a sane default for the type it's totally
    // possible for us to just have that be its "empty" state, even if it isn't represented the same
    // as the "empty" value of the integer it is written as. This just means we take on an
    // additional burden of making sure that this "empty" default never changes, because if it does
    // it could change the meaning of a lot of our encoded and transmitted data.

    // ---

    // And now, because we own the `AlwaysEven` and `AlwaysOdd` types, we are able to delegate their
    // encoding directly to the "general" encodings in the bilrost crate if we want to:
    bilrost::delegate_proxied_encoding!(
        use encoding (bilrost::encoding::Varint)
        to encode proxied type (AlwaysEven)
        with general encodings
        including distinguished
        including schema
    );
    bilrost::delegate_proxied_encoding!(
        use encoding (bilrost::encoding::Varint)
        to encode proxied type (AlwaysOdd) using proxy tag (Tag)
        with general encodings
        including distinguished
        including schema
    );

    // We can also delegate these to our own encoding, perhaps with a different default meaning.
    bilrost::delegate_proxied_encoding!(
        use encoding(bilrost::encoding::Fixed)
        to encode proxied type (AlwaysEven)
        with encoding (super::CustomEncoding)
        including distinguished
        including schema
    );
    bilrost::delegate_proxied_encoding!(
        use encoding(bilrost::encoding::Fixed)
        to encode proxied type (AlwaysOdd) using proxy tag (Tag)
        with encoding (super::CustomEncoding)
        including distinguished
        including schema
    );
}

fn main() {
    use crate_defined_structs::{AlwaysEven, AlwaysOdd};

    #[derive(Debug, PartialEq, Message)]
    struct MessageWithCustomTypes {
        plain: AlwaysEven,
        repeated: Vec<AlwaysEven>,
        values: BTreeMap<String, AlwaysEven>,
        // `AlwaysOdd` can't be a plain field because it has no "empty" value
        must_be_wrapped: Option<AlwaysOdd>,
        #[bilrost(encoding(CustomEncoding))]
        encoded_customly: Option<AlwaysOdd>,
    }

    #[derive(Message)]
    struct EncodesSingle<T>(T);
    #[derive(Message)]
    struct EncodesPacked<T>(#[bilrost(encoding(packed))] T);

    // `AlwaysOdd` doesn't have an empty value. That means that it can't be a message value that
    // is expected to always be written, whether it is a bare field or an array that has a fixed
    // number of values that are always present.
    static_assertions::assert_not_impl_any!(EncodesSingle<AlwaysOdd>: Message);
    static_assertions::assert_not_impl_any!(EncodesPacked<[AlwaysOdd; 5]>: Message);
    // However if we nest either one of those things in another type, like an `Option` or a
    // collection or mapping type, that restriction goes away because now the field that contains
    // our `AlwaysOdd` value can be empty.
    static_assertions::assert_impl_all!(EncodesSingle<Option<AlwaysOdd>>: Message);
    static_assertions::assert_impl_all!(EncodesPacked<Option<[AlwaysOdd; 5]>>: Message);
    static_assertions::assert_impl_all!(EncodesSingle<Vec<AlwaysOdd>>: Message);
    static_assertions::assert_impl_all!(EncodesSingle<BTreeMap<String, AlwaysOdd>>: Message);

    let msg = MessageWithCustomTypes {
        plain: 2.try_into().unwrap(),
        repeated: [10, 12, 16, 22]
            .into_iter()
            .map(|i| i.try_into().unwrap())
            .collect(),
        values: BTreeMap::from_iter([
            ("hundred".to_owned(), 100.try_into().unwrap()),
            ("thousand".to_owned(), 1000.try_into().unwrap()),
            ("six".to_owned(), 6.try_into().unwrap()),
        ]),
        must_be_wrapped: None,
        encoded_customly: Some(13.try_into().unwrap()),
    };
    let encoded = msg.encode_to_vec();
    println!("encoded bytes: {encoded:02x?}");
    let round_tripped = MessageWithCustomTypes::decode(encoded.as_slice());
    println!("decoded: {round_tripped:#?}");
    assert_eq!(round_tripped.as_ref(), Ok(&msg));

    // Let's create a message with invalid data, like an odd number 1 in our first field. We defined
    // the proxy implementations to return the right error code, `InvalidValue`, when the value
    // can't be converted to our type:
    use bilrost::encoding::opaque::{OpaqueMessage, OpaqueValue};
    let message_with_invalid_data =
        OpaqueMessage::from_iter([(1, OpaqueValue::u64(1))]).encode_to_vec();
    let decode_error = MessageWithCustomTypes::decode(message_with_invalid_data.as_slice())
        .expect_err("invalid message should not decode without error");
    assert_eq!(decode_error.kind(), bilrost::DecodeErrorKind::InvalidValue);
    println!("got the expected invalid value error -- {decode_error}");
}
