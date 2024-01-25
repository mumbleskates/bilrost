use alloc::boxed::Box;
use alloc::vec::Vec;

use bytes::{Buf, BufMut, Bytes, BytesMut};

use crate::encoding::{
    encode_varint, encoded_len_varint, Capped, DecodeContext, TagReader, WireType,
};
use crate::{DecodeError, EncodeError};

/// Merges fields from the given buffer, to its cap, into the given `TaggedDecodable` value.
/// Implemented as a private standalone method to discourage "merging" as a usage pattern.
#[inline]
pub(crate) fn merge<T: RawMessage, B: Buf + ?Sized>(
    value: &mut T,
    buf: Capped<B>,
    ctx: DecodeContext,
) -> Result<(), DecodeError> {
    let tr = &mut TagReader::new();
    let mut last_tag = None::<u32>;
    buf.consume(|buf| {
        let (tag, wire_type) = tr.decode_key(buf.buf())?;
        let duplicated = last_tag == Some(tag);
        last_tag = Some(tag);
        value.raw_decode_field(tag, wire_type, duplicated, buf.lend(), ctx.clone())
    })
    .collect()
}

/// Merges fields from the given buffer, to its cap, into the given `DistinguishedTaggedDecodable`
/// value. Implemented as a private standalone method to discourage "merging" as a usage pattern.
#[inline]
pub(crate) fn merge_distinguished<T: RawDistinguishedMessage, B: Buf + ?Sized>(
    value: &mut T,
    buf: Capped<B>,
    ctx: DecodeContext,
) -> Result<(), DecodeError> {
    let tr = &mut TagReader::new();
    let mut last_tag = None::<u32>;
    buf.consume(|buf| {
        let (tag, wire_type) = tr.decode_key(buf.buf())?;
        let duplicated = last_tag == Some(tag);
        last_tag = Some(tag);
        value.raw_decode_field_distinguished(tag, wire_type, duplicated, buf.lend(), ctx.clone())
    })
    .collect()
}

/// A Bilrost message. Provides basic encoding and decoding functionality for message types. For
/// an object-safe proxy trait, see `MessageDyn`.
///
/// Messages are expected to have a `Default` implementation that is exactly the same as each
/// field's respective `Default` value; that is, functionally identical to a derived `Default`. This
/// is automatically satisfied by the derive macro.
pub trait Message: Default {
    /// Returns the encoded length of the message without a length delimiter.
    fn encoded_len(&self) -> usize;

    /// Encodes the message to a buffer.
    ///
    /// An error will be returned if the buffer does not have sufficient capacity.
    fn encode<B: BufMut + ?Sized>(&self, buf: &mut B) -> Result<(), EncodeError>;

    /// Encodes the message to a newly allocated buffer.
    fn encode_to_vec(&self) -> Vec<u8>;

    /// Encodes the message to a `Bytes` buffer.
    fn encode_to_bytes(&self) -> Bytes;

    /// Encodes the message with a length-delimiter to a buffer.
    ///
    /// An error will be returned if the buffer does not have sufficient capacity.
    fn encode_length_delimited<B: BufMut + ?Sized>(&self, buf: &mut B) -> Result<(), EncodeError>;

    /// Encodes the message with a length-delimiter to a newly allocated buffer.
    fn encode_length_delimited_to_vec(&self) -> Vec<u8>;

    /// Encodes the message with a length-delimiter to a `Bytes` buffer.
    fn encode_length_delimited_to_bytes(&self) -> Bytes;

    /// Decodes an instance of the message from a buffer.
    ///
    /// The entire buffer will be consumed.
    fn decode<B: Buf>(buf: B) -> Result<Self, DecodeError>;

    /// Decodes a length-delimited instance of the message from the buffer.
    fn decode_length_delimited<B: Buf>(buf: B) -> Result<Self, DecodeError>;

    /// Decodes an instance from the given `Capped` buffer, consuming it to its cap.
    fn decode_capped<B: Buf + ?Sized>(buf: Capped<B>) -> Result<Self, DecodeError>;

    // TODO(widders): encode and decode with unknown fields in an unknown-fields companion struct
}

