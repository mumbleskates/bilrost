use crate::buf::ReverseBuf;
use crate::encoding::{
    Canonicity, Capped, DecodeContext, EmptyState, RestrictedDecodeContext, TagReader, WireType,
};
use crate::Canonicity::Canonical;
use crate::DecodeError;
use alloc::boxed::Box;
use bytes::{Buf, BufMut};

/// Merges fields from the given buffer, to its cap, into the given owned message value.
/// Implemented as a private standalone method to discourage "merging" as a usage pattern.
#[inline]
pub(crate) fn merge<T: RawMessageDecoder, B: Buf + ?Sized>(
    value: &mut T,
    mut buf: Capped<B>,
    ctx: DecodeContext,
) -> Result<(), DecodeError> {
    let tr = &mut TagReader::new();
    let mut last_tag = None::<u32>;
    while buf.has_remaining()? {
        let (tag, wire_type) = tr.decode_key(buf.lend())?;
        let duplicated = last_tag == Some(tag);
        last_tag = Some(tag);
        value.raw_decode_field(tag, wire_type, duplicated, buf.lend(), ctx.clone())?;
    }
    Ok(())
}

/// Merges fields from the given buffer, to its cap, into the given distinguished owned message
/// value. Implemented as a private standalone method to discourage "merging" as a usage pattern.
#[inline]
pub(crate) fn merge_distinguished<T: RawDistinguishedMessageDecoder, B: Buf + ?Sized>(
    value: &mut T,
    mut buf: Capped<B>,
    ctx: RestrictedDecodeContext,
) -> Result<Canonicity, DecodeError> {
    let tr = &mut TagReader::new();
    let mut last_tag = None::<u32>;
    let mut canon = Canonical;
    while buf.has_remaining()? {
        let (tag, wire_type) = tr.decode_key(buf.lend())?;
        let duplicated = last_tag == Some(tag);
        last_tag = Some(tag);
        canon.update(value.raw_decode_field_distinguished(
            tag,
            wire_type,
            duplicated,
            buf.lend(),
            ctx.clone(),
        )?);
    }
    Ok(canon)
}

/// Merges fields from the given buffer, to its cap, into the given borrowed message value.
/// Implemented as a private standalone method to discourage "merging" as a usage pattern.
#[inline]
pub(crate) fn borrow_merge<'a, T: RawMessageBorrowDecoder<'a>>(
    value: &mut T,
    mut buf: Capped<&'a [u8]>,
    ctx: DecodeContext,
) -> Result<(), DecodeError> {
    let tr = &mut TagReader::new();
    let mut last_tag = None::<u32>;
    while buf.has_remaining()? {
        let (tag, wire_type) = tr.decode_key(buf.lend())?;
        let duplicated = last_tag == Some(tag);
        last_tag = Some(tag);
        value.raw_borrow_decode_field(tag, wire_type, duplicated, buf.lend(), ctx.clone())?;
    }
    Ok(())
}

/// Merges fields from the given buffer, to its cap, into the given distinguished borrowed message
/// value. Implemented as a private standalone method to discourage "merging" as a usage pattern.
#[inline]
pub(crate) fn borrow_merge_distinguished<'a, T: RawDistinguishedMessageBorrowDecoder<'a>>(
    value: &mut T,
    mut buf: Capped<&'a [u8]>,
    ctx: RestrictedDecodeContext,
) -> Result<Canonicity, DecodeError> {
    let tr = &mut TagReader::new();
    let mut last_tag = None::<u32>;
    let mut canon = Canonical;
    while buf.has_remaining()? {
        let (tag, wire_type) = tr.decode_key(buf.lend())?;
        let duplicated = last_tag == Some(tag);
        last_tag = Some(tag);
        canon.update(value.raw_borrow_decode_field_distinguished(
            tag,
            wire_type,
            duplicated,
            buf.lend(),
            ctx.clone(),
        )?);
    }
    Ok(canon)
}

/// Trait to be implemented by messages, which have knowledge of their fields' tags and encoding.
/// The methods of this trait are meant to only be used by the `Message` implementation.
pub trait RawMessage: EmptyState {
    const __ASSERTIONS: ();

    /// Encodes the message to a buffer.
    ///
    /// This method will panic if the buffer has insufficient capacity.
    fn raw_encode<B: BufMut + ?Sized>(&self, buf: &mut B);

