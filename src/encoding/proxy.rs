use crate::buf::ReverseBuf;
use crate::encoding::{
    Capped, DecodeContext, DistinguishedValueBorrowDecoder, DistinguishedValueDecoder,
    RestrictedDecodeContext, ValueBorrowDecoder, ValueDecoder, ValueEncoder, WireType, Wiretyped,
};
use crate::{Canonicity, DecodeError, DecodeErrorKind};
use bytes::{Buf, BufMut};
use core::ops::Deref;

/// `Proxied` is a special encoder which translates the encoded type into its "proxy" type first,
/// simplifying the encoding logic. It provides value-encoding implementations for types that
/// implement the proxied conversions defined in `Proxiable` and `DistinguishedProxiable`, which can
/// then be delegated (such as with `delegate_value_encoding!`) to another encoding to be used as
/// field types in messages.
///
/// `Proxied` itself cannot be used to encode message fields because its
/// trait support means that it cannot ever implement `Encoder`, `Decoder` etc. for its supported,
/// un-wrapped types; only when they are nested in `Option<T>`, or appear in a oneof, or in some
/// other container.
pub struct Proxied<E, Tag = ()>(E, Tag);

/// Tag struct used for sealing proxy implementations to our own crate specifically. Other crates
/// may do the same in order to keep their proxy implementations from leaking.
pub(crate) struct SealedBilrostTag;

/// Implement this trait to make a type supported for encoding by proxy; this can be thought of like
/// a special pair of `Into` and `TryFrom` implementations that are dedicated to this proxy
/// specification and are used every time this type is encoded or decoded, since that is essentially
/// exactly what happens.
pub trait Proxiable<Tag = ()> {
    /// The type that the value should appear as when it is encoded on the wire.
    type Proxy;

    /// Return a fresh proxy value. This should just be a cheap default, its value needn't be
    /// significant.
    // TODO(widders): eliminate new_proxy, use ForOverwrite<E> and EmptyState<E> for the proxy type
    //  instead
    fn new_proxy() -> Self::Proxy;

    /// Convert this value into a value of the proxy's type.
    fn encode_proxy(&self) -> Self::Proxy;

    /// Try to store the value represented in the proxy's type in this value, returning an error if
    /// the value is not valid (ideally the error should be `OutOfDomainValue`, `InvalidValue`, or
    /// `Other`).
    fn decode_proxy(&mut self, proxy: Self::Proxy) -> Result<(), DecodeErrorKind>;
}

/// Extension trait for distinguished types that can decode by proxy.
pub trait DistinguishedProxiable<Tag = ()>: Proxiable<Tag> {
    /// Try to store the value represented in the proxy's type in this value, returning an error if
    /// the value is not valid (ideally the error should be `OutOfDomainValue`, `InvalidValue`, or
    /// `Other`). On success, return whether the value was a canonical representation (the same
    /// proxy value that would have been created for this value, `Canonical`) or not
    /// (`NotCanonical`).
    fn decode_proxy_distinguished(
        &mut self,
        proxy: Self::Proxy,
    ) -> Result<Canonicity, DecodeErrorKind>;
}

// TODO(widders): consider implementing proxied empty states in terms of the proxy's empty state
//  instead, from here

impl<T, E, Tag> Wiretyped<Proxied<E, Tag>, T> for ()
where
    T: Proxiable<Tag>,
    (): Wiretyped<E, T::Proxy>,
{
    const WIRE_TYPE: WireType = <() as Wiretyped<E, T::Proxy>>::WIRE_TYPE;
}

