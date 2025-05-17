use crate::buf::{ReverseBuf, ReverseBuffer};
use crate::encoding::message::{
    borrow_merge, borrow_merge_distinguished, merge, merge_distinguished,
};
use crate::encoding::{
    encode_varint, encoded_len_varint, prepend_varint, Capped, DecodeContext,
    RawDistinguishedMessageBorrowDecoder, RawDistinguishedMessageDecoder, RawMessage,
    RawMessageBorrowDecoder, RawMessageDecoder, RestrictedDecodeContext,
};
use crate::Canonicity::{Canonical, NotCanonical};
use crate::{length_delimiter_len, Canonicity, DecodeError, EncodeError};
use alloc::vec::Vec;
use bytes::{Buf, BufMut, Bytes, BytesMut};

/// A Bilrost message. Provides basic encoding functionality for message types.
pub trait Message {
    /// Creates a new message with an empty state.
    fn new_empty() -> Self
    where
        Self: Sized;

    /// Encodes the message to a buffer.
    ///
    /// An error will be returned if the buffer does not have sufficient capacity.
    fn encode<B: BufMut + ?Sized>(&self, buf: &mut B) -> Result<(), EncodeError>
    where
        Self: Sized;

    /// Prepends the message to a buffer.
    fn prepend<B: ReverseBuf + ?Sized>(&self, buf: &mut B)
    where
        Self: Sized;

    /// Encodes the message with a length-delimiter to a buffer.
    ///
    /// An error will be returned if the buffer does not have sufficient capacity.
    fn encode_length_delimited<B: BufMut + ?Sized>(&self, buf: &mut B) -> Result<(), EncodeError>
    where
        Self: Sized;

    // ------------ Dyn-compatible methods follow ------------

    /// Returns whether the message is currently in an empty state.
    fn message_is_empty(&self) -> bool;

    /// Resets the message to an empty state.
    fn clear_message(&mut self);

    /// Returns the encoded length of the message without a length delimiter.
    fn encoded_len(&self) -> usize;

    /// Encodes the message to a newly allocated buffer.
    fn encode_to_vec(&self) -> Vec<u8>;

    /// Encodes the message to a `Bytes` buffer.
    fn encode_to_bytes(&self) -> Bytes;

    /// Encodes the message to a `ReverseBuffer`.
    fn encode_fast(&self) -> ReverseBuffer;

    /// Encodes the message with a length-delimiter to a `ReverseBuffer`.
    fn encode_length_delimited_fast(&self) -> ReverseBuffer;

    /// Encodes the message to a new `RevserseBuffer` which will have exactly the required capacity
    /// in one contiguous slice.
    fn encode_contiguous(&self) -> ReverseBuffer;

    /// Encodes the message with a length-delimiter to a new `RevserseBuffer` which will have
    /// exactly the required capacity in one contiguous slice.
    fn encode_length_delimited_contiguous(&self) -> ReverseBuffer;

    /// Encodes the message to a `Bytes` buffer.
    fn encode_dyn(&self, buf: &mut dyn BufMut) -> Result<(), EncodeError>;

    /// Encodes the message with a length-delimiter to a newly allocated buffer.
    fn encode_length_delimited_to_vec(&self) -> Vec<u8>;

    /// Encodes the message with a length-delimiter to a `Bytes` buffer.
    fn encode_length_delimited_to_bytes(&self) -> Bytes;

    /// Encodes the message with a length-delimiter to a `Bytes` buffer.
    fn encode_length_delimited_dyn(&self, buf: &mut dyn BufMut) -> Result<(), EncodeError>;
}

/// Basic decoding functionality for a Bilrost message that can decode to an owned form. This
/// trait's decoding methods can decode from any byte buffer that implements `bytes::Buf`.
pub trait OwnedMessage: Message {
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

    // ------------ Dyn-compatible methods follow ------------

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

/// An enhanced trait for owned Bilrost messages that promise a distinguished representation.
///
/// Implementation of this trait comes with the following promises:
///
///  1. The message will always encode to the same bytes as any other message with an equal value.
///  2. A message equal to that value will only ever decode canonically and without error from that
///     exact sequence of bytes, not from any other.
///
/// Distinguished decoding methods come in three flavors:
/// * "distinguished" methods, which decode anything that relaxed decoding will and return the
///   value along with a `Canonicity`
/// * "restricted" methods, which also require a minimum `Canonicity` and will early-exit decoding
///   and return an appropriate error if the canonicity violates that constraint:
///     * restrict to `Canonical` will return an error any time the encoding is not fully canonical
///     * restrict to `HasExtensions` will return an error any time the encoding has known fields
///       with non-canonical representations, but will not fail when unknown fields are present
///     * passing `NotCanonical` gives exactly the same result as using the distinguished decoding
///       methods
/// * "canonical" methods, which are shorthand for "restricted" methods with `Canonical` constraint
///   and do not return the `Canonicity`, because it will always be fully `Canonical`.
///
/// Note that currently the only restriction level that is sensible to *explicitly* pass to
/// "restricted" methods is `HasExtensions`: "distinguished" methods already dispatch to passing
/// `NotCanonical`, and when `Canonical` is passed only `Canonical` can be returned from a
/// successful result (hence the "canonical" methods). It can of course make sense to call these
/// methods with a varying restriction level.
pub trait DistinguishedOwnedMessage: OwnedMessage {
    // ------------ Distinguished mode ------------

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