/// An enhanced trait for Bilrost messages that promise a distinguished representation.
/// Implementation of this trait comes with the following promises:
///
///  1. The message will always encode to the same bytes as any other message with an equal value
///  2. A message equal to that value will only ever decode without error from that exact sequence
///     of bytes, not from any other.
pub trait DistinguishedMessage: Message + Eq {
    /// Decodes an instance of the message from a buffer in distinguished mode.
    ///
    /// The entire buffer will be consumed.
    fn decode_distinguished<B: Buf>(buf: B) -> Result<Self, DecodeError>;

    /// Decodes a length-delimited instance of the message from the buffer in distinguished mode.
    fn decode_distinguished_length_delimited<B: Buf>(buf: B) -> Result<Self, DecodeError>;

    /// Decodes an instance from the given `Capped` buffer in distinguished mode, consuming it to
    /// its cap.
    fn decode_distinguished_capped<B: Buf + ?Sized>(buf: Capped<B>) -> Result<Self, DecodeError>;
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
    fn encoded_len(&self) -> usize {
        self.raw_encoded_len()
    }

    fn encode<B: BufMut + ?Sized>(&self, buf: &mut B) -> Result<(), EncodeError> {
        let required = self.encoded_len();
        let remaining = buf.remaining_mut();
        if required > buf.remaining_mut() {
            return Err(EncodeError::new(required, remaining));
        }

        self.raw_encode(buf);
        Ok(())
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

    fn decode<B: Buf>(mut buf: B) -> Result<Self, DecodeError> {
        Self::decode_capped(Capped::new(&mut buf))
    }

    fn decode_length_delimited<B: Buf>(mut buf: B) -> Result<Self, DecodeError> {
        Self::decode_capped(Capped::new_length_delimited(&mut buf)?)
    }

    fn decode_capped<B: Buf + ?Sized>(buf: Capped<B>) -> Result<Self, DecodeError> {
        let mut message = Self::default();
        merge(&mut message, buf, DecodeContext::default())?;
        Ok(message)
    }
}

impl<T> DistinguishedMessage for T
where
    T: RawDistinguishedMessage + Message + Eq,
{
    fn decode_distinguished<B: Buf>(mut buf: B) -> Result<Self, DecodeError> {
        Self::decode_distinguished_capped(Capped::new(&mut buf))
    }

    fn decode_distinguished_length_delimited<B: Buf>(mut buf: B) -> Result<Self, DecodeError> {
        Self::decode_distinguished_capped(Capped::new_length_delimited(&mut buf)?)
    }

    fn decode_distinguished_capped<B: Buf + ?Sized>(buf: Capped<B>) -> Result<Self, DecodeError> {
        let mut message = Self::default();
        merge_distinguished(&mut message, buf, DecodeContext::default())?;
        Ok(message)
    }
}

/// Object-safe interface implemented for all `Message` types. Accepts `dyn Buf` and `dyn BufMut`
/// for encoding and decoding, and `replace`-* methods for decoding in-place.
pub trait MessageDyn {
    fn encoded_len_dyn(&self) -> usize;
    fn encode_dyn(&self, buf: &mut dyn BufMut) -> Result<(), EncodeError>;
    fn encode_to_vec_dyn(&self) -> Vec<u8>;
    fn encode_length_delimited_dyn(&self, buf: &mut dyn BufMut) -> Result<(), EncodeError>;
    fn encode_length_delimited_to_vec_dyn(&self) -> Vec<u8>;
    fn encode_length_delimited_to_bytes_dyn(&self) -> Bytes;
    fn replace_from(&mut self, buf: &mut dyn Buf) -> Result<(), DecodeError>;
    fn replace_from_length_delimited(&mut self, buf: &mut dyn Buf) -> Result<(), DecodeError>;
    fn replace_from_capped(&mut self, buf: Capped<dyn Buf>) -> Result<(), DecodeError>;
}
impl<T> MessageDyn for T
where
    T: RawMessage,
{
    fn encoded_len_dyn(&self) -> usize {
        Message::encoded_len(self)
    }

    fn encode_dyn(&self, buf: &mut dyn BufMut) -> Result<(), EncodeError> {
        Message::encode(self, buf)
    }

    fn encode_to_vec_dyn(&self) -> Vec<u8> {
        Message::encode_to_vec(self)
    }

    fn encode_length_delimited_dyn(&self, buf: &mut dyn BufMut) -> Result<(), EncodeError> {
        Message::encode_length_delimited(self, buf)
    }

    fn encode_length_delimited_to_vec_dyn(&self) -> Vec<u8> {
        Message::encode_length_delimited_to_vec(self)
    }

    fn encode_length_delimited_to_bytes_dyn(&self) -> Bytes {
        Message::encode_length_delimited_to_bytes(self)
    }

    fn replace_from(&mut self, buf: &mut dyn Buf) -> Result<(), DecodeError> {
        self.replace_from_capped(Capped::new(buf))
    }

    fn replace_from_length_delimited(&mut self, buf: &mut dyn Buf) -> Result<(), DecodeError> {
        self.replace_from_capped(Capped::new_length_delimited(buf)?)
    }

    fn replace_from_capped(&mut self, buf: Capped<dyn Buf>) -> Result<(), DecodeError> {
        self.clear();
        merge(self, buf, DecodeContext::default()).map_err(|err| {
            self.clear();
            err
        })
    }
}

pub trait DistinguishedMessageDyn {
    fn replace_distinguished_from(&mut self, buf: &mut dyn Buf) -> Result<(), DecodeError>;
    fn replace_distinguished_from_length_delimited(
        &mut self,
        buf: &mut dyn Buf,
    ) -> Result<(), DecodeError>;
    fn replace_distinguished_from_capped(
        &mut self,
        buf: Capped<dyn Buf>,
    ) -> Result<(), DecodeError>;
}
impl<T> DistinguishedMessageDyn for T
where
    T: RawDistinguishedMessage,
{
    fn replace_distinguished_from(&mut self, buf: &mut dyn Buf) -> Result<(), DecodeError> {
        self.replace_distinguished_from_capped(Capped::new(buf))
    }

    fn replace_distinguished_from_length_delimited(
        &mut self,
        buf: &mut dyn Buf,
    ) -> Result<(), DecodeError> {
        self.replace_distinguished_from_capped(Capped::new_length_delimited(buf)?)
    }

    fn replace_distinguished_from_capped(
        &mut self,
        buf: Capped<dyn Buf>,
    ) -> Result<(), DecodeError> {
        self.clear();
        merge_distinguished(self, buf, DecodeContext::default()).map_err(|err| {
            self.clear();
            err
        })
    }
}

/// Trait to be implemented by messages, which have knowledge of their fields' tags and encoding.
/// The methods of this trait are meant to only be used by the `Message` implementation.
pub trait RawMessage: Default {
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

