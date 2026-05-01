use crate::buf::ReverseBuf;
use crate::encoding::schema::{RegisterFields, Schema, ValueRepr};
use crate::encoding::{
    encode_varint, encoded_len_varint, implement_core_empty_state_rules, prepend_varint,
    Canonicity, Capped, DecodeContext, DistinguishedValueBorrowDecoder, DistinguishedValueDecoder,
    EmptyState, ForOverwrite, RestrictedDecodeContext, TagReader, ValueBorrowDecoder, ValueDecoder,
    ValueEncoder, WireType, Wiretyped,
};
use crate::Canonicity::Canonical;
use crate::DecodeError;
use alloc::boxed::Box;
use alloc::format;
use bytes::{Buf, BufMut};
use core::fmt::Display;

/// Encoding that performs the actual value-encoding of messages, to and from `RawMessage`-family
/// traits into length-delimited values on the wire. By default this is directly delegated to by
/// the general encodings.
pub struct MessageEncoding;

implement_core_empty_state_rules!(MessageEncoding);

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
    debug_assert!(
        canon >= ctx.min_canonicity,
        "a poorly behaved distinguished decoder did not check canonicity against the context and \
        convert it into an error"
    );
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
    debug_assert!(
        canon >= ctx.min_canonicity,
        "a poorly behaved distinguished decoder did not check canonicity against the context and \
        convert it into an error"
    );
    Ok(canon)
}

/// Encoding trait to be implemented by messages. The methods of this trait are meant to only be
/// used by the `Message` implementation.
pub trait RawMessage {
    const __ASSERTIONS: ();

    /// Returns an initialized message in an empty state.
    fn empty() -> Self
    where
        Self: Sized;

    /// Returns whether the message is currently empty.
    fn is_empty(&self) -> bool;

    /// Resets the message to an empty state.
    fn clear(&mut self);

    /// Encodes the message to a buffer.
    ///
    /// This method will panic if the buffer has insufficient capacity.
    fn raw_encode<B: BufMut + ?Sized>(&self, buf: &mut B);

    /// Prepends the message to a prepend buffer.
    fn raw_prepend<B: ReverseBuf + ?Sized>(&self, buf: &mut B);

    /// Returns the encoded length of the message without a length delimiter.
    fn raw_encoded_len(&self) -> usize;
}

/// Decoding trait to be implemented by messages. The methods of this trait are meant to only be
/// used by the `OwnedMessage` implementation.
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

/// Distinguished decoding trait to be implemented by messages. The methods of this trait are meant
/// to only be used by the `DistinguishedOwnedMessage` implementation.
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

/// Borrowed decoding trait to be implemented by messages. The methods of this trait are meant to
/// only be used by the `BorrowedMessage` implementation.
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

/// Borrowed distinguished decoding trait to be implemented by messages. The methods of this trait
/// are meant to only be used by the `DistinguishedBorrowedMessage` implementation.
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

impl<T> RegisterFields for Box<T>
where
    T: RawMessage + RegisterFields,
{
    fn register(schema: &Schema) {
        schema.register_message_wrapper::<Self, T>();
        T::register(schema);
    }
}