impl<T, E, Tag> ValueEncoder<Proxied<E, Tag>, T> for ()
where
    T: Proxiable<Tag>,
    (): ValueEncoder<E, T::Proxy>,
{
    #[inline]
    fn encode_value<B: BufMut + ?Sized>(value: &T, buf: &mut B) {
        ValueEncoder::<E, _>::encode_value(&value.encode_proxy(), buf);
    }

    #[inline]
    fn prepend_value<B: ReverseBuf + ?Sized>(value: &T, buf: &mut B) {
        ValueEncoder::<E, _>::prepend_value(&value.encode_proxy(), buf);
    }

    #[inline]
    fn value_encoded_len(value: &T) -> usize {
        ValueEncoder::<E, _>::value_encoded_len(&value.encode_proxy())
    }

    #[inline]
    fn many_values_encoded_len<I>(values: I) -> usize
    where
        I: ExactSizeIterator,
        I::Item: Deref<Target = T>,
    {
        /// Do-nothing wrapper allowing us to return items by-value and still have them Deref to T. Maybe
        /// it would be "more correct" to use Borrow or something like that but this is pretty easy too.
        #[repr(transparent)]
        struct WrapDeref<T>(T);

        impl<T> Deref for WrapDeref<T> {
            type Target = T;

            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }

        ValueEncoder::<E, _>::many_values_encoded_len(
            values.map(|item| WrapDeref(item.encode_proxy())),
        )
    }
}

impl<T, E, Tag> ValueDecoder<Proxied<E, Tag>, T> for ()
where
    T: Proxiable<Tag>,
    (): ValueDecoder<E, T::Proxy>,
{
    #[inline]
    fn decode_value<B: Buf + ?Sized>(
        value: &mut T,
        buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        let mut proxy = T::new_proxy();
        ValueDecoder::<E, _>::decode_value(&mut proxy, buf, ctx)?;
        Ok(value.decode_proxy(proxy)?)
    }
}

impl<T, E, Tag> DistinguishedValueDecoder<Proxied<E, Tag>, T> for ()
where
    T: DistinguishedProxiable<Tag> + Eq,
    (): DistinguishedValueDecoder<E, T::Proxy>,
{
    const CHECKS_EMPTY: bool = <() as DistinguishedValueDecoder<E, T::Proxy>>::CHECKS_EMPTY;

    fn decode_value_distinguished<const ALLOW_EMPTY: bool>(
        value: &mut T,
        buf: Capped<impl Buf + ?Sized>,
        ctx: RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError> {
        let mut proxy = T::new_proxy();
        let mut canon = DistinguishedValueDecoder::<E, _>::decode_value_distinguished::<ALLOW_EMPTY>(
            &mut proxy,
            buf,
            ctx.clone(),
        )?;
        canon.update(ctx.check(value.decode_proxy_distinguished(proxy)?)?);
        Ok(canon)
    }
}

impl<'a, T, E, Tag> ValueBorrowDecoder<'a, Proxied<E, Tag>, T> for ()
where
    T: Proxiable<Tag>,
    (): ValueBorrowDecoder<'a, E, T::Proxy>,
{
    #[inline]
    fn borrow_decode_value(
        value: &mut T,
        buf: Capped<&'a [u8]>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        let mut proxy = T::new_proxy();
        ValueBorrowDecoder::<E, _>::borrow_decode_value(&mut proxy, buf, ctx)?;
        Ok(value.decode_proxy(proxy)?)
    }
}

impl<'a, T, E, Tag> DistinguishedValueBorrowDecoder<'a, Proxied<E, Tag>, T> for ()
where
    T: DistinguishedProxiable<Tag> + Eq,
    (): DistinguishedValueBorrowDecoder<'a, E, T::Proxy>,
{
    const CHECKS_EMPTY: bool = <() as DistinguishedValueBorrowDecoder<E, T::Proxy>>::CHECKS_EMPTY;

    fn borrow_decode_value_distinguished<const ALLOW_EMPTY: bool>(
        value: &mut T,
        buf: Capped<&'a [u8]>,
        ctx: RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError> {
        let mut proxy = T::new_proxy();
        let mut canon = DistinguishedValueBorrowDecoder::<E, _>::borrow_decode_value_distinguished::<
            ALLOW_EMPTY,
        >(&mut proxy, buf, ctx.clone())?;
        canon.update(ctx.check(value.decode_proxy_distinguished(proxy)?)?);
        Ok(canon)
    }
}