    // ------------ Dyn-compatible methods follow ------------

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

    // ------------ Restricted mode ------------

    /// Decodes an instance of the message from a buffer in restricted mode.
    ///
    /// The entire buffer will be consumed.
    fn decode_restricted<B: Buf>(
        buf: B,
        restrict_to: Canonicity,
    ) -> Result<(Self, Canonicity), DecodeError>
    where
        Self: Sized;

    /// Decodes a length-delimited instance of the message from the buffer in restricted mode.
    fn decode_restricted_length_delimited<B: Buf>(
        buf: B,
        restrict_to: Canonicity,
    ) -> Result<(Self, Canonicity), DecodeError>
    where
        Self: Sized;

    /// Decodes an instance from the given `Capped` buffer in restricted mode, consuming it to
    /// its cap.
    #[doc(hidden)]
    fn decode_restricted_capped<B: Buf + ?Sized>(
        buf: Capped<B>,
        restrict_to: Canonicity,
    ) -> Result<(Self, Canonicity), DecodeError>
    where
        Self: Sized;

    /// Decodes the non-ignored fields of this message from the buffer in restricted mode,
    /// replacing their values.
    fn replace_restricted_from<B: Buf>(
        &mut self,
        buf: B,
        restrict_to: Canonicity,
    ) -> Result<Canonicity, DecodeError>
    where
        Self: Sized;

    /// Decodes the non-ignored fields of this message in restricted mode, replacing their values
    /// from a length-delimited value encoded in the buffer.
    fn replace_restricted_from_length_delimited<B: Buf>(
        &mut self,
        buf: B,
        restrict_to: Canonicity,
    ) -> Result<Canonicity, DecodeError>
    where
        Self: Sized;

    /// Decodes the non-ignored fields of this message in restricted mode, replacing their values
    /// from the given capped buffer.
    #[doc(hidden)]
    fn replace_restricted_from_capped<B: Buf + ?Sized>(
        &mut self,
        buf: Capped<B>,
        restrict_to: Canonicity,
    ) -> Result<Canonicity, DecodeError>
    where
        Self: Sized;

    // ------------ Dyn-compatible methods follow ------------

    /// Decodes a length-delimited instance of the message from the buffer in restricted mode.
    fn replace_restricted_from_slice(
        &mut self,
        buf: &[u8],
        restrict_to: Canonicity,
    ) -> Result<Canonicity, DecodeError>;

    /// Decodes the non-ignored fields of this message, replacing their values from a
    /// length-delimited value encoded in the buffer in restricted mode.
    fn replace_restricted_from_dyn(
        &mut self,
        buf: &mut dyn Buf,
        restrict_to: Canonicity,
    ) -> Result<Canonicity, DecodeError>;

    /// Decodes the non-ignored fields of this message from the buffer in restricted mode,
    /// replacing their values.
    fn replace_restricted_from_length_delimited_slice(
        &mut self,
        buf: &[u8],
        restrict_to: Canonicity,
    ) -> Result<Canonicity, DecodeError>;

    /// Decodes the non-ignored fields of this message, replacing their values from a
    /// length-delimited value encoded in the buffer in restricted mode.
    fn replace_restricted_from_length_delimited_dyn(
        &mut self,
        buf: &mut dyn Buf,
        restrict_to: Canonicity,
    ) -> Result<Canonicity, DecodeError>;

    /// Decodes the non-ignored fields of this message, replacing their values from the given capped
    /// buffer in restricted mode.
    #[doc(hidden)]
    fn replace_restricted_from_capped_dyn(
        &mut self,
        buf: Capped<dyn Buf>,
        restrict_to: Canonicity,
    ) -> Result<Canonicity, DecodeError>;

    // ------------ Canonical mode ------------

    /// Decodes an instance of the message from a buffer in canonical mode.
    ///
    /// The entire buffer will be consumed.
    fn decode_canonical<B: Buf>(buf: B) -> Result<Self, DecodeError>
    where
        Self: Sized;

    /// Decodes a length-delimited instance of the message from the buffer in canonical mode.
    fn decode_canonical_length_delimited<B: Buf>(buf: B) -> Result<Self, DecodeError>
    where
        Self: Sized;

    /// Decodes an instance from the given `Capped` buffer in canonical mode, consuming it to
    /// its cap.
    #[doc(hidden)]
    fn decode_canonical_capped<B: Buf + ?Sized>(buf: Capped<B>) -> Result<Self, DecodeError>
    where
        Self: Sized;

    /// Decodes the non-ignored fields of this message from the buffer in canonical mode,
    /// replacing their values.
    fn replace_canonical_from<B: Buf>(&mut self, buf: B) -> Result<(), DecodeError>
    where
        Self: Sized;

    /// Decodes the non-ignored fields of this message in canonical mode, replacing their values
    /// from a length-delimited value encoded in the buffer.
    fn replace_canonical_from_length_delimited<B: Buf>(
        &mut self,
        buf: B,
    ) -> Result<(), DecodeError>
    where
        Self: Sized;

    /// Decodes the non-ignored fields of this message in canonical mode, replacing their values
    /// from the given capped buffer.
    #[doc(hidden)]
    fn replace_canonical_from_capped<B: Buf + ?Sized>(
        &mut self,
        buf: Capped<B>,
    ) -> Result<(), DecodeError>
    where
        Self: Sized;

