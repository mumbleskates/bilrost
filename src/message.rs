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
fn merge<T: TaggedDecodable, B: Buf + ?Sized>(
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
        value.decode_tagged_field(tag, wire_type, duplicated, buf.lend(), ctx.clone())
    })
    .collect()
}

/// Merges fields from the given buffer, to its cap, into the given `DistinguishedTaggedDecodable`
/// value. Implemented as a private standalone method to discourage "merging" as a usage pattern.
#[inline]
fn merge_distinguished<T: DistinguishedTaggedDecodable, B: Buf + ?Sized>(
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
        value.decode_tagged_field_distinguished(tag, wire_type, duplicated, buf.lend(), ctx.clone())
    })
    .collect()
}

/// A Bilrost message.
///
/// Messages are expected to have a `Default` implementation that is exactly the same as each
/// field's respective `Default` value; that is, functionally identical to a derived `Default`.
// TODO(widders): make TaggedDecodable not a supertrait to remove its methods from this one
pub trait Message: TaggedDecodable + Default {
    /// Encodes the message to a buffer.
    ///
    /// This method will panic if the buffer has insufficient capacity.
    ///
    /// Meant to be used only by `Message` implementations.
    // TODO(widders): move this into TaggedDecodable to get it off here too?
    #[doc(hidden)]
    fn encode_raw<B: BufMut + ?Sized>(&self, buf: &mut B);

    /// Returns the encoded length of the message without a length delimiter.
    fn encoded_len(&self) -> usize;

    /// Encodes the message to a buffer.
    ///
    /// An error will be returned if the buffer does not have sufficient capacity.
    fn encode<B: BufMut + ?Sized>(&self, buf: &mut B) -> Result<(), EncodeError> {
        let required = self.encoded_len();
        let remaining = buf.remaining_mut();
        if required > buf.remaining_mut() {
            return Err(EncodeError::new(required, remaining));
        }

        self.encode_raw(buf);
        Ok(())
    }

    /// Encodes the message to a newly allocated buffer.
    fn encode_to_vec(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(self.encoded_len());
        self.encode_raw(&mut buf);
        buf
    }

    /// Encodes the message to a `Bytes` buffer.
    fn encode_to_bytes(&self) -> Bytes {
        let mut buf = BytesMut::with_capacity(self.encoded_len());
        self.encode_raw(&mut buf);
        buf.freeze()
    }

    /// Encodes the message with a length-delimiter to a buffer.
    ///
    /// An error will be returned if the buffer does not have sufficient capacity.
    fn encode_length_delimited<B: BufMut + ?Sized>(&self, buf: &mut B) -> Result<(), EncodeError> {
        let len = self.encoded_len();
        let required = len + encoded_len_varint(len as u64);
        let remaining = buf.remaining_mut();
        if required > remaining {
            return Err(EncodeError::new(required, remaining));
        }
        encode_varint(len as u64, buf);
        self.encode_raw(buf);
        Ok(())
    }

    /// Encodes the message with a length-delimiter to a newly allocated buffer.
    fn encode_length_delimited_to_vec(&self) -> Vec<u8> {
        let len = self.encoded_len();
        let mut buf = Vec::with_capacity(len + encoded_len_varint(len as u64));

        encode_varint(len as u64, &mut buf);
        self.encode_raw(&mut buf);
        buf
    }

    /// Encodes the message with a length-delimiter to a `Bytes` buffer.
    fn encode_length_delimited_to_bytes(&self) -> Bytes {
        let len = self.encoded_len();
        let mut buf = BytesMut::with_capacity(len + encoded_len_varint(len as u64));

        encode_varint(len as u64, &mut buf);
        self.encode_raw(&mut buf);
        buf.freeze()
    }

    /// Decodes an instance of the message from a buffer.
    ///
    /// The entire buffer will be consumed.
    fn decode<B: Buf>(mut buf: B) -> Result<Self, DecodeError> {
        Self::decode_capped(Capped::new(&mut buf))
    }

    /// Decodes a length-delimited instance of the message from the buffer.
    fn decode_length_delimited<B: Buf>(mut buf: B) -> Result<Self, DecodeError> {
        Self::decode_capped(Capped::new_length_delimited(&mut buf)?)
    }

