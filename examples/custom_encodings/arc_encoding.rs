use bilrost::buf::ReverseBuf;
use bilrost::bytes::{Buf, BufMut};
use bilrost::encoding::{
    BorrowDecoder, Capped, DecodeContext, Decoder, DistinguishedBorrowDecoder,
    DistinguishedDecoder, DistinguishedValueBorrowDecoder, DistinguishedValueDecoder, EmptyState,
    Encoder, ForOverwrite, RestrictedDecodeContext, TagMeasurer, TagRevWriter, TagWriter,
    ValueBorrowDecoder, ValueDecoder, ValueEncoder, WireType, Wiretyped,
};
use bilrost::{Canonicity, DecodeError};
use std::sync::Arc;

pub(crate) struct ArcEncoding<E>(E);

// This enables `Option<Arc<T>>` and [Arc<T>; N]
bilrost::implement_core_empty_state_rules!(ArcEncoding<E>, with generics (E));

// The rest of the file is trait implementations that perform direct method pass-through.

impl<T, E> ForOverwrite<ArcEncoding<E>> for Arc<T>
where
    T: ForOverwrite<E>,
{
    #[inline(always)]
    fn for_overwrite() -> Self
    where
        Self: Sized,
    {
        Arc::new(T::for_overwrite())
    }
}

impl<T, E> EmptyState<ArcEncoding<E>> for Arc<T>
where
    T: Clone + EmptyState<E>,
{
    #[inline(always)]
    fn empty() -> Self
    where
        Self: Sized,
    {
        Arc::new(T::empty())
    }

    #[inline(always)]
    fn is_empty(&self) -> bool {
        (**self).is_empty()
    }

    #[inline(always)]
    fn clear(&mut self) {
        Arc::make_mut(self).clear()
    }
}

impl<T, E> Wiretyped<ArcEncoding<E>> for Arc<T>
where
    T: Wiretyped<E>,
{
    const WIRE_TYPE: WireType = <T as Wiretyped<E>>::WIRE_TYPE;
}

impl<T, E> ValueEncoder<ArcEncoding<E>> for Arc<T>
where
    T: ValueEncoder<E>,
{
    #[inline(always)]
    fn encode_value<B: BufMut + ?Sized>(value: &Self, buf: &mut B) {
        <T as ValueEncoder<E>>::encode_value(&**value, buf)
    }

    #[inline(always)]
    fn prepend_value<B: ReverseBuf + ?Sized>(value: &Self, buf: &mut B) {
        <T as ValueEncoder<E>>::prepend_value(&**value, buf)
    }

    #[inline(always)]
    fn value_encoded_len(value: &Self) -> usize {
        <T as ValueEncoder<E>>::value_encoded_len(&**value)
    }
}

impl<T, E> ValueDecoder<ArcEncoding<E>> for Arc<T>
where
    T: Clone + ValueDecoder<E>,
{
    #[inline(always)]
    fn decode_value<B: Buf + ?Sized>(
        value: &mut Self,
        buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        <T as ValueDecoder<E>>::decode_value(Arc::make_mut(value), buf, ctx)
    }
}