    // ------------ Dyn-compatible methods follow ------------

    /// Decodes a length-delimited instance of the message from the buffer in canonical mode.
    fn replace_canonical_from_slice(&mut self, buf: &[u8]) -> Result<(), DecodeError>;

    /// Decodes the non-ignored fields of this message, replacing their values from a
    /// length-delimited value encoded in the buffer in canonical mode.
    fn replace_canonical_from_dyn(&mut self, buf: &mut dyn Buf) -> Result<(), DecodeError>;

    /// Decodes the non-ignored fields of this message from the buffer in canonical mode,
    /// replacing their values.
    fn replace_canonical_from_length_delimited_slice(
        &mut self,
        buf: &[u8],
    ) -> Result<(), DecodeError>;

    /// Decodes the non-ignored fields of this message, replacing their values from a
    /// length-delimited value encoded in the buffer in canonical mode.
    fn replace_canonical_from_length_delimited_dyn(
        &mut self,
        buf: &mut dyn Buf,
    ) -> Result<(), DecodeError>;

    /// Decodes the non-ignored fields of this message, replacing their values from the given capped
    /// buffer in canonical mode.
    #[doc(hidden)]
    fn replace_canonical_from_capped_dyn(
        &mut self,
        buf: Capped<dyn Buf>,
    ) -> Result<(), DecodeError>;
}

/// Basic decoding functionality for a Bilrost message that can decode from a borrowed slice.
pub trait BorrowedMessage<'a>: Message {
    /// Decodes an instance of the message from a buffer.
    ///
    /// The entire buffer will be consumed.
    fn decode_borrowed(buf: &'a [u8]) -> Result<Self, DecodeError>
    where
        Self: Sized;

    /// Decodes a length-delimited instance of the message from the buffer.
    ///
    /// * If the message decodes successfully, the provided slice will be shortened to no longer
    ///   include the bytes that encoded it or its length delimiter.
    /// * If the message is correctly delimited within the bounds of the slice but fails to decode,
    ///   the provided slice will still be shortened even though an error is returned.
    /// * If the slice is shorter than the length delimiter indicates, or if the length delimiter
    ///   itself is truncated, an error with Truncated kind is returned and it is unspecified how
    ///   the provided slice value is modified.
    fn decode_borrowed_length_delimited(buf: &mut &'a [u8]) -> Result<Self, DecodeError>
    where
        Self: Sized;

    // ------------ Dyn-compatible methods follow ------------

    /// Decodes the non-ignored fields of this message from the buffer, replacing their values.
    fn replace_borrowed_from(&mut self, buf: &'a [u8]) -> Result<(), DecodeError>;

    /// Decodes the non-ignored fields of this message, replacing their values from a
    /// length-delimited value encoded in the buffer.
    ///
    /// * If the message decodes successfully, the provided slice will be shortened to no longer
    ///   include the bytes that encoded it or its length delimiter.
    /// * If the message is correctly delimited within the bounds of the slice but fails to decode,
    ///   the provided slice will still be shortened even though an error is returned.
    /// * If the slice is shorter than the length delimiter indicates, or if the length delimiter
    ///   itself is truncated, an error with Truncated kind is returned and it is unspecified how
    ///   the provided slice value is modified.
    fn replace_borrowed_from_length_delimited(
        &mut self,
        buf: &mut &'a [u8],
    ) -> Result<(), DecodeError>;
}

/// An enhanced trait for borrowed Bilrost messages that promise a distinguished representation.
///
/// Implementation of this trait comes with the following promises:
///
///  1. The message will always encode to the same bytes as any other message with an equal value.
///  2. A message equal to that value will only ever decode canonically and without error from that
///     exact sequence of bytes, not from any other.
///
/// Distinguished decoding methods come in three flavors:
/// * "distinguished" methods, which decode anything that relaxed decoding will and return the
///   value along with a `Canonicity`
/// * "restricted" methods, which also require a minimum `Canonicity` and will early-exit decoding
///   and return an appropriate error if the canonicity violates that constraint:
///     * restrict to `Canonical` will return an error any time the encoding is not fully canonical
///     * restrict to `HasExtensions` will return an error any time the encoding has known fields
///       with non-canonical representations, but will not fail when unknown fields are present
///     * passing `NotCanonical` gives exactly the same result as using the distinguished decoding
///       methods
/// * "canonical" methods, which are shorthand for "restricted" methods with `Canonical` constraint
///   and do not return the `Canonicity`, because it will always be fully `Canonical`.
///
/// Note that currently the only restriction level that is sensible to *explicitly* pass to
/// "restricted" methods is `HasExtensions`: "distinguished" methods already dispatch to passing
/// `NotCanonical`, and when `Canonical` is passed only `Canonical` can be returned from a
/// successful result (hence the "canonical" methods). It can of course make sense to call these
/// methods with a varying restriction level.
pub trait DistinguishedBorrowedMessage<'a>: BorrowedMessage<'a> {
    // ------------ Distinguished mode ------------

    /// Decodes an instance of the message from a buffer in distinguished mode.
    ///
    /// The entire buffer will be consumed.
    fn decode_distinguished_borrowed(buf: &'a [u8]) -> Result<(Self, Canonicity), DecodeError>
    where
        Self: Sized;

    /// Decodes a length-delimited instance of the message from the buffer in distinguished mode.
    ///
    /// * If the message decodes successfully, the provided slice will be shortened to no longer
    ///   include the bytes that encoded it or its length delimiter.
    /// * If the message is correctly delimited within the bounds of the slice but fails to decode,
    ///   the provided slice will still be shortened even though an error is returned.
    /// * If the slice is shorter than the length delimiter indicates, or if the length delimiter
    ///   itself is truncated, an error with Truncated kind is returned and it is unspecified how
    ///   the provided slice value is modified.
    fn decode_distinguished_borrowed_length_delimited(
        buf: &mut &'a [u8],
    ) -> Result<(Self, Canonicity), DecodeError>
    where
        Self: Sized;

    // ------------ Dyn-compatible methods follow ------------

    /// Decodes the non-ignored fields of this message from the buffer in distinguished mode,
    /// replacing their values.
    fn replace_distinguished_borrowed_from(
        &mut self,
        buf: &'a [u8],
    ) -> Result<Canonicity, DecodeError>;

    /// Decodes the non-ignored fields of this message in distinguished mode, replacing their values
    /// from a length-delimited value encoded in the buffer.
    ///
    /// * If the message decodes successfully, the provided slice will be shortened to no longer
    ///   include the bytes that encoded it or its length delimiter.
    /// * If the message is correctly delimited within the bounds of the slice but fails to decode,
    ///   the provided slice will still be shortened even though an error is returned.
    /// * If the slice is shorter than the length delimiter indicates, or if the length delimiter
    ///   itself is truncated, an error with Truncated kind is returned and it is unspecified how
    ///   the provided slice value is modified.
    fn replace_distinguished_borrowed_from_length_delimited(
        &mut self,
        buf: &mut &'a [u8],
    ) -> Result<Canonicity, DecodeError>;

    // ------------ Restricted mode ------------

    /// Decodes an instance of the message from a buffer in restricted mode.
    ///
    /// The entire buffer will be consumed.
    fn decode_restricted_borrowed(
        buf: &'a [u8],
        restrict_to: Canonicity,
    ) -> Result<(Self, Canonicity), DecodeError>
    where
        Self: Sized;

    /// Decodes a length-delimited instance of the message from the buffer in restricted mode.
    ///
    /// * If the message decodes successfully, the provided slice will be shortened to no longer
    ///   include the bytes that encoded it or its length delimiter.
    /// * If the message is correctly delimited within the bounds of the slice but fails to decode,
    ///   the provided slice will still be shortened even though an error is returned.
    /// * If the slice is shorter than the length delimiter indicates, or if the length delimiter
    ///   itself is truncated, an error with Truncated kind is returned and it is unspecified how
    ///   the provided slice value is modified.
    fn decode_restricted_borrowed_length_delimited(
        buf: &mut &'a [u8],
        restrict_to: Canonicity,
    ) -> Result<(Self, Canonicity), DecodeError>
    where
        Self: Sized;

    // ------------ Dyn-compatible methods follow ------------

    /// Decodes the non-ignored fields of this message from the buffer in restricted mode,
    /// replacing their values.
    fn replace_restricted_borrowed_from(
        &mut self,
        buf: &'a [u8],
        restrict_to: Canonicity,
    ) -> Result<Canonicity, DecodeError>;

    /// Decodes the non-ignored fields of this message in restricted mode, replacing their values
    /// from a length-delimited value encoded in the buffer.
    ///
    /// * If the message decodes successfully, the provided slice will be shortened to no longer
    ///   include the bytes that encoded it or its length delimiter.
    /// * If the message is correctly delimited within the bounds of the slice but fails to decode,
    ///   the provided slice will still be shortened even though an error is returned.
    /// * If the slice is shorter than the length delimiter indicates, or if the length delimiter
    ///   itself is truncated, an error with Truncated kind is returned and it is unspecified how
    ///   the provided slice value is modified.
    fn replace_restricted_borrowed_from_length_delimited(
        &mut self,
        buf: &mut &'a [u8],
        restrict_to: Canonicity,
    ) -> Result<Canonicity, DecodeError>;

    // ------------ Canonical mode ------------

    /// Decodes an instance of the message from a buffer in canonical mode.
    ///
    /// The entire buffer will be consumed.
    fn decode_canonical_borrowed(buf: &'a [u8]) -> Result<Self, DecodeError>
    where
        Self: Sized;

    /// Decodes a length-delimited instance of the message from the buffer in canonical mode.
    ///
    /// * If the message decodes successfully, the provided slice will be shortened to no longer
    ///   include the bytes that encoded it or its length delimiter.
    /// * If the message is correctly delimited within the bounds of the slice but fails to decode,
    ///   the provided slice will still be shortened even though an error is returned.
    /// * If the slice is shorter than the length delimiter indicates, or if the length delimiter
    ///   itself is truncated, an error with Truncated kind is returned and it is unspecified how
    ///   the provided slice value is modified.
    fn decode_canonical_borrowed_length_delimited(buf: &mut &'a [u8]) -> Result<Self, DecodeError>
    where
        Self: Sized;

    // ------------ Dyn-compatible methods follow ------------

    /// Decodes the non-ignored fields of this message from the buffer in canonical mode,
    /// replacing their values.
    fn replace_canonical_borrowed_from(&mut self, buf: &'a [u8]) -> Result<(), DecodeError>;

    /// Decodes the non-ignored fields of this message in canonical mode, replacing their values
    /// from a length-delimited value encoded in the buffer.
    ///
    /// * If the message decodes successfully, the provided slice will be shortened to no longer
    ///   include the bytes that encoded it or its length delimiter.
    /// * If the message is correctly delimited within the bounds of the slice but fails to decode,
    ///   the provided slice will still be shortened even though an error is returned.
    /// * If the slice is shorter than the length delimiter indicates, or if the length delimiter
    ///   itself is truncated, an error with Truncated kind is returned and it is unspecified how
    ///   the provided slice value is modified.
    fn replace_canonical_borrowed_from_length_delimited(
        &mut self,
        buf: &mut &'a [u8],
    ) -> Result<(), DecodeError>;
}

/// `Message` is implemented as a usability layer on top of the basic functionality afforded by
/// `RawMessage`.
// TODO(widders): in the future, make it possible to decode with extension Message types for all
//  fields not covered by the own type. The default extension can be `()`, which always skips in
//  relaxed mode and always errs in distinguished mode; the most permissive possible extension
//  would then be OpaqueMessage, which losslessly captures all unknown fields. A composing wrapper
//  type that combines two message types in an overlay can be implemented. This will require an
//  alternate encoding mode which emits field groups to be sorted in a stricter way, only grouping
//  truly contiguous runs of field ids so that they can be sorted with any other type's fields at
//  runtime.
impl<T> Message for T
where
    T: RawMessage + Sized,
{
    fn new_empty() -> Self {
        T::empty()
    }

    fn encode<B: BufMut + ?Sized>(&self, buf: &mut B) -> Result<(), EncodeError> {
        let required = self.encoded_len();
        let remaining = buf.remaining_mut();
        if required > remaining {
            return Err(EncodeError::new(required, remaining));
        }

        self.raw_encode(buf);
        Ok(())
    }

    fn prepend<B: ReverseBuf + ?Sized>(&self, buf: &mut B) {
        self.raw_prepend(buf);
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

    fn message_is_empty(&self) -> bool {
        self.is_empty()
    }

    fn clear_message(&mut self) {
        self.clear();
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

    fn encode_fast(&self) -> ReverseBuffer {
        let mut buf = ReverseBuffer::new();
        self.raw_prepend(&mut buf);
        buf
    }

    fn encode_length_delimited_fast(&self) -> ReverseBuffer {
        let mut buf = self.encode_fast();
        prepend_varint(buf.remaining() as u64, &mut buf);
        buf
    }

    fn encode_contiguous(&self) -> ReverseBuffer {
        let mut buf = ReverseBuffer::with_capacity(self.encoded_len());
        self.raw_prepend(&mut buf);
        debug_assert!(buf.contiguous().is_some());
        debug_assert!(buf.capacity() == buf.len());
        buf
    }

    fn encode_length_delimited_contiguous(&self) -> ReverseBuffer {
        let len = self.encoded_len();
        let mut buf = ReverseBuffer::with_capacity(len + length_delimiter_len(len));
        self.raw_prepend(&mut buf);
        prepend_varint(len as u64, &mut buf);
        debug_assert!(buf.contiguous().is_some());
        debug_assert!(buf.capacity() == buf.len());
        buf
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
}

impl<T> OwnedMessage for T
where
    T: RawMessageDecoder + Sized,
{
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
        // MSRV: here, and elsewhere, this `map_err` could be `inspect_err` (1.76)
        merge(self, buf, DecodeContext::default()).map_err(|err| {
            self.clear();
            err
        })
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

impl<T> DistinguishedOwnedMessage for T
where
    T: RawDistinguishedMessageDecoder + RawMessageDecoder,
{
    fn decode_distinguished<B: Buf>(buf: B) -> Result<(Self, Canonicity), DecodeError> {
        Self::decode_restricted(buf, NotCanonical)
    }

    fn decode_distinguished_length_delimited<B: Buf>(
        buf: B,
    ) -> Result<(Self, Canonicity), DecodeError> {
        Self::decode_restricted_length_delimited(buf, NotCanonical)
    }

    #[doc(hidden)]
    fn decode_distinguished_capped<B: Buf + ?Sized>(
        buf: Capped<B>,
    ) -> Result<(Self, Canonicity), DecodeError> {
        Self::decode_restricted_capped(buf, NotCanonical)
    }

    fn replace_distinguished_from<B: Buf>(&mut self, buf: B) -> Result<Canonicity, DecodeError> {
        self.replace_restricted_from(buf, NotCanonical)
    }

    fn replace_distinguished_from_length_delimited<B: Buf>(
        &mut self,
        buf: B,
    ) -> Result<Canonicity, DecodeError> {
        self.replace_restricted_from_length_delimited(buf, NotCanonical)
    }

    #[doc(hidden)]
    fn replace_distinguished_from_capped<B: Buf + ?Sized>(
        &mut self,
        buf: Capped<B>,
    ) -> Result<Canonicity, DecodeError> {
        self.replace_restricted_from_capped(buf, NotCanonical)
    }

    fn replace_distinguished_from_slice(&mut self, buf: &[u8]) -> Result<Canonicity, DecodeError> {
        self.replace_restricted_from(buf, NotCanonical)
    }

    fn replace_distinguished_from_dyn(
        &mut self,
        buf: &mut dyn Buf,
    ) -> Result<Canonicity, DecodeError> {
        self.replace_restricted_from(buf, NotCanonical)
    }

    fn replace_distinguished_from_length_delimited_slice(
        &mut self,
        buf: &[u8],
    ) -> Result<Canonicity, DecodeError> {
        self.replace_restricted_from_length_delimited(buf, NotCanonical)
    }

    fn replace_distinguished_from_length_delimited_dyn(
        &mut self,
        buf: &mut dyn Buf,
    ) -> Result<Canonicity, DecodeError> {
        self.replace_restricted_from_length_delimited(buf, NotCanonical)
    }

    #[doc(hidden)]
    fn replace_distinguished_from_capped_dyn(
        &mut self,
        buf: Capped<dyn Buf>,
    ) -> Result<Canonicity, DecodeError> {
        self.replace_restricted_from_capped(buf, NotCanonical)
    }

    fn decode_restricted<B: Buf>(
        mut buf: B,
        restrict_to: Canonicity,
    ) -> Result<(Self, Canonicity), DecodeError> {
        Self::decode_restricted_capped(Capped::new(&mut buf), restrict_to)
    }

    fn decode_restricted_length_delimited<B: Buf>(
        mut buf: B,
        restrict_to: Canonicity,
    ) -> Result<(Self, Canonicity), DecodeError> {
        Self::decode_restricted_capped(Capped::new_length_delimited(&mut buf)?, restrict_to)
    }

    fn decode_restricted_capped<B: Buf + ?Sized>(
        buf: Capped<B>,
        restrict_to: Canonicity,
    ) -> Result<(Self, Canonicity), DecodeError> {
        let mut message = Self::empty();
        let ctx = RestrictedDecodeContext::new(restrict_to);
        let canon = merge_distinguished(&mut message, buf, ctx.clone())
            // Safety backstop to ensure we do not return a canonicity worse than restrict_to.
            // See the docs on `RestrictedDecodeContext::check` for details on canonicity
            // checking.
            .and_then(|canon| ctx.check(canon))?;
        Ok((message, canon))
    }

    fn replace_restricted_from<B: Buf>(
        &mut self,
        mut buf: B,
        restrict_to: Canonicity,
    ) -> Result<Canonicity, DecodeError> {
        self.replace_restricted_from_capped(Capped::new(&mut buf), restrict_to)
    }

    fn replace_restricted_from_length_delimited<B: Buf>(
        &mut self,
        mut buf: B,
        restrict_to: Canonicity,
    ) -> Result<Canonicity, DecodeError> {
        self.replace_restricted_from_capped(Capped::new_length_delimited(&mut buf)?, restrict_to)
    }

    fn replace_restricted_from_capped<B: Buf + ?Sized>(
        &mut self,
        buf: Capped<B>,
        restrict_to: Canonicity,
    ) -> Result<Canonicity, DecodeError> {
        self.clear();
        let ctx = RestrictedDecodeContext::new(restrict_to);
        merge_distinguished(self, buf, ctx.clone())
            .map_err(|err| {
                self.clear();
                err
            })
            // Safety backstop to ensure we do not return a canonicity worse than restrict_to.
            // See the docs on `RestrictedDecodeContext::check` for details on canonicity
            // checking.
            .and_then(|canon| ctx.check(canon))
    }

    fn replace_restricted_from_slice(
        &mut self,
        buf: &[u8],
        restrict_to: Canonicity,
    ) -> Result<Canonicity, DecodeError> {
        self.replace_restricted_from(buf, restrict_to)
    }

    fn replace_restricted_from_dyn(
        &mut self,
        buf: &mut dyn Buf,
        restrict_to: Canonicity,
    ) -> Result<Canonicity, DecodeError> {
        self.replace_restricted_from(buf, restrict_to)
    }

    fn replace_restricted_from_length_delimited_slice(
        &mut self,
        buf: &[u8],
        restrict_to: Canonicity,
    ) -> Result<Canonicity, DecodeError> {
        self.replace_restricted_from_length_delimited(buf, restrict_to)
    }

    fn replace_restricted_from_length_delimited_dyn(
        &mut self,
        buf: &mut dyn Buf,
        restrict_to: Canonicity,
    ) -> Result<Canonicity, DecodeError> {
        self.replace_restricted_from_length_delimited(buf, restrict_to)
    }

    fn replace_restricted_from_capped_dyn(
        &mut self,
        buf: Capped<dyn Buf>,
        restrict_to: Canonicity,
    ) -> Result<Canonicity, DecodeError> {
        self.replace_restricted_from_capped(buf, restrict_to)
    }

    fn decode_canonical<B: Buf>(buf: B) -> Result<Self, DecodeError> {
        Self::decode_restricted(buf, Canonical).map(|(val, _)| val)
    }

    fn decode_canonical_length_delimited<B: Buf>(buf: B) -> Result<Self, DecodeError> {
        Self::decode_restricted_length_delimited(buf, Canonical).map(|(val, _)| val)
    }

    #[doc(hidden)]
    fn decode_canonical_capped<B: Buf + ?Sized>(buf: Capped<B>) -> Result<Self, DecodeError> {
        Self::decode_restricted_capped(buf, Canonical).map(|(val, _)| val)
    }

    fn replace_canonical_from<B: Buf>(&mut self, buf: B) -> Result<(), DecodeError> {
        self.replace_restricted_from(buf, Canonical).map(|_| ())
    }

    fn replace_canonical_from_length_delimited<B: Buf>(
        &mut self,
        buf: B,
    ) -> Result<(), DecodeError> {
        self.replace_restricted_from_length_delimited(buf, Canonical)
            .map(|_| ())
    }

    #[doc(hidden)]
    fn replace_canonical_from_capped<B: Buf + ?Sized>(
        &mut self,
        buf: Capped<B>,
    ) -> Result<(), DecodeError> {
        self.replace_restricted_from_capped(buf, Canonical)
            .map(|_| ())
    }

    fn replace_canonical_from_slice(&mut self, buf: &[u8]) -> Result<(), DecodeError> {
        self.replace_restricted_from(buf, Canonical).map(|_| ())
    }

    fn replace_canonical_from_dyn(&mut self, buf: &mut dyn Buf) -> Result<(), DecodeError> {
        self.replace_restricted_from(buf, Canonical).map(|_| ())
    }

    fn replace_canonical_from_length_delimited_slice(
        &mut self,
        buf: &[u8],
    ) -> Result<(), DecodeError> {
        self.replace_restricted_from_length_delimited(buf, Canonical)
            .map(|_| ())
    }

    fn replace_canonical_from_length_delimited_dyn(
        &mut self,
        buf: &mut dyn Buf,
    ) -> Result<(), DecodeError> {
        self.replace_restricted_from_length_delimited(buf, Canonical)
            .map(|_| ())
    }

    #[doc(hidden)]
    fn replace_canonical_from_capped_dyn(
        &mut self,
        buf: Capped<dyn Buf>,
    ) -> Result<(), DecodeError> {
        self.replace_restricted_from_capped(buf, Canonical)
            .map(|_| ())
    }
}

impl<'a, T> BorrowedMessage<'a> for T
where
    T: RawMessageBorrowDecoder<'a> + Sized,
{
    fn decode_borrowed(mut buf: &'a [u8]) -> Result<Self, DecodeError> {
        let mut message = Self::empty();
        borrow_merge(
            &mut message,
            Capped::new(&mut buf),
            DecodeContext::default(),
        )?;
        Ok(message)
    }

    fn decode_borrowed_length_delimited(buf: &mut &'a [u8]) -> Result<Self, DecodeError> {
        Self::decode_borrowed(Capped::new(buf).take_borrowed_length_delimited()?)
    }

    fn replace_borrowed_from(&mut self, mut buf: &'a [u8]) -> Result<(), DecodeError> {
        self.clear();
        borrow_merge(self, Capped::new(&mut buf), DecodeContext::default()).map_err(|err| {
            self.clear();
            err
        })
    }

    fn replace_borrowed_from_length_delimited(
        &mut self,
        buf: &mut &'a [u8],
    ) -> Result<(), DecodeError> {
        self.replace_borrowed_from(Capped::new(buf).take_borrowed_length_delimited()?)
    }
}

impl<'a, T> DistinguishedBorrowedMessage<'a> for T
where
    T: RawDistinguishedMessageBorrowDecoder<'a> + RawMessageBorrowDecoder<'a>,
{
    fn decode_distinguished_borrowed(buf: &'a [u8]) -> Result<(Self, Canonicity), DecodeError> {
        Self::decode_restricted_borrowed(buf, NotCanonical)
    }

    fn decode_distinguished_borrowed_length_delimited(
        buf: &mut &'a [u8],
    ) -> Result<(Self, Canonicity), DecodeError> {
        Self::decode_restricted_borrowed_length_delimited(buf, NotCanonical)
    }

    fn replace_distinguished_borrowed_from(
        &mut self,
        buf: &'a [u8],
    ) -> Result<Canonicity, DecodeError> {
        self.replace_restricted_borrowed_from(buf, NotCanonical)
    }

    fn replace_distinguished_borrowed_from_length_delimited(
        &mut self,
        buf: &mut &'a [u8],
    ) -> Result<Canonicity, DecodeError> {
        self.replace_restricted_borrowed_from_length_delimited(buf, NotCanonical)
    }

    fn decode_restricted_borrowed(
        mut buf: &'a [u8],
        restrict_to: Canonicity,
    ) -> Result<(Self, Canonicity), DecodeError> {
        let mut message = Self::empty();
        let ctx = RestrictedDecodeContext::new(restrict_to);
        let canon = borrow_merge_distinguished(&mut message, Capped::new(&mut buf), ctx.clone())
            // Safety backstop to ensure we do not return a canonicity worse than restrict_to.
            // See the docs on `RestrictedDecodeContext::check` for details on canonicity
            // checking.
            .and_then(|canon| ctx.check(canon))?;
        Ok((message, canon))
    }

    fn decode_restricted_borrowed_length_delimited(
        buf: &mut &'a [u8],
        restrict_to: Canonicity,
    ) -> Result<(Self, Canonicity), DecodeError> {
        Self::decode_restricted_borrowed(
            Capped::new(buf).take_borrowed_length_delimited()?,
            restrict_to,
        )
    }

    fn replace_restricted_borrowed_from(
        &mut self,
        mut buf: &'a [u8],
        restrict_to: Canonicity,
    ) -> Result<Canonicity, DecodeError> {
        self.clear();
        let ctx = RestrictedDecodeContext::new(restrict_to);
        borrow_merge_distinguished(self, Capped::new(&mut buf), ctx.clone())
            .map_err(|err| {
                self.clear();
                err
            })
            // Safety backstop to ensure we do not return a canonicity worse than restrict_to.
            // See the docs on `RestrictedDecodeContext::check` for details on canonicity
            // checking.
            .and_then(|canon| ctx.check(canon))
    }

    fn replace_restricted_borrowed_from_length_delimited(
        &mut self,
        buf: &mut &'a [u8],
        restrict_to: Canonicity,
    ) -> Result<Canonicity, DecodeError> {
        self.replace_restricted_borrowed_from(
            Capped::new(buf).take_borrowed_length_delimited()?,
            restrict_to,
        )
    }

    fn decode_canonical_borrowed(buf: &'a [u8]) -> Result<Self, DecodeError> {
        Self::decode_restricted_borrowed(buf, Canonical).map(|(val, _)| val)
    }

    fn decode_canonical_borrowed_length_delimited(buf: &mut &'a [u8]) -> Result<Self, DecodeError> {
        Self::decode_restricted_borrowed_length_delimited(buf, Canonical).map(|(val, _)| val)
    }

    fn replace_canonical_borrowed_from(&mut self, buf: &'a [u8]) -> Result<(), DecodeError> {
        self.replace_restricted_borrowed_from(buf, Canonical)
            .map(|_| ())
    }

    fn replace_canonical_borrowed_from_length_delimited(
        &mut self,
        buf: &mut &'a [u8],
    ) -> Result<(), DecodeError> {
        self.replace_restricted_borrowed_from_length_delimited(buf, Canonical)
            .map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BorrowedMessage, DistinguishedBorrowedMessage, DistinguishedOwnedMessage, Message,
        OwnedMessage,
    };
    use alloc::vec::Vec;

    const _MESSAGE_IS_DYN_COMPATIBLE: Option<&dyn Message> = None;
    const _OWNED_MESSAGE_IS_DYN_COMPATIBLE: Option<&dyn OwnedMessage> = None;
    const _DISTINGUISHED_OWNED_MESSAGE_IS_DYN_COMPATIBLE: Option<&dyn DistinguishedOwnedMessage> =
        None;
    const _BORROWED_MESSAGE_IS_DYN_COMPATIBLE: Option<&dyn BorrowedMessage<'static>> = None;
    const _DISTINGUISHED_BORROWED_MESSAGE_IS_DYN_COMPATIBLE: Option<
        &dyn DistinguishedBorrowedMessage<'static>,
    > = None;

    fn use_dyn_owned_messages<M: DistinguishedOwnedMessage>(
        safe: &mut dyn DistinguishedOwnedMessage,
        mut msg: M,
    ) {
        let mut vec = Vec::<u8>::new();

        safe.encoded_len();
        safe.encode_dyn(&mut vec).unwrap();
        assert_eq!(vec, safe.encode_to_vec());
        assert_eq!(vec, safe.encode_contiguous().into_vec());
        safe.replace_from_length_delimited_dyn(&mut [0u8].as_slice())
            .unwrap();
        assert!(safe.message_is_empty());
        safe.replace_canonical_from_length_delimited_dyn(&mut [0u8].as_slice())
            .unwrap();
        assert!(safe.message_is_empty());
        safe.replace_from_slice(&[]).unwrap();
        assert!(safe.message_is_empty());
        safe.replace_canonical_from_slice(&[]).unwrap();
        assert!(safe.message_is_empty());

        msg.encoded_len();
        msg = M::decode_length_delimited(&mut [0u8].as_slice()).unwrap();
        msg.encode(&mut vec).unwrap();
        msg.clear_message();
    }

    fn use_dyn_borrowed_messages<'a, M: DistinguishedBorrowedMessage<'a>>(
        safe: &mut dyn DistinguishedBorrowedMessage<'a>,
        mut msg: M,
    ) {
        let mut vec = Vec::<u8>::new();

        safe.encoded_len();
        safe.encode_dyn(&mut vec).unwrap();
        assert_eq!(vec, safe.encode_to_vec());
        safe.replace_borrowed_from_length_delimited(&mut [0u8].as_slice())
            .unwrap();
        assert!(safe.message_is_empty());
        safe.replace_canonical_borrowed_from_length_delimited(&mut [0u8].as_slice())
            .unwrap();
        assert!(safe.message_is_empty());

        msg.encoded_len();
        msg = M::decode_borrowed_length_delimited(&mut [0u8].as_slice()).unwrap();
        msg.encode(&mut vec).unwrap();
        msg.clear_message();
    }

    #[test]
    fn using_dyn_messages() {
        let mut vec = Vec::<u8>::new();
        use_dyn_owned_messages(&mut (), ());
        use_dyn_borrowed_messages(&mut (), ());
        assert_eq!(().encoded_len(), 0);
        ().encode(&mut vec).unwrap();
        ().encode_dyn(&mut vec).unwrap();
        <()>::decode(&mut [].as_slice()).unwrap();
    }
}
