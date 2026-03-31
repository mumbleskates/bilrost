use bilrost::{Message, OwnedMessage};
use std::ops::RangeInclusive;

struct CustomEncoding;
struct Tag;

// When creating a custom encoding, it is almost always desirable to implement the `Encoder` traits
// exclusively in terms of when `ValueEncoder` traits are implemented. This is needed for the
// overwhelming majority of types that are encoded, including all proxy-encoded types; only
// encodings that may take different decoding paths depending on what they encounter need to act
// otherwise, and those use cases should hopefully be covered by `Packed` and `Unpacked`.
bilrost::encoding_implemented_via_value_encoding!(CustomEncoding);

// When we're implementing proxied encoding for types that our crate doesn't own, we can't use the
// "base empty state" that includes the implementation for all the other types that `bilrost`
// implements already. Delegating our "empty" traits to the common one like that would conflict with
// the special implementations we're going to have to define. Instead, we can get some really basic
// helper implementations from this "core empty state rules" macro, which provides impls for
// `Option<T>` and `[T; N]` so we don't have to.
bilrost::implement_core_empty_state_rules!(CustomEncoding);

// Now we have a `CustomEncoding` that implements encoding for basically nothing. As soon as we
// start providing implementations for value encoding by implementing directly or delegating to
// proxied encodings it will become useful.

mod implement_encoding_for_range {
    use crate::{CustomEncoding, Tag};
    use bilrost::encoding::{DistinguishedProxiable, EmptyState, ForOverwrite, Proxiable, Proxied};
    use bilrost::Canonicity::Canonical;
    use bilrost::{Canonicity, DecodeErrorKind};
    use std::ops::RangeInclusive;

    // We *have* to use a tag for these implementations. The tag type doesn't have to be a private
    // type, and it *can* be the same type as the encoding if we want, but we do have to own it
    // otherwise it's illegal for us to implement the `Proxiable` traits.
    impl<T> Proxiable<Tag> for RangeInclusive<T>
    where
        T: Clone,
        (): ForOverwrite<(), T>,
    {
        type Proxy = [T; 2];

        fn encode_proxy(&self) -> Self::Proxy {
            [self.start().clone(), self.end().clone()]
        }

        fn decode_proxy(&mut self, proxy: Self::Proxy) -> Result<(), DecodeErrorKind> {
            let [start, end] = proxy;
            *self = start..=end;
            Ok(())
        }
    }

    impl<T: Clone> DistinguishedProxiable<Tag> for RangeInclusive<T>
    where
        (): ForOverwrite<(), T>,
    {
        fn decode_proxy_distinguished(
            &mut self,
            proxy: Self::Proxy,
        ) -> Result<Canonicity, DecodeErrorKind> {
            self.decode_proxy(proxy)?;
            // Because we are just taking values directly in and out, this is always canonical.
            // Note that we still aren't doing anything in our proxy implementation to actually
            // enforce that the range ends after it starts, we're just sending and receiving it as
            // it is.
            Ok(Canonical)
        }
    }

    // Currently, we also need to implement the empty traits for the type that we want to encode
    // directly on our encoding.

    impl<T> ForOverwrite<CustomEncoding, RangeInclusive<T>> for ()
    where
        (): ForOverwrite<(), T>,
    {
        fn for_overwrite() -> RangeInclusive<T> {
            <() as ForOverwrite<(), T>>::for_overwrite()
                ..=<() as ForOverwrite<(), T>>::for_overwrite()
        }
    }

    impl<T> EmptyState<CustomEncoding, RangeInclusive<T>> for ()
    where
        T: PartialEq,
        (): EmptyState<(), T>,
    {
        fn empty() -> RangeInclusive<T> {
            <() as EmptyState<(), T>>::empty()..=<() as EmptyState<(), T>>::empty()
        }

        fn is_empty(val: &RangeInclusive<T>) -> bool {
            *val == <() as EmptyState<CustomEncoding, RangeInclusive<T>>>::empty()
        }

        fn clear(val: &mut RangeInclusive<T>) {
            *val = <() as EmptyState<CustomEncoding, RangeInclusive<T>>>::empty();
        }
    }

    bilrost::delegate_proxied_encoding!(
        use encoding (bilrost::encoding::Packed)
        to encode proxied type (RangeInclusive<T>) using proxy tag (Tag)
        with encoding (CustomEncoding) including distinguished
        with generics (T)
    );
    // TODO: use including schema mode of the above macro instead
    bilrost::delegate_schema!(
        (CustomEncoding) encodes (RangeInclusive<T>)
        like (bilrost::encoding::Packed) encodes ([T; 2])
        with generics (T)
    );
}

fn main() {
    #[derive(Debug, PartialEq, Message)]
    struct MessageContainingRange {
        #[bilrost(encoding(CustomEncoding))]
        numeric: RangeInclusive<i64>,
        #[bilrost(encoding(CustomEncoding))]
        stringy: Option<RangeInclusive<String>>,
    }

    let msg = MessageContainingRange {
        numeric: -100..=234,
        stringy: Some("aardvark".to_owned()..="after".to_owned()),
    };
    let encoded = msg.encode_to_vec();
    println!("encoded bytes: {encoded:02x?}");
    let round_tripped = MessageContainingRange::decode(encoded.as_slice());
    println!("decoded: {round_tripped:#?}");
    assert_eq!(round_tripped.as_ref(), Ok(&msg));

    #[derive(Debug, PartialEq, Message)]
    struct EquivalentMessage {
        numeric: (i64, i64),
        #[bilrost(encoding(packed))]
        stringy: [String; 2],
    }
    let equivalent = EquivalentMessage::decode(encoded.as_slice());
    println!("we can see the ranges are encoded as-if they were the proxy type: {equivalent:#?}");
    assert_eq!(
        equivalent,
        Ok(EquivalentMessage {
            numeric: (-100, 234),
            stringy: ["aardvark".to_string(), "after".to_string()],
        })
    );
}
