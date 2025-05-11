struct CustomEncoding;
struct Tag;

bilrost::implement_core_empty_state_rules!(CustomEncoding);
// TODO(widders): neither of these are possible, it turns out, because implementing
//  `bilrost::Trait<crate::Something> for T` is disallowed: T is a type parameter, is uncovered, and
//  appears before the first local type (the type implemented "for" is ordered first). the traits
//  must be rejiggered to make this possible
bilrost::encoding_implemented_via_value_encoding!(CustomEncoding);
// bilrost::encoding_uses_base_empty_state!(CustomEncoding);

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
    use bilrost::encoding::{DistinguishedProxiable, EmptyState, ForOverwrite, Proxiable, Proxied};
    use bilrost::Canonicity::Canonical;
    use bilrost::{Canonicity, DecodeErrorKind};


    impl Proxiable for AlwaysEven {
        type Proxy = u64;

        fn new_proxy() -> Self::Proxy {
            0
        }

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

        fn new_proxy() -> Self::Proxy {
            0
        }

        fn encode_proxy(&self) -> Self::Proxy {
            self.value()
        }

        fn decode_proxy(&mut self, proxy: Self::Proxy) -> Result<(), DecodeErrorKind> {
            *self = Self::new(proxy).map_err(|_| DecodeErrorKind::InvalidValue)?;
            Ok(())
        }
    }

    impl DistinguishedProxiable<Tag> for AlwaysOdd {
        fn decode_proxy_distinguished(
            &mut self,
            proxy: Self::Proxy,
        ) -> Result<Canonicity, DecodeErrorKind> {
            self.decode_proxy(proxy)?;
            Ok(Canonical)
        }
    }

    impl ForOverwrite for AlwaysEven {
        fn for_overwrite() -> Self
        where
            Self: Sized,
        {
            AlwaysEven::new(0).unwrap()
        }
    }

    impl EmptyState for AlwaysEven {
        fn is_empty(&self) -> bool {
            self.value() == 0
        }

        fn clear(&mut self) {
            *self = AlwaysEven::new(0).unwrap();
        }
    }

    impl ForOverwrite for AlwaysOdd {
        fn for_overwrite() -> Self
        where
            Self: Sized,
        {
            AlwaysOdd::new(1).unwrap()
        }
    }

    // Since we are representing `AlwaysOdd` as its plain integer value, it won't have an "empty"
    // state because the zero value isn't an odd number. As a result we just won't implement
    // `EmptyState` for this type. To include it in a message, it will always need to be wrapped in
    // another type like an `Option`, a `Vec`, or a variant of a oneof.

    // And now, because we own the `AlwaysEven` and `AlwaysOdd` types, we are able to delegate their
    // encoding directly to the "general" encodings in the bilrost crate if we want to:
    // bilrost::delegate_proxied_encoding!(
    //     use encoding (bilrost::encoding::Varint)
    //     to encode proxied type (AlwaysEven)
    //     with general encodings including distinguished
    // );
    // bilrost::delegate_proxied_encoding!(
    //     use encoding (bilrost::encoding::Varint)
    //     to encode proxied type (AlwaysOdd) using proxy tag (Tag)
    //     with general encodings including distinguished
    // );
    //
    // // We can also delegate these to our own encoding, perhaps with a different default meaning.
    // bilrost::delegate_proxied_encoding!(
    //     use encoding(bilrost::encoding::Fixed)
    //     to encode proxied type (AlwaysEven)
    //     with encoding (super::CustomEncoding) including distinguished
    // );
    // bilrost::delegate_proxied_encoding!(
    //     use encoding(bilrost::encoding::Fixed)
    //     to encode proxied type (AlwaysOdd) using proxy tag (Tag)
    //     with encoding (super::CustomEncoding) including distinguished
    // );
}

fn main() {
    // TODO(widders): this
    use crate_defined_structs::{AlwaysEven, AlwaysOdd};
}