impl<T, E> DistinguishedValueDecoder<ArcEncoding<E>> for Arc<T>
where
    T: Clone + DistinguishedValueDecoder<E>,
{
    const CHECKS_EMPTY: bool = <T as DistinguishedValueDecoder<E>>::CHECKS_EMPTY;

    #[inline(always)]
    fn decode_value_distinguished<const ALLOW_EMPTY: bool>(
        value: &mut Self,
        buf: Capped<impl Buf + ?Sized>,
        ctx: RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError> {
        <T as DistinguishedValueDecoder<E>>::decode_value_distinguished::<ALLOW_EMPTY>(
            Arc::make_mut(value),
            buf,
            ctx,
        )
    }
}

impl<'a, T, E> ValueBorrowDecoder<'a, ArcEncoding<E>> for Arc<T>
where
    T: Clone + ValueBorrowDecoder<'a, E>,
{
    #[inline(always)]
    fn borrow_decode_value(
        value: &mut Self,
        buf: Capped<&'a [u8]>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        <T as ValueBorrowDecoder<E>>::borrow_decode_value(Arc::make_mut(value), buf, ctx)
    }
}

impl<'a, T, E> DistinguishedValueBorrowDecoder<'a, ArcEncoding<E>> for Arc<T>
where
    T: Clone + DistinguishedValueBorrowDecoder<'a, E>,
{
    const CHECKS_EMPTY: bool = <T as DistinguishedValueBorrowDecoder<'a, E>>::CHECKS_EMPTY;

    #[inline(always)]
    fn borrow_decode_value_distinguished<const ALLOW_EMPTY: bool>(
        value: &mut Self,
        buf: Capped<&'a [u8]>,
        ctx: RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError> {
        <T as DistinguishedValueBorrowDecoder<E>>::borrow_decode_value_distinguished::<ALLOW_EMPTY>(
            Arc::make_mut(value),
            buf,
            ctx,
        )
    }
}

impl<T, E> Encoder<ArcEncoding<E>> for Arc<T>
where
    T: Encoder<E>,
{
    #[inline(always)]
    fn encode<B: BufMut + ?Sized>(tag: u32, value: &Self, buf: &mut B, tw: &mut TagWriter) {
        <T as Encoder<E>>::encode(tag, value, buf, tw)
    }

    #[inline(always)]
    fn prepend_encode<B: ReverseBuf + ?Sized>(
        tag: u32,
        value: &Self,
        buf: &mut B,
        tw: &mut TagRevWriter,
    ) {
        <T as Encoder<E>>::prepend_encode(tag, value, buf, tw)
    }

    #[inline(always)]
    fn encoded_len(tag: u32, value: &Self, tm: &mut impl TagMeasurer) -> usize {
        <T as Encoder<E>>::encoded_len(tag, value, tm)
    }
}

impl<T, E> Decoder<ArcEncoding<E>> for Arc<T>
where
    T: Clone + Decoder<E>,
{
    #[inline(always)]
    fn decode<B: Buf + ?Sized>(
        wire_type: WireType,
        value: &mut Self,
        buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        <T as Decoder<E>>::decode(wire_type, Arc::make_mut(value), buf, ctx)
    }
}

impl<T, E> DistinguishedDecoder<ArcEncoding<E>> for Arc<T>
where
    T: Clone + DistinguishedDecoder<E>,
{
    #[inline(always)]
    fn decode_distinguished<B: Buf + ?Sized>(
        wire_type: WireType,
        value: &mut Self,
        buf: Capped<B>,
        ctx: RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError> {
        <T as DistinguishedDecoder<E>>::decode_distinguished(
            wire_type,
            Arc::make_mut(value),
            buf,
            ctx,
        )
    }
}

impl<'a, T, E> BorrowDecoder<'a, ArcEncoding<E>> for Arc<T>
where
    T: Clone + BorrowDecoder<'a, E>,
{
    #[inline(always)]
    fn borrow_decode(
        wire_type: WireType,
        value: &mut Self,
        buf: Capped<&'a [u8]>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        <T as BorrowDecoder<'a, E>>::borrow_decode(wire_type, Arc::make_mut(value), buf, ctx)
    }
}

impl<'a, T, E> DistinguishedBorrowDecoder<'a, ArcEncoding<E>> for Arc<T>
where
    T: Clone + DistinguishedBorrowDecoder<'a, E>,
{
    #[inline(always)]
    fn borrow_decode_distinguished(
        wire_type: WireType,
        value: &mut Self,
        buf: Capped<&'a [u8]>,
        ctx: RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError> {
        <T as DistinguishedBorrowDecoder<E>>::borrow_decode_distinguished(
            wire_type,
            Arc::make_mut(value),
            buf,
            ctx,
        )
    }
}