    /// Prepends the message to a prepend buffer.
    fn raw_prepend<B: ReverseBuf + ?Sized>(&self, buf: &mut B);

    /// Returns the encoded length of the message without a length delimiter.
    fn raw_encoded_len(&self) -> usize;
}

pub trait RawMessageDecoder: RawMessage {
    /// Decodes a field from a buffer into `self`.
    fn raw_decode_field<B: Buf + ?Sized>(
        &mut self,
        tag: u32,
        wire_type: WireType,
        duplicated: bool,
        buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError>
    where
        Self: Sized;
}

/// Complementary underlying trait for distinguished messages, all of whose fields have a
/// distinguished encoding.
pub trait RawDistinguishedMessageDecoder: RawMessage + Eq {
    fn raw_decode_field_distinguished<B: Buf + ?Sized>(
        &mut self,
        tag: u32,
        wire_type: WireType,
        duplicated: bool,
        buf: Capped<B>,
        ctx: RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError>
    where
        Self: Sized;
}

pub trait RawMessageBorrowDecoder<'a>: RawMessage {
    /// Decodes a field from a buffer into `self` from a borrowed slice.
    fn raw_borrow_decode_field(
        &mut self,
        tag: u32,
        wire_type: WireType,
        duplicated: bool,
        buf: Capped<&'a [u8]>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError>
    where
        Self: Sized;
}

pub trait RawDistinguishedMessageBorrowDecoder<'a>: RawMessage + Eq {
    fn raw_borrow_decode_field_distinguished(
        &mut self,
        tag: u32,
        wire_type: WireType,
        duplicated: bool,
        buf: Capped<&'a [u8]>,
        ctx: RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError>
    where
        Self: Sized;
}

impl<T> RawMessage for Box<T>
where
    T: RawMessage,
{
    const __ASSERTIONS: () = ();

    fn raw_encode<B: BufMut + ?Sized>(&self, buf: &mut B) {
        (**self).raw_encode(buf)
    }

    fn raw_prepend<B: ReverseBuf + ?Sized>(&self, buf: &mut B) {
        (**self).raw_prepend(buf)
    }

    fn raw_encoded_len(&self) -> usize {
        (**self).raw_encoded_len()
    }
}

impl<T> RawMessageDecoder for Box<T>
where
    T: RawMessageDecoder,
{
    fn raw_decode_field<B: Buf + ?Sized>(
        &mut self,
        tag: u32,
        wire_type: WireType,
        duplicated: bool,
        buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError>
    where
        Self: Sized,
    {
        (**self).raw_decode_field(tag, wire_type, duplicated, buf, ctx)
    }
}

impl<'a, T> RawMessageBorrowDecoder<'a> for Box<T>
where
    T: RawMessageBorrowDecoder<'a>,
{
    fn raw_borrow_decode_field(
        &mut self,
        tag: u32,
        wire_type: WireType,
        duplicated: bool,
        buf: Capped<&'a [u8]>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError>
    where
        Self: Sized,
    {
        (**self).raw_borrow_decode_field(tag, wire_type, duplicated, buf, ctx)
    }
}

impl<T> RawDistinguishedMessageDecoder for Box<T>
where
    T: RawDistinguishedMessageDecoder,
{
    fn raw_decode_field_distinguished<B: Buf + ?Sized>(
        &mut self,
        tag: u32,
        wire_type: WireType,
        duplicated: bool,
        buf: Capped<B>,
        ctx: RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError>
    where
        Self: Sized,
    {
        (**self).raw_decode_field_distinguished(tag, wire_type, duplicated, buf, ctx)
    }
}

impl<'a, T> RawDistinguishedMessageBorrowDecoder<'a> for Box<T>
where
    T: RawDistinguishedMessageBorrowDecoder<'a>,
{
    fn raw_borrow_decode_field_distinguished(
        &mut self,
        tag: u32,
        wire_type: WireType,
        duplicated: bool,
        buf: Capped<&'a [u8]>,
        ctx: RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError>
    where
        Self: Sized,
    {
        (**self).raw_borrow_decode_field_distinguished(tag, wire_type, duplicated, buf, ctx)
    }
}