    /// Decodes an instance from the given `Capped` buffer, consuming it to its cap.
    fn decode_capped<B: Buf>(buf: Capped<B>) -> Result<Self, DecodeError> {
        let mut message = Self::default();
        merge(&mut message, buf, DecodeContext::default())?;
        Ok(message)
    }

    // TODO(widders): encode and decode with unknown fields in an unknown-fields companion struct
}

pub trait DistinguishedMessage: Message + DistinguishedTaggedDecodable {
    /// Decodes an instance of the message from a buffer in distinguished mode.
    ///
    /// The entire buffer will be consumed.
    fn distinguished_from_buffer<B: Buf>(mut buf: B) -> Result<Self, DecodeError> {
        Self::distinguished_from_capped(Capped::new(&mut buf))
    }

    /// Decodes a length-delimited instance of the message from the buffer in distinguished mode.
    fn distinguished_from_length_delimited<B: Buf>(mut buf: B) -> Result<Self, DecodeError> {
        Self::distinguished_from_capped(Capped::new_length_delimited(&mut buf)?)
    }

    /// Decodes an instance from the given `Capped` buffer in distinguished mode, consuming it to
    /// its cap.
    fn distinguished_from_capped<B: Buf>(buf: Capped<B>) -> Result<Self, DecodeError> {
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
    T: Message,
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
        merge(self, buf, DecodeContext::default())
    }
}

pub trait DistinguishedMessageDyn {
    fn replace_distinguished_from(&mut self, buf: &mut dyn Buf) -> Result<(), DecodeError>;
    fn replace_distinguished_from_length_delimited(&mut self, buf: &mut dyn Buf) -> Result<(), DecodeError>;
    fn replace_distinguished_from_capped(&mut self, buf: Capped<dyn Buf>) -> Result<(), DecodeError>;
}
impl<T> DistinguishedMessageDyn for T
where
    T: DistinguishedMessage,
{
    fn replace_distinguished_from(&mut self, buf: &mut dyn Buf) -> Result<(), DecodeError> {
        self.replace_distinguished_from_capped(Capped::new(buf))
    }

    fn replace_distinguished_from_length_delimited(&mut self, buf: &mut dyn Buf) -> Result<(), DecodeError> {
        self.replace_distinguished_from_capped(Capped::new_length_delimited(buf)?)
    }

    fn replace_distinguished_from_capped(&mut self, buf: Capped<dyn Buf>) -> Result<(), DecodeError> {
        self.clear();
        merge_distinguished(self, buf, DecodeContext::default())
    }
}

/// Trait to be implemented by (or more commonly derived for) oneofs and messages, which have
/// knowledge of their fields' tags and encoding.
pub trait TaggedDecodable: Default {
    /// Decodes a field from a buffer into `self`.
    ///
    /// Meant to be used only by `Message` and `Oneof` implementations.
    fn decode_tagged_field<B: Buf + ?Sized>(
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

/// Complementary trait for oneof fields and messages, all of whose fields have a distinguished
/// encoding.
pub trait DistinguishedTaggedDecodable {
    fn decode_tagged_field_distinguished<B: Buf + ?Sized>(
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

impl<M> TaggedDecodable for Box<M>
where
    M: Message,
{
    fn decode_tagged_field<B: Buf + ?Sized>(
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
        (**self).decode_tagged_field(tag, wire_type, duplicated, buf, ctx)
    }
}

impl<M> Message for Box<M>
where
    M: Message,
{
    fn encode_raw<B: BufMut + ?Sized>(&self, buf: &mut B) {
        (**self).encode_raw(buf)
    }

    fn encoded_len(&self) -> usize {
        (**self).encoded_len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const _MESSAGE_DYN_IS_OBJECT_SAFE: Option<&dyn MessageDyn> = None;
    const _DISTINGUISHED_MESSAGE_DYN_IS_OBJECT_SAFE: Option<&dyn DistinguishedMessageDyn> = None;

    fn use_dyn_messages<M: Message>(safe: &mut dyn MessageDyn, mut msg: M) {
        let mut vec = Vec::<u8>::new();

        safe.encoded_len_dyn();
        safe.encode_dyn(&mut vec).unwrap();
        safe.replace_from_length_delimited(&mut [0u8].as_slice()).unwrap();

        msg.encoded_len();
        msg = M::decode_length_delimited(&mut [0u8].as_slice()).unwrap();
        msg.encode(&mut vec).unwrap();
        msg.encode_dyn(&mut vec).unwrap();
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
