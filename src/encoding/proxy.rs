use crate::buf::ReverseBuf;
use crate::encoding::{
    Capped, DecodeContext, DistinguishedValueBorrowDecoder, DistinguishedValueDecoder,
    RestrictedDecodeContext, ValueBorrowDecoder, ValueDecoder, ValueEncoder, WireType, Wiretyped,
};
use crate::{Canonicity, DecodeError, DecodeErrorKind};
use bytes::{Buf, BufMut};
use core::ops::Deref;

/// Proxied is a special encoder which translates the encoded type into its "proxy" type first,
/// simplifying the encoding logic.
pub struct Proxied<E, Tag = ()>(E, Tag);

/// Tag struct used for sealing proxy implementations to our own crate specifically. Other crates
/// may do the same in order to keep their proxy implementations from leaking.
pub(crate) struct SealedBilrostTag;

pub trait Proxiable<Tag = ()> {
    type Proxy;

    fn new_proxy() -> Self::Proxy;

    fn encode_proxy(&self) -> Self::Proxy;

    fn decode_proxy(&mut self, proxy: Self::Proxy) -> Result<(), DecodeErrorKind>;
}

pub trait DistinguishedProxiable<Tag = ()>: Proxiable<Tag> {
    fn decode_proxy_distinguished(
        &mut self,
        proxy: Self::Proxy,
    ) -> Result<Canonicity, DecodeErrorKind>;
}

impl<T, E, Tag> Wiretyped<Proxied<E, Tag>> for T
where
    T: Proxiable<Tag>,
    T::Proxy: Wiretyped<E>,
{
    const WIRE_TYPE: WireType = T::Proxy::WIRE_TYPE;
}

impl<T, E, Tag> ValueEncoder<Proxied<E, Tag>> for T
where
    T: Proxiable<Tag>,
    T::Proxy: ValueEncoder<E>,
{
    #[inline]
    fn encode_value<B: BufMut + ?Sized>(value: &Self, buf: &mut B) {
        ValueEncoder::<E>::encode_value(&value.encode_proxy(), buf);
    }

    #[inline]
    fn prepend_value<B: ReverseBuf + ?Sized>(value: &Self, buf: &mut B) {
        ValueEncoder::<E>::prepend_value(&value.encode_proxy(), buf);
    }

    #[inline]
    fn value_encoded_len(value: &Self) -> usize {
        ValueEncoder::<E>::value_encoded_len(&value.encode_proxy())
    }

    #[inline]
    fn many_values_encoded_len<I>(values: I) -> usize
    where
        I: ExactSizeIterator,
        I::Item: Deref<Target = Self>,
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

        ValueEncoder::<E>::many_values_encoded_len(
            values.map(|item| WrapDeref(item.encode_proxy())),
        )
    }
}

impl<T, E, Tag> ValueDecoder<Proxied<E, Tag>> for T
where
    T: Proxiable<Tag>,
    T::Proxy: ValueDecoder<E>,
{
    #[inline]
    fn decode_value<B: Buf + ?Sized>(
        value: &mut Self,
        buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        let mut proxy = T::new_proxy();
        ValueDecoder::<E>::decode_value(&mut proxy, buf, ctx)?;
        Ok(value.decode_proxy(proxy)?)
    }
}

impl<T, E, Tag> DistinguishedValueDecoder<Proxied<E, Tag>> for T
where
    T: DistinguishedProxiable<Tag> + Eq,
    T::Proxy: DistinguishedValueDecoder<E>,
{
    const CHECKS_EMPTY: bool = T::Proxy::CHECKS_EMPTY;

    fn decode_value_distinguished<const ALLOW_EMPTY: bool>(
        value: &mut Self,
        buf: Capped<impl Buf + ?Sized>,
        ctx: RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError> {
        let mut proxy = T::new_proxy();
        let mut canon = DistinguishedValueDecoder::<E>::decode_value_distinguished::<ALLOW_EMPTY>(
            &mut proxy, buf, ctx,
        )?;
        canon.update(value.decode_proxy_distinguished(proxy)?);
        Ok(canon)
    }
}

impl<'a, T, E, Tag> ValueBorrowDecoder<'a, Proxied<E, Tag>> for T
where
    T: Proxiable<Tag>,
    T::Proxy: ValueBorrowDecoder<'a, E>,
{
    #[inline]
    fn borrow_decode_value(
        value: &mut Self,
        buf: Capped<&'a [u8]>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        let mut proxy = T::new_proxy();
        ValueBorrowDecoder::<E>::borrow_decode_value(&mut proxy, buf, ctx)?;
        Ok(value.decode_proxy(proxy)?)
    }
}

impl<'a, T, E, Tag> DistinguishedValueBorrowDecoder<'a, Proxied<E, Tag>> for T
where
    T: DistinguishedProxiable<Tag> + Eq,
    T::Proxy: DistinguishedValueBorrowDecoder<'a, E>,
{
    const CHECKS_EMPTY: bool = T::Proxy::CHECKS_EMPTY;

    fn borrow_decode_value_distinguished<const ALLOW_EMPTY: bool>(
        value: &mut Self,
        buf: Capped<&'a [u8]>,
        ctx: RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError> {
        let mut proxy = T::new_proxy();
        let mut canon = DistinguishedValueBorrowDecoder::<E>::borrow_decode_value_distinguished::<
            ALLOW_EMPTY,
        >(&mut proxy, buf, ctx)?;
        canon.update(value.decode_proxy_distinguished(proxy)?);
        Ok(canon)
    }
}
