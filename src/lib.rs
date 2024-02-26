#![no_std]
#![doc(html_root_url = "https://docs.rs/bilrost/0.1004.0-dev")]
#![doc = include_str!("../README.md")]

extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

/// Re-export of the bytes crate for use within derived code.
pub use bytes;

#[cfg(feature = "derive")]
#[doc(hidden)]
pub use bilrost_derive::{DistinguishedMessage, DistinguishedOneof, Enumeration, Message, Oneof};

mod error;
mod message;
mod types;

#[doc(hidden)]
pub mod encoding;

pub use crate::encoding::{Canonicity, Enumeration, WithCanonicity};
pub use crate::error::{DecodeError, DecodeErrorKind, EncodeError};
pub use crate::message::{DistinguishedMessage, Message};
#[doc(hidden)]
pub use crate::message::{RawDistinguishedMessage, RawMessage};

pub use types::Blob;

use bytes::{Buf, BufMut};
#[cfg(feature = "extended-diagnostics")]
use const_panic::concat_panic;

use crate::encoding::{decode_varint, encode_varint, encoded_len_varint};

// See `encoding::DecodeContext` for more info.
// 100 is the default recursion limit in the C++ implementation.
#[cfg(not(feature = "no-recursion-limit"))]
const RECURSION_LIMIT: u32 = 100;

/// Encodes a length delimiter to the buffer.
///
/// See [Message.encode_length_delimited] for more info.
///
/// An error will be returned if the buffer does not have sufficient capacity to encode the
/// delimiter.
#[inline]
pub fn encode_length_delimiter<B>(length: usize, buf: &mut B) -> Result<(), EncodeError>
where
    B: BufMut,
{
    let length = length as u64;
    let required = encoded_len_varint(length);
    let remaining = buf.remaining_mut();
    if required > remaining {
        return Err(EncodeError::new(required, remaining));
    }
    encode_varint(length, buf);
    Ok(())
}

/// Returns the encoded length of a length delimiter.
///
/// Applications may use this method to ensure sufficient buffer capacity before calling
/// `encode_length_delimiter`. The returned size will be between 1 and 9, inclusive.
#[inline(always)]
pub fn length_delimiter_len(length: usize) -> usize {
    encoded_len_varint(length as u64)
}

/// Decodes a length delimiter from the buffer.
///
/// This method allows the length delimiter to be decoded independently of the message, when the
/// message is encoded with [Message.encode_length_delimited].
///
/// An error may be returned in two cases:
///
///  * If the supplied buffer contains fewer than 9 bytes, then an error indicates that more
///    input is required to decode the full delimiter.
///  * If the supplied buffer contains 9 or more bytes, then the buffer contains an invalid
///    delimiter, and typically the buffer should be considered corrupt.
#[inline(always)]
pub fn decode_length_delimiter<B: Buf>(mut buf: B) -> Result<usize, DecodeError> {
    decode_varint(&mut buf)?
        .try_into()
        .map_err(|_| DecodeError::new(DecodeErrorKind::Oversize))
}

/// Helper function for derived types, asserting that lists of tags are equal at compile time.
#[doc(hidden)]
pub const fn assert_tags_are_equal(failure_description: &str, a: &[u32], b: &[u32]) {
    if a.len() != b.len() {
        #[cfg(feature = "extended-diagnostics")]
        concat_panic!({}: failure_description, ": expected ", a, " but got ", b);
        #[cfg(not(feature = "extended-diagnostics"))]
        panic!("{}", failure_description);
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            #[cfg(feature = "extended-diagnostics")]
            concat_panic!({}: failure_description, ": expected ", a, " but got ", b);
            #[cfg(not(feature = "extended-diagnostics"))]
            panic!("{}", failure_description);
        }
        i += 1;
    }
}