impl<T> RawMessage for Box<T>
where
    T: RawMessage,
{
    const __ASSERTIONS: () = ();

    fn empty() -> Self
    where
        Self: Sized,
    {
        Box::new(T::empty())
    }

    fn is_empty(&self) -> bool {
        self.as_ref().is_empty()
    }

    fn clear(&mut self) {
        self.as_mut().clear();
    }

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

impl<T> ForOverwrite<MessageEncoding, T> for ()
where
    T: RawMessage,
{
    fn for_overwrite() -> T
    where
        T: Sized,
    {
        T::empty()
    }
}

impl<T> EmptyState<MessageEncoding, T> for ()
where
    T: RawMessage,
{
    fn is_empty(val: &T) -> bool {
        val.is_empty()
    }

    fn clear(val: &mut T) {
        val.clear();
    }
}

impl<T> Wiretyped<MessageEncoding, T> for ()
where
    T: RawMessage,
{
    const WIRE_TYPE: WireType = WireType::LengthDelimited;
}

impl<T> ValueRepr<MessageEncoding, T> for ()
where
    T: RawMessage + RegisterFields,
{
    fn repr(schema: &Schema) -> Box<dyn Display> {
        T::register(schema);
        schema.make_lazy_repr(|schema| {
            format!(
                "delimited message {message_type}",
                message_type = schema.type_reference::<T>(),
            )
        })
    }
}

impl<T> ValueEncoder<MessageEncoding, T> for ()
where
    T: RawMessage,
{
    #[inline]
    fn encode_value<B: BufMut + ?Sized>(value: &T, buf: &mut B) {
        encode_varint(value.raw_encoded_len() as u64, buf);
        value.raw_encode(buf);
    }

    #[inline]
    fn prepend_value<B: ReverseBuf + ?Sized>(value: &T, buf: &mut B) {
        let end = buf.remaining();
        value.raw_prepend(buf);
        prepend_varint((buf.remaining() - end) as u64, buf);
    }

    #[inline]
    fn value_encoded_len(value: &T) -> usize {
        let inner_len = value.raw_encoded_len();
        encoded_len_varint(inner_len as u64) + inner_len
    }
}

impl<T> ValueDecoder<MessageEncoding, T> for ()
where
    T: RawMessageDecoder,
{
    #[inline]
    fn decode_value<B: Buf + ?Sized>(
        value: &mut T,
        mut buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        ctx.limit_reached()?;
        merge(value, buf.take_length_delimited()?, ctx.enter_recursion())
    }
}

impl<T> DistinguishedValueDecoder<MessageEncoding, T> for ()
where
    T: RawDistinguishedMessageDecoder + Eq,
{
    const CHECKS_EMPTY: bool = true; // Empty messages are always zero-length

    #[inline]
    fn decode_value_distinguished<const ALLOW_EMPTY: bool>(
        value: &mut T,
        mut buf: Capped<impl Buf + ?Sized>,
        ctx: RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError> {
        ctx.limit_reached()?;
        let buf = buf.take_length_delimited()?;
        // Empty message types always encode and decode from zero bytes. It is far cheaper to check
        // here than to check after the value has been decoded and checking the message's
        // `is_empty()`.
        if !ALLOW_EMPTY && buf.remaining_before_cap() == 0 {
            return ctx.check(Canonicity::NotCanonical);
        }
        merge_distinguished(value, buf, ctx.enter_recursion())
    }
}

impl<'a, T> ValueBorrowDecoder<'a, MessageEncoding, T> for ()
where
    T: RawMessageBorrowDecoder<'a>,
{
    #[inline]
    fn borrow_decode_value(
        value: &mut T,
        mut buf: Capped<&'a [u8]>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        ctx.limit_reached()?;
        borrow_merge(value, buf.take_length_delimited()?, ctx.enter_recursion())
    }
}

impl<'a, T> DistinguishedValueBorrowDecoder<'a, MessageEncoding, T> for ()
where
    T: RawDistinguishedMessageBorrowDecoder<'a> + Eq,
{
    const CHECKS_EMPTY: bool = true; // Empty messages are always zero-length

    #[inline]
    fn borrow_decode_value_distinguished<const ALLOW_EMPTY: bool>(
        value: &mut T,
        mut buf: Capped<&'a [u8]>,
        ctx: RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError> {
        ctx.limit_reached()?;
        let buf = buf.take_length_delimited()?;
        // Empty message types always encode and decode from zero bytes. It is far cheaper to check
        // here than to check after the value has been decoded and checking the message's
        // `is_empty()`.
        if !ALLOW_EMPTY && buf.remaining_before_cap() == 0 {
            return ctx.check(Canonicity::NotCanonical);
        }
        borrow_merge_distinguished(value, buf, ctx.enter_recursion())
    }
}
