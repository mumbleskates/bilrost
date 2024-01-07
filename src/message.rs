use alloc::boxed::Box;
use alloc::vec::Vec;

use bytes::{Buf, BufMut};

use crate::encoding::{
    encode_varint, encoded_len_varint, Capped, DecodeContext, General, TagReader, ValueEncoder,
    WireType,
};
use crate::{DecodeError, EncodeError};

/// A Bilrost message.
pub trait Message: TaggedDecoder + Send + Sync {
    /// Encodes the message to a buffer.
    ///
    /// This method will panic if the buffer has insufficient capacity.
    ///
    /// Meant to be used only by `Message` implementations.
    #[doc(hidden)]
    fn encode_raw<B>(&self, buf: &mut B)
    where
        B: BufMut,
        Self: Sized;

    /// Returns the encoded length of the message without a length delimiter.
    fn encoded_len(&self) -> usize;

    /// Encodes the message to a buffer.
    ///
    /// An error will be returned if the buffer does not have sufficient capacity.
    fn encode<B>(&self, buf: &mut B) -> Result<(), EncodeError>
    where
        B: BufMut,
        Self: Sized,
    {
        let required = self.encoded_len();
        let remaining = buf.remaining_mut();
        if required > buf.remaining_mut() {
            return Err(EncodeError::new(required, remaining));
        }

        self.encode_raw(buf);
        Ok(())
    }

    /// Encodes the message to a newly allocated buffer.
    fn encode_to_vec(&self) -> Vec<u8>
    where
        Self: Sized,
    {
        let mut buf = Vec::with_capacity(self.encoded_len());

        self.encode_raw(&mut buf);
        buf
    }

    /// Encodes the message with a length-delimiter to a buffer.
    ///
    /// An error will be returned if the buffer does not have sufficient capacity.
    fn encode_length_delimited<B>(&self, buf: &mut B) -> Result<(), EncodeError>
    where
        B: BufMut,
        Self: Sized,
    {
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
    fn encode_length_delimited_to_vec(&self) -> Vec<u8>
    where
        Self: Sized,
    {
        let len = self.encoded_len();
        let mut buf = Vec::with_capacity(len + encoded_len_varint(len as u64));

        encode_varint(len as u64, &mut buf);
        self.encode_raw(&mut buf);
        buf
    }

    /// Decodes an instance of the message from a buffer.
    ///
    /// The entire buffer will be consumed.
    fn decode<B>(mut buf: B) -> Result<Self, DecodeError>
    where
        B: Buf,
        Self: Default,
    {
        let mut message = Self::default();
        Self::merge(&mut message, &mut buf).map(|_| message)
    }

    /// Decodes a length-delimited instance of the message from the buffer.
    fn decode_length_delimited<B>(buf: B) -> Result<Self, DecodeError>
    where
        B: Buf,
        Self: Default,
    {
        let mut message = Self::default();
        message.merge_length_delimited(buf)?;
        Ok(message)
    }

    /// Decodes an instance of the message from a buffer, and merges it into `self`.
    ///
    /// The entire buffer will be consumed.
    fn merge<B>(&mut self, mut buf: B) -> Result<(), DecodeError>
    where
        B: Buf,
        Self: Sized,
    {
        let ctx = DecodeContext::default();
        let tr = &mut TagReader::new();
        let mut last_tag = None::<u32>;
        Capped::new(&mut buf)
            .consume(|buf| {
                let (tag, wire_type) = tr.decode_key(buf.buf())?;
                let duplicated = last_tag == Some(tag);
                last_tag = Some(tag);
                self.decode_tagged_field(tag, wire_type, duplicated, buf, ctx.clone())
            })
            .collect()
    }

    /// Decodes a length-delimited instance of the message from buffer, and
    /// merges it into `self`.
    fn merge_length_delimited<B>(&mut self, mut buf: B) -> Result<(), DecodeError>
    where
        B: Buf,
        Self: Sized,
    {
        General::decode_value(self, &mut Capped::new(&mut buf), DecodeContext::default())
    }

    /// Clears the message, resetting all fields to their default.
    fn clear(&mut self);

    // TODO(widders): encode and decode with unknown fields in an unknown-fields companion struct
}

pub trait DistinguishedMessage: Message + DistinguishedTaggedDecoder + Eq {
    // TODO(widders): this. and revise the above
}

/// Trait to be implemented by (or more commonly derived for) oneofs and messages, which have
/// knowledge of their fields' tags and encoding.
pub trait TaggedDecoder {
    /// Decodes a field from a buffer into `self`.
    ///
    /// Meant to be used only by `Message` and `Oneof` implementations.
    fn decode_tagged_field<B: Buf>(
        &mut self,
        tag: u32,
        wire_type: WireType,
        duplicated: bool,
        buf: &mut Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError>
    where
        Self: Sized;
}

/// Complementary trait for oneof fields and messages, all of whose fields have a distinguished\
/// encoding.
pub trait DistinguishedTaggedDecoder {
    fn decode_tagged_field_distinguished<B: Buf>(
        &mut self,
        tag: u32,
        wire_type: WireType,
        duplicated: bool,
        buf: &mut Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError>
    where
        Self: Sized;
}

impl<M> TaggedDecoder for Box<M>
where
    M: Message,
{
    fn decode_tagged_field<B: Buf>(
        &mut self,
        tag: u32,
        wire_type: WireType,
        duplicated: bool,
        buf: &mut Capped<B>,
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
    fn encode_raw<B>(&self, buf: &mut B)
    where
        B: BufMut,
    {
        (**self).encode_raw(buf)
    }

    fn encoded_len(&self) -> usize {
        (**self).encoded_len()
    }

    fn clear(&mut self) {
        (**self).clear()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const _MESSAGE_IS_OBJECT_SAFE: Option<&dyn Message> = None;
    // TODO(widders): fix this? do we need it? should we remove the Eq requirement?
    // const _DISTINGUISHED_MESSAGE_IS_OBJECT_SAFE: Option<&dyn DistinguishedMessage> = None;
}