    /// Clears the message, resetting all fields to their default.
    fn clear(&mut self) {
        *self = Self::default();
    }
}

/// Complementary underlying trait for distinguished messages, all of whose fields have a
/// distinguished encoding.
pub trait RawDistinguishedMessage: RawMessage {
    fn raw_decode_field_distinguished<B: Buf + ?Sized>(
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

impl<T> RawMessage for Box<T>
where
    T: RawMessage,
{
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
    ) -> Result<(), DecodeError>
    where
        Self: Sized,
    {
        (**self).raw_decode_field_distinguished(tag, wire_type, duplicated, buf, ctx)
    }
}

#[cfg(test)]
mod tests {
    use super::{DistinguishedMessageDyn, Message, MessageDyn, Vec};

    const _MESSAGE_DYN_IS_OBJECT_SAFE: Option<&dyn MessageDyn> = None;
    const _DISTINGUISHED_MESSAGE_DYN_IS_OBJECT_SAFE: Option<&dyn DistinguishedMessageDyn> = None;

    fn use_dyn_messages<M: Message>(safe: &mut dyn MessageDyn, mut msg: M) {
        let mut vec = Vec::<u8>::new();

        safe.encoded_len_dyn();
        safe.encode_dyn(&mut vec).unwrap();
        safe.replace_from_length_delimited(&mut [0u8].as_slice())
            .unwrap();

        msg.encoded_len();
        msg = M::decode_length_delimited(&mut [0u8].as_slice()).unwrap();
        msg.encode(&mut vec).unwrap();
    }

    #[test]
    fn using_dyn_messages() {
        let mut vec = Vec::<u8>::new();
        use_dyn_messages(&mut (), ());
        assert_eq!(().encoded_len(), 0);
        assert_eq!(().encoded_len_dyn(), 0);
        ().encode(&mut vec).unwrap();
        ().encode_dyn(&mut vec).unwrap();
        <()>::decode(&mut [].as_slice()).unwrap();
    }
}
