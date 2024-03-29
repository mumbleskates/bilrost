use alloc::boxed::Box;
use alloc::vec::Vec;

use bytes::{Buf, BufMut, Bytes, BytesMut};

use crate::encoding::{
    encode_varint, encoded_len_varint, Canonicity, Capped, DecodeContext, EmptyState, TagReader,
    WireType,
};
use crate::{DecodeError, EncodeError};

/// Merges fields from the given buffer, to its cap, into the given `TaggedDecodable` value.
/// Implemented as a private standalone method to discourage "merging" as a usage pattern.
#[inline]
pub(crate) fn merge<T: RawMessage, B: Buf + ?Sized>(
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

/// Merges fields from the given buffer, to its cap, into the given `DistinguishedTaggedDecodable`
/// value. Implemented as a private standalone method to discourage "merging" as a usage pattern.
#[inline]
pub(crate) fn merge_distinguished<T: RawDistinguishedMessage, B: Buf + ?Sized>(
    value: &mut T,
    mut buf: Capped<B>,
    ctx: DecodeContext,
) -> Result<Canonicity, DecodeError> {
    let tr = &mut TagReader::new();
    let mut last_tag = None::<u32>;
    let mut canon = Canonicity::Canonical;
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

/// A Bilrost message. Provides basic encoding and decoding functionality for message types.
pub trait Message: EmptyState {
    /// Encodes the message to a buffer.
    ///
    /// An error will be returned if the buffer does not have sufficient capacity.
    fn encode<B: BufMut + ?Sized>(&self, buf: &mut B) -> Result<(), EncodeError>
    where
        Self: Sized;

    /// Encodes the message with a length-delimiter to a buffer.
    ///
    /// An error will be returned if the buffer does not have sufficient capacity.
    fn encode_length_delimited<B: BufMut + ?Sized>(&self, buf: &mut B) -> Result<(), EncodeError>
    where
        Self: Sized;

    /// Decodes an instance of the message from a buffer.
    ///
    /// The entire buffer will be consumed.
    fn decode<B: Buf>(buf: B) -> Result<Self, DecodeError>
    where
        Self: Sized;

    /// Decodes a length-delimited instance of the message from the buffer.
    fn decode_length_delimited<B: Buf>(buf: B) -> Result<Self, DecodeError>
    where
        Self: Sized;

    /// Decodes an instance from the given `Capped` buffer, consuming it to its cap.
    #[doc(hidden)]
    fn decode_capped<B: Buf + ?Sized>(buf: Capped<B>) -> Result<Self, DecodeError>
    where
        Self: Sized;

    /// Decodes the non-ignored fields of this message from the buffer, replacing their values.
    fn replace_from<B: Buf>(&mut self, buf: B) -> Result<(), DecodeError>
    where
        Self: Sized;

    /// Decodes the non-ignored fields of this message, replacing their values from a
    /// length-delimited value encoded in the buffer.
    fn replace_from_length_delimited<B: Buf>(&mut self, buf: B) -> Result<(), DecodeError>
    where
        Self: Sized;

    /// Decodes the non-ignored fields of this message, replacing their values from the given capped
    /// buffer.
    #[doc(hidden)]
    fn replace_from_capped<B: Buf + ?Sized>(&mut self, buf: Capped<B>) -> Result<(), DecodeError>
    where
        Self: Sized;

    // ------------ Object-safe methods follow ------------

    /// Returns the encoded length of the message without a length delimiter.
    fn encoded_len(&self) -> usize;

    /// Encodes the message to a newly allocated buffer.
    fn encode_to_vec(&self) -> Vec<u8>;

    /// Encodes the message to a `Bytes` buffer.
    fn encode_to_bytes(&self) -> Bytes;

    /// Encodes the message to a `Bytes` buffer.
    fn encode_dyn(&self, buf: &mut dyn BufMut) -> Result<(), EncodeError>;

    /// Encodes the message with a length-delimiter to a newly allocated buffer.
    fn encode_length_delimited_to_vec(&self) -> Vec<u8>;

    /// Encodes the message with a length-delimiter to a `Bytes` buffer.
    fn encode_length_delimited_to_bytes(&self) -> Bytes;

    /// Encodes the message with a length-delimiter to a `Bytes` buffer.
    fn encode_length_delimited_dyn(&self, buf: &mut dyn BufMut) -> Result<(), EncodeError>;

    /// Decodes the non-ignored fields of this message from the buffer, replacing their values.
    fn replace_from_slice(&mut self, buf: &[u8]) -> Result<(), DecodeError>;

    /// Decodes the non-ignored fields of this message, replacing their values from a
    /// length-delimited value encoded in the buffer.
    fn replace_from_length_delimited_slice(&mut self, buf: &[u8]) -> Result<(), DecodeError>;

    /// Decodes the non-ignored fields of this message from the buffer, replacing their values.
    fn replace_from_dyn(&mut self, buf: &mut dyn Buf) -> Result<(), DecodeError>;

    /// Decodes the non-ignored fields of this message, replacing their values from a
    /// length-delimited value encoded in the buffer.
    fn replace_from_length_delimited_dyn(&mut self, buf: &mut dyn Buf) -> Result<(), DecodeError>;

    /// Decodes the non-ignored fields of this message, replacing their values from the given capped
    /// buffer.
    #[doc(hidden)]
    fn replace_from_capped_dyn(&mut self, buf: Capped<dyn Buf>) -> Result<(), DecodeError>;
}

/// An enhanced trait for Bilrost messages that promise a distinguished representation.
/// Implementation of this trait comes with the following promises:
///
///  1. The message will always encode to the same bytes as any other message with an equal value.
///  2. A message equal to that value will only ever decode canonically and without error from that
///     exact sequence of bytes, not from any other.
pub trait DistinguishedMessage: Message {
    /// Decodes an instance of the message from a buffer in distinguished mode.
    ///
    /// The entire buffer will be consumed.
    fn decode_distinguished<B: Buf>(buf: B) -> Result<(Self, Canonicity), DecodeError>
    where
        Self: Sized;

    /// Decodes a length-delimited instance of the message from the buffer in distinguished mode.
    fn decode_distinguished_length_delimited<B: Buf>(
        buf: B,
    ) -> Result<(Self, Canonicity), DecodeError>
    where
        Self: Sized;

    /// Decodes an instance from the given `Capped` buffer in distinguished mode, consuming it to
    /// its cap.
    #[doc(hidden)]
    fn decode_distinguished_capped<B: Buf + ?Sized>(
        buf: Capped<B>,
    ) -> Result<(Self, Canonicity), DecodeError>
    where
        Self: Sized;

    /// Decodes the non-ignored fields of this message from the buffer in distinguished mode,
    /// replacing their values.
    fn replace_distinguished_from<B: Buf>(&mut self, buf: B) -> Result<Canonicity, DecodeError>
    where
        Self: Sized;

    /// Decodes the non-ignored fields of this message in distinguished mode, replacing their values
    /// from a length-delimited value encoded in the buffer.
    fn replace_distinguished_from_length_delimited<B: Buf>(
        &mut self,
        buf: B,
    ) -> Result<Canonicity, DecodeError>
    where
        Self: Sized;

    /// Decodes the non-ignored fields of this message in distinguished mode, replacing their values
    /// from the given capped buffer.
    #[doc(hidden)]
    fn replace_distinguished_from_capped<B: Buf + ?Sized>(
        &mut self,
        buf: Capped<B>,
    ) -> Result<Canonicity, DecodeError>
    where
        Self: Sized;

    // ------------ Object-safe methods follow ------------

    /// Decodes a length-delimited instance of the message from the buffer in distinguished mode.
    fn replace_distinguished_from_slice(&mut self, buf: &[u8]) -> Result<Canonicity, DecodeError>;

    /// Decodes the non-ignored fields of this message, replacing their values from a
    /// length-delimited value encoded in the buffer in distinguished mode.
    fn replace_distinguished_from_dyn(
        &mut self,
        buf: &mut dyn Buf,
    ) -> Result<Canonicity, DecodeError>;

    /// Decodes the non-ignored fields of this message from the buffer in distinguished mode,
    /// replacing their values.
    fn replace_distinguished_from_length_delimited_slice(
        &mut self,
        buf: &[u8],
    ) -> Result<Canonicity, DecodeError>;

    /// Decodes the non-ignored fields of this message, replacing their values from a
    /// length-delimited value encoded in the buffer in distinguished mode.
    fn replace_distinguished_from_length_delimited_dyn(
        &mut self,
        buf: &mut dyn Buf,
    ) -> Result<Canonicity, DecodeError>;

    /// Decodes the non-ignored fields of this message, replacing their values from the given capped
    /// buffer in distinguished mode.
    #[doc(hidden)]
    fn replace_distinguished_from_capped_dyn(
        &mut self,
        buf: Capped<dyn Buf>,
    ) -> Result<Canonicity, DecodeError>;
}

/// `Message` is implemented as a usability layer on top of the basic functionality afforded by
/// `RawMessage`.
// TODO(widders): in the future, make it possible to decode with extension Message types for all
//  fields not covered by the own type. The default extension can be `()`, which always skips in
//  expedient mode and always errs in distinguished mode; the most permissive possible extension
//  would then be OpaqueMessage, which losslessly captures all unknown fields. A composing wrapper
//  type that combines two message types in an overlay can be implemented. This will require an
//  alternate encoding mode which emits field groups to be sorted in a stricter way, only grouping
//  truly contiguous runs of field ids so that they can be sorted with any other type's fields at
//  runtime.
impl<T> Message for T
where
    T: RawMessage,
{
    fn encode<B: BufMut + ?Sized>(&self, buf: &mut B) -> Result<(), EncodeError> {
        let required = self.encoded_len();
        let remaining = buf.remaining_mut();
        if required > remaining {
            return Err(EncodeError::new(required, remaining));
        }

        self.raw_encode(buf);
        Ok(())
    }

    fn encode_length_delimited<B: BufMut + ?Sized>(&self, buf: &mut B) -> Result<(), EncodeError> {
        let len = self.encoded_len();
        let required = len + encoded_len_varint(len as u64);
        let remaining = buf.remaining_mut();
        if required > remaining {
            return Err(EncodeError::new(required, remaining));
        }
        encode_varint(len as u64, buf);
        self.raw_encode(buf);
        Ok(())
    }

    fn decode<B: Buf>(mut buf: B) -> Result<Self, DecodeError> {
        Self::decode_capped(Capped::new(&mut buf))
    }

    fn decode_length_delimited<B: Buf>(mut buf: B) -> Result<Self, DecodeError> {
        Self::decode_capped(Capped::new_length_delimited(&mut buf)?)
    }

    #[doc(hidden)]
    fn decode_capped<B: Buf + ?Sized>(buf: Capped<B>) -> Result<Self, DecodeError> {
        let mut message = Self::empty();
        merge(&mut message, buf, DecodeContext::default())?;
        Ok(message)
    }

    fn replace_from<B: Buf>(&mut self, mut buf: B) -> Result<(), DecodeError> {
        self.replace_from_capped(Capped::new(&mut buf))
    }

    fn replace_from_length_delimited<B: Buf>(&mut self, mut buf: B) -> Result<(), DecodeError> {
        self.replace_from_capped(Capped::new_length_delimited(&mut buf)?)
    }

    #[doc(hidden)]
    fn replace_from_capped<B: Buf + ?Sized>(&mut self, buf: Capped<B>) -> Result<(), DecodeError> {
        self.clear();
        merge(self, buf, DecodeContext::default()).map_err(|err| {
            self.clear();
            err
        })
    }

    fn encoded_len(&self) -> usize {
        self.raw_encoded_len()
    }

    fn encode_to_vec(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(self.encoded_len());
        self.raw_encode(&mut buf);
        buf
    }

    fn encode_to_bytes(&self) -> Bytes {
        let mut buf = BytesMut::with_capacity(self.encoded_len());
        self.raw_encode(&mut buf);
        buf.freeze()
    }

    fn encode_dyn(&self, buf: &mut dyn BufMut) -> Result<(), EncodeError> {
        self.encode(buf)
    }

    fn encode_length_delimited_to_vec(&self) -> Vec<u8> {
        let len = self.encoded_len();
        let mut buf = Vec::with_capacity(len + encoded_len_varint(len as u64));

        encode_varint(len as u64, &mut buf);
        self.raw_encode(&mut buf);
        buf
    }

    fn encode_length_delimited_to_bytes(&self) -> Bytes {
        let len = self.encoded_len();
        let mut buf = BytesMut::with_capacity(len + encoded_len_varint(len as u64));

        encode_varint(len as u64, &mut buf);
        self.raw_encode(&mut buf);
        buf.freeze()
    }

    fn encode_length_delimited_dyn(&self, buf: &mut dyn BufMut) -> Result<(), EncodeError> {
        self.encode_length_delimited(buf)
    }

    fn replace_from_slice(&mut self, buf: &[u8]) -> Result<(), DecodeError> {
        self.replace_from(buf)
    }

    fn replace_from_length_delimited_slice(&mut self, buf: &[u8]) -> Result<(), DecodeError> {
        self.replace_from_length_delimited(buf)
    }

    fn replace_from_dyn(&mut self, buf: &mut dyn Buf) -> Result<(), DecodeError> {
        self.replace_from(buf)
    }

    fn replace_from_length_delimited_dyn(&mut self, buf: &mut dyn Buf) -> Result<(), DecodeError> {
        self.replace_from_length_delimited(buf)
    }

    #[doc(hidden)]
    fn replace_from_capped_dyn(&mut self, buf: Capped<dyn Buf>) -> Result<(), DecodeError> {
        self.replace_from_capped(buf)
    }
}

impl<T> DistinguishedMessage for T
where
    T: RawDistinguishedMessage + Message,
{
    fn decode_distinguished<B: Buf>(mut buf: B) -> Result<(Self, Canonicity), DecodeError> {
        Self::decode_distinguished_capped(Capped::new(&mut buf))
    }

    fn decode_distinguished_length_delimited<B: Buf>(
        mut buf: B,
    ) -> Result<(Self, Canonicity), DecodeError> {
        Self::decode_distinguished_capped(Capped::new_length_delimited(&mut buf)?)
    }

    #[doc(hidden)]
    fn decode_distinguished_capped<B: Buf + ?Sized>(
        buf: Capped<B>,
    ) -> Result<(Self, Canonicity), DecodeError> {
        let mut message = Self::empty();
        let canon = merge_distinguished(&mut message, buf, DecodeContext::default())?;
        Ok((message, canon))
    }

    fn replace_distinguished_from<B: Buf>(
        &mut self,
        mut buf: B,
    ) -> Result<Canonicity, DecodeError> {
        self.replace_distinguished_from_capped(Capped::new(&mut buf))
    }

    fn replace_distinguished_from_length_delimited<B: Buf>(
        &mut self,
        mut buf: B,
    ) -> Result<Canonicity, DecodeError> {
        self.replace_distinguished_from_capped(Capped::new_length_delimited(&mut buf)?)
    }

    #[doc(hidden)]
    fn replace_distinguished_from_capped<B: Buf + ?Sized>(
        &mut self,
        buf: Capped<B>,
    ) -> Result<Canonicity, DecodeError> {
        self.clear();
        merge_distinguished(self, buf, DecodeContext::default()).map_err(|err| {
            self.clear();
            err
        })
    }

    fn replace_distinguished_from_slice(&mut self, buf: &[u8]) -> Result<Canonicity, DecodeError> {
        self.replace_distinguished_from(buf)
    }

    fn replace_distinguished_from_dyn(
        &mut self,
        buf: &mut dyn Buf,
    ) -> Result<Canonicity, DecodeError> {
        self.replace_distinguished_from(buf)
    }

    fn replace_distinguished_from_length_delimited_slice(
        &mut self,
        buf: &[u8],
    ) -> Result<Canonicity, DecodeError> {
        self.replace_distinguished_from_length_delimited(buf)
    }

    fn replace_distinguished_from_length_delimited_dyn(
        &mut self,
        buf: &mut dyn Buf,
    ) -> Result<Canonicity, DecodeError> {
        self.replace_distinguished_from_length_delimited(buf)
    }

    #[doc(hidden)]
    fn replace_distinguished_from_capped_dyn(
        &mut self,
        buf: Capped<dyn Buf>,
    ) -> Result<Canonicity, DecodeError> {
        self.replace_distinguished_from_capped(buf)
    }
}

/// Trait to be implemented by messages, which have knowledge of their fields' tags and encoding.
/// The methods of this trait are meant to only be used by the `Message` implementation.
pub trait RawMessage: EmptyState {
    const __ASSERTIONS: ();

    /// Encodes the message to a buffer.
    ///
    /// This method will panic if the buffer has insufficient capacity.
    fn raw_encode<B: BufMut + ?Sized>(&self, buf: &mut B);

    /// Returns the encoded length of the message without a length delimiter.
    fn raw_encoded_len(&self) -> usize;

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
pub trait RawDistinguishedMessage: RawMessage + Eq {
    fn raw_decode_field_distinguished<B: Buf + ?Sized>(
        &mut self,
        tag: u32,
        wire_type: WireType,
        duplicated: bool,
        buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<Canonicity, DecodeError>
    where
        Self: Sized;
}

impl<T> EmptyState for Box<T>
where
    T: EmptyState,
{
    fn empty() -> Self {
        Self::new(T::empty())
    }

    fn is_empty(&self) -> bool {
        self.as_ref().is_empty()
    }

    fn clear(&mut self) {
        self.as_mut().clear()
    }
}

impl<T> RawMessage for Box<T>
where
    T: RawMessage,
{
    const __ASSERTIONS: () = ();

    fn raw_encode<B: BufMut + ?Sized>(&self, buf: &mut B) {
        (**self).raw_encode(buf)
    }

    fn raw_encoded_len(&self) -> usize {
        (**self).raw_encoded_len()
    }

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

impl<T> RawDistinguishedMessage for Box<T>
where
    T: RawDistinguishedMessage,
{
    fn raw_decode_field_distinguished<B: Buf + ?Sized>(
        &mut self,
        tag: u32,
        wire_type: WireType,
        duplicated: bool,
        buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<Canonicity, DecodeError>
    where
        Self: Sized,
    {
        (**self).raw_decode_field_distinguished(tag, wire_type, duplicated, buf, ctx)
    }
}

#[cfg(test)]
mod tests {
    use super::{DistinguishedMessage, Message, Vec};
    use crate::WithCanonicity;

    const _MESSAGE_DYN_IS_OBJECT_SAFE: Option<&dyn Message> = None;
    const _DISTINGUISHED_MESSAGE_DYN_IS_OBJECT_SAFE: Option<&dyn DistinguishedMessage> = None;

    fn use_dyn_messages<M: Message>(safe: &mut dyn Message, mut msg: M) {
        let mut vec = Vec::<u8>::new();

        safe.encoded_len();
        safe.encode_dyn(&mut vec).unwrap();
        assert_eq!(vec, safe.encode_to_vec());
        safe.replace_from_length_delimited_dyn(&mut [0u8].as_slice())
            .unwrap();

        msg.encoded_len();
        msg = M::decode_length_delimited(&mut [0u8].as_slice()).unwrap();
        msg.encode(&mut vec).unwrap();
        msg.clear();
    }

    fn use_dyn_distinguished_messages<M: DistinguishedMessage>(
        safe: &mut dyn DistinguishedMessage,
        mut msg: M,
    ) {
        let mut vec = Vec::<u8>::new();

        safe.encoded_len();
        safe.encode_dyn(&mut vec).unwrap();
        assert_eq!(vec, safe.encode_to_vec());
        safe.replace_from_length_delimited_dyn(&mut [0u8].as_slice())
            .unwrap();
        safe.replace_distinguished_from_length_delimited_dyn(&mut [0u8].as_slice())
            .canonical()
            .unwrap();
        safe.clear();

        msg.encoded_len();
        msg = M::decode_length_delimited(&mut [0u8].as_slice()).unwrap();
        msg.encode(&mut vec).unwrap();
        msg.clear();
    }

    #[test]
    fn using_dyn_messages() {
        let mut vec = Vec::<u8>::new();
        use_dyn_messages(&mut (), ());
        use_dyn_distinguished_messages(&mut (), ());
        assert_eq!(().encoded_len(), 0);
        ().encode(&mut vec).unwrap();
        ().encode_dyn(&mut vec).unwrap();
        <()>::decode(&mut [].as_slice()).unwrap();
    }
}
