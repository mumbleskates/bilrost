//! This is the module that defines the core encoding implementation for bilrost, including the
//! traits that dispatch it.
//!
//! ---
//!
//! ⚠️ All of the things beneath this module are "under the hood" and are intended for consumption
//! of `bilrost` itself, in the output of the derive macros of the exactly matching version of the
//! library. Historically these have undergone significant evolution, and stability of outside use
//! of anything in or under this module is to be considered **EXPERIMENTAL** until further notice.
//! That said, the changes that have been made over time are all aimed at eventual stability and a
//! useful set of features for advanced external users to have a set of tools to work around
//! annoyances and end up with a result that is as pleasing, ergonomic, and performant as possible.
//!
//! ---
//!
//! There are a whole product of traits for encoding and decoding in bilrost, based on the type of
//! value and the capability.
//!
//! Values:
//!
//! * supported value that has an empty state (and can be a message field)
//! * any supported value (may be nested)
//! * (a helper trait that encodes/decodes fields for anything that implements value encoding)
//! * oneof with no empty state of its own (must be nested in Option)
//! * oneof with an empty state
//! * message
//!
//! Capabilities:
//!
//! * encode
//! * decode to owned value, relaxed mode
//! * decode to owned value, distinguished mode
//! * decode from borrowed slice, relaxed mode
//! * decode from borrowed slice, distinguished mode
//!
//! ...And here are the names of the traits we define for all the above combinations:
//!
//! * Supported value with an empty state:
//!     * `Encoder<E, T>`
//!     * `Decoder<E, T>`
//!     * `DistinguishedDecoder<E, T>`
//!     * `BorrowDecoder<'a, E, T>`
//!     * `DistinguishedBorrowDecoder<'a, E, T>`
//! * Any supported value:
//!     * `ValueEncoder<E, T>`
//!     * `ValueDecoder<E, T>`
//!     * `DistinguishedValueDecoder<E, T>`
//!     * `ValueBorrowDecoder<'a, E, T>`
//!     * `DistinguishedValueBorrowDecoder<'a, E, T>`
//! * Oneof with no empty state:
//!     * `NonEmptyOneof`
//!     * `NonEmptyOneofDecoder`
//!     * `NonEmptyDistinguishedOneofDecoder`
//!     * `NonEmptyOneofBorrowDecoder<'a>`
//!     * `NonEmptyDistinguishedOneofBorrowDecoder<'a>`
//! * Oneof:
//!     * `Oneof`
//!     * `OneofDecoder`
//!     * `DistinguishedOneofDecoder`
//!     * `OneofBorrowDecoder<'a>`
//!     * `DistinguishedOneofBorrowDecoder<'a>`
//! * Message:
//!     * `RawMessage`
//!     * `RawMessageDecoder`
//!     * `RawDistinguishedMessageDecoder`
//!     * `RawMessageBorrowDecoder<'a>`
//!     * `RawDistinguishedMessageBorrowDecoder<'a>`
//!
//! These traits and their main generic implementations are defined in this module and in its
//! `message` and `oneof` sub-modules.
//!
//! Values themselves often have the trait of being
//!
//! The traits for values are parametrized by "encodings", marker structs which denote *how* the
//! value is to be encoded, whose implementations are also defined in sub-modules here. These
//! include:
//!
//! * `Fixed`, for fixed-width encodings of either 4 or 8 bytes
//! * `General`, the default encoding in messages
//! * `GeneralPacked`, the default encoding in oneofs and inside already-packed values
//! * `Map<KE, VE>`, which encodes key/value mappings where the keys are encoded by the given
//!   encodings `KE` and `VE`
//! * `Packed<E>`, which encodes homogenous containers as a value packed in a single field with the
//!   given encoding `E`
//! * `PlainBytes`, which implements encodings for `[u8]`-like types
//! * `Proxied<E>`, which encodes values with the given encoding `E` after translating them to and
//!   from a proxy type using the Proxiable traits
//! * `(T1, T2, ...)`, which implements encoding for tuples which have corresponding fields
//! * `Unpacked<E>`, which encodes homogenous containers as zero or more values each encoded as
//!   their own message field with the given encoding `E`
//! * `Varint`, which encodes all integers in the varint format (even `u8` and `i8`)
//!
//! Type support for third party types and for many common aspects of core type implementations can
//! be found in the `type_support` sub-module tree.

use crate::buf::ReverseBuf;
use crate::DecodeErrorKind::{
    InvalidVarint, NotCanonical, Oversize, TagOverflowed, Truncated, UnknownField, WrongWireType,
};
use crate::{decode_length_delimiter, DecodeError, DecodeErrorKind};
use bytes::buf::Take;
use bytes::{Buf, BufMut};
use core::cmp::{min, Eq, Ordering, PartialEq};
use core::default::Default;
use core::fmt::Debug;
use core::ops::{Deref, DerefMut};

pub(crate) mod decoding_modes;
mod encoding_traits;
mod fixed;
mod general;
mod local_proxy;
mod macros;
mod map;
pub(crate) mod message;
mod oneof;
/// Tools for opaque encoding and decoding of any valid bilrost data.
pub mod opaque;
mod packed;
mod plain_bytes;
mod proxy;
mod range_as_tuple;
#[cfg(test)]
mod test;
mod tuple;
mod type_support;
mod underived;
mod unpacked;
mod value_traits;
mod varint;

pub use encoding_traits::Wiretyped;
pub use encoding_traits::{
    BorrowDecoder, Decoder, DistinguishedBorrowDecoder, DistinguishedDecoder, Encoder,
};
pub use encoding_traits::{
    DistinguishedFieldBorrowDecoder, DistinguishedFieldDecoder, FieldBorrowDecoder, FieldDecoder,
    FieldEncoder,
};
pub use encoding_traits::{
    DistinguishedValueBorrowDecoder, DistinguishedValueDecoder, ValueBorrowDecoder, ValueDecoder,
    ValueEncoder,
};
pub use macros::{delegate_encoding, delegate_proxied_encoding, delegate_value_encoding};
pub(crate) use macros::{
    encoding_implemented_via_value_encoding, encoding_uses_base_empty_state,
    impl_cow_value_encoding, implement_core_empty_state_rules,
};
pub use message::{
    MessageEncoding, RawDistinguishedMessageBorrowDecoder, RawDistinguishedMessageDecoder,
    RawMessage, RawMessageBorrowDecoder, RawMessageDecoder,
};
pub use oneof::{
    DistinguishedOneofBorrowDecoder, DistinguishedOneofDecoder, Oneof, OneofBorrowDecoder,
    OneofDecoder,
};
pub use oneof::{
    NonEmptyDistinguishedOneofBorrowDecoder, NonEmptyDistinguishedOneofDecoder, NonEmptyOneof,
    NonEmptyOneofBorrowDecoder, NonEmptyOneofDecoder,
};
pub use value_traits::{
    empty_state_via_default, empty_state_via_for_overwrite, for_overwrite_via_default, Collection,
    DistinguishedCollection, DistinguishedMapping, EmptyState, Enumeration, ForOverwrite, Mapping,
};

/// Fixed-size encoder. Encodes integers in fixed-size format.
pub use fixed::Fixed;
/// General encoder. Encodes strings and byte blobs, numbers as varints, floats as fixed size,
/// repeated types unpacked, maps with its own encoding for keys and values, and message types.
pub use general::{General, GeneralGeneric, GeneralPacked};
/// Encoder for mapping types. Encodes alternating keys and values in packed format.
pub use map::Map;
/// Packed encoder. Encodes repeated types in packed format.
pub use packed::Packed;
/// Encoder that decodes bytes data directly into `Vec<u8>`, rather than requiring it to be wrapped
/// in `Blob`.
pub use plain_bytes::PlainBytes;
/// Unpacked encoder. Encodes repeated types in unpacked format, writing repeated fields.
pub use unpacked::Unpacked;
/// Varint encoder. Encodes integer types as varints.
pub use varint::Varint;

// Proxied is an encoding that provides value-encoding implementations for types that implement
// their encoded representations by first translating to another type that is already supported.
pub use proxy::{DistinguishedProxiable, Proxiable, Proxied};

// This is an array of the smallest values whose varint representation is N+1 bytes, where N is the
// index in the array.
const VARINT_LIMIT: [u64; 9] = [
    0,
    0x80,
    0x4080,
    0x20_4080,
    0x1020_4080,
    0x8_1020_4080,
    0x408_1020_4080,
    0x2_0408_1020_4080,
    0x102_0408_1020_4080,
];

/// Encodes an integer value into LEB128-bijective variable length format, and writes it to the
/// buffer. The buffer must have enough remaining space (maximum 9 bytes).
#[cfg(any(
    all(
        feature = "auto-unroll-varint-encoding",
        not(feature = "prefer-no-unroll-varint-encoding")
    ),
    feature = "unroll-varint-encoding",
))]
#[inline(always)]
pub fn encode_varint<B: BufMut + ?Sized>(value: u64, buf: &mut B) {
    #[inline(always)]
    fn encode_varint_inner<const N: usize>(mut value: u64, buf: &mut (impl BufMut + ?Sized)) {
        let mut varint_data = [0u8; N];
        for b in &mut varint_data[..N - 1] {
            *b = ((value & 0x7F) | 0x80) as u8;
            value = (value >> 7) - 1;
        }
        varint_data[N - 1] = value as u8;
        buf.put_slice(&varint_data);
    }

    if value < VARINT_LIMIT[1] {
        buf.put_u8(value as u8);
    } else if value < VARINT_LIMIT[5] {
        if value < VARINT_LIMIT[3] {
            if value < VARINT_LIMIT[2] {
                encode_varint_inner::<2>(value, buf);
            } else {
                encode_varint_inner::<3>(value, buf);
            }
        } else if value < VARINT_LIMIT[4] {
            encode_varint_inner::<4>(value, buf);
        } else {
            encode_varint_inner::<5>(value, buf);
        }
    } else if value < VARINT_LIMIT[7] {
        if value < VARINT_LIMIT[6] {
            encode_varint_inner::<6>(value, buf);
        } else {
            encode_varint_inner::<7>(value, buf);
        }
    } else if value < VARINT_LIMIT[8] {
        encode_varint_inner::<8>(value, buf);
    } else {
        encode_varint_inner::<9>(value, buf);
    }
}

/// Encodes an integer value into LEB128-bijective variable length format, and writes it to the
/// buffer. The buffer must have enough remaining space (maximum 9 bytes).
#[cfg(not(any(
    all(
        feature = "auto-unroll-varint-encoding",
        not(feature = "prefer-no-unroll-varint-encoding")
    ),
    feature = "unroll-varint-encoding",
)))]
#[inline(always)]
pub fn encode_varint<B: BufMut + ?Sized>(mut value: u64, buf: &mut B) {
    for _ in 0..9 {
        if value < 0x80 {
            buf.put_u8(value as u8);
            break;
        } else {
            buf.put_u8(((value & 0x7F) | 0x80) as u8);
            value = (value >> 7) - 1;
        }
    }
}

/// Prepends an integer value in LEB128-bijective format to the given buffer.
#[cfg(any(
    all(
        feature = "auto-unroll-varint-encoding",
        not(feature = "prefer-no-unroll-varint-encoding")
    ),
    feature = "unroll-varint-encoding",
))]
#[inline(always)]
pub fn prepend_varint<B: ReverseBuf + ?Sized>(value: u64, buf: &mut B) {
    #[inline(always)]
    fn prepend_varint_inner<const N: usize>(mut value: u64, buf: &mut (impl ReverseBuf + ?Sized)) {
        let mut varint_data = [0u8; N];
        for b in &mut varint_data[..N - 1] {
            *b = ((value & 0x7F) | 0x80) as u8;
            value = (value >> 7) - 1;
        }
        varint_data[N - 1] = value as u8;
        buf.prepend_slice(&varint_data);
    }

    if value < VARINT_LIMIT[1] {
        buf.prepend_u8(value as u8);
    } else if value < VARINT_LIMIT[5] {
        if value < VARINT_LIMIT[3] {
            if value < VARINT_LIMIT[2] {
                prepend_varint_inner::<2>(value, buf);
            } else {
                prepend_varint_inner::<3>(value, buf);
            }
        } else if value < VARINT_LIMIT[4] {
            prepend_varint_inner::<4>(value, buf);
        } else {
            prepend_varint_inner::<5>(value, buf);
        }
    } else if value < VARINT_LIMIT[7] {
        if value < VARINT_LIMIT[6] {
            prepend_varint_inner::<6>(value, buf);
        } else {
            prepend_varint_inner::<7>(value, buf);
        }
    } else if value < VARINT_LIMIT[8] {
        prepend_varint_inner::<8>(value, buf);
    } else {
        // TODO: This implementation frequently becomes much slower for this case specifically; as
        //  much as 40% slower than the 8-byte case! Rooting out the cause of this will be a big
        //  win for performance in many cases.
        prepend_varint_inner::<9>(value, buf);
    }
}

/// Prepends an integer value in LEB128-bijective format to the given buffer.
#[cfg(not(any(
    all(
        feature = "auto-unroll-varint-encoding",
        not(feature = "prefer-no-unroll-varint-encoding")
    ),
    feature = "unroll-varint-encoding",
)))]
#[inline(always)]
pub fn prepend_varint<B: ReverseBuf + ?Sized>(mut value: u64, buf: &mut B) {
    if value < 0x80 {
        buf.prepend_u8(value as u8);
        return;
    }
    let mut varint_data = [0u8; 9];
    for (i, b) in varint_data.iter_mut().enumerate() {
        if value < 0x80 {
            *b = value as u8;
            buf.prepend_slice(&varint_data[..=i]);
            return;
        } else {
            *b = ((value & 0x7F) | 0x80) as u8;
            value = (value >> 7) - 1;
        }
    }
    buf.prepend_slice(&varint_data);
}

/// Holds a varint value and dereferences to the slice of its relevant bytes.
pub struct ConstVarint {
    value: [u8; 9],
    len: u8,
}

impl Deref for ConstVarint {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.value[..self.len as usize]
    }
}

/// Encodes a varint at const time.
pub const fn const_varint(mut value: u64) -> ConstVarint {
    let mut res = [0; 9];
    let mut i: usize = 0;
    while i < 9 {
        if value < 0x80 {
            res[i] = value as u8;
            return ConstVarint {
                value: res,
                len: (i + 1) as u8,
            };
        } else {
            res[i] = ((value as u8) & 0x7f) | 0x80;
            value = (value >> 7) - 1;
            i += 1;
        }
    }
    ConstVarint { value: res, len: 9 }
}

/// Decodes a LEB128-bijective-encoded variable length integer from the buffer.
#[inline(always)]
pub fn decode_varint<B: Buf + ?Sized>(buf: &mut B) -> Result<u64, DecodeError> {
    let bytes = buf.chunk();
    let len = bytes.len();
    if len == 0 {
        return Err(DecodeError::new(Truncated));
    }

    let byte = bytes[0];
    if byte < 0x80 {
        buf.advance(1);
        Ok(u64::from(byte))
    } else if len >= 9 || bytes[len - 1] < 0x80 {
        // If we read an invalid varint from a contiguous slice, we still want to advance the buffer
        // by the bytes we looked at, to be maximally consistent.
        let (result, advance) = match decode_varint_slice(bytes) {
            Ok((ok, advance)) => (Ok(ok), advance),
            Err(err) => (Err(err), 9), // Invalid varints are always 9 bytes
        };
        buf.advance(advance);
        result
    } else {
        decode_varint_slow(buf)
    }
}

/// Decodes a LEB128-bijective-encoded variable length integer from the slice, returning the value
/// and the number of bytes read.
///
/// Based loosely on [`ReadVarint64FromArray`][1] with a varint overflow check from
/// [`ConsumeVarint`][2].
///
/// ## Safety
///
/// The caller must ensure that `bytes` is non-empty and either `bytes.len() >= 9` or the last
/// element in bytes is < `0x80`.
///
/// [1]: https://github.com/google/protobuf/blob/3.3.x/src/google/protobuf/io/coded_stream.cc#L365-L406
/// [2]: https://github.com/protocolbuffers/protobuf-go/blob/v1.27.1/encoding/protowire/wire.go#L358
#[cfg(not(feature = "forbid-unsafe"))]
#[inline(always)]
fn decode_varint_slice(bytes: &[u8]) -> Result<(u64, usize), DecodeError> {
    // Fully unrolled varint decoding loop. Splitting into 32-bit pieces gives better performance.

    // Use assertions to ensure memory safety, but it should always be optimized after inline.
    assert!(!bytes.is_empty());
    // If the varint is 9 bytes long, the last byte may have its MSB set.
    assert!(bytes.len() >= 9 || bytes[bytes.len() - 1] < 0x80);

    let mut b: u8 = unsafe { *bytes.get_unchecked(0) };
    let mut part0: u32 = u32::from(b);
    if b < 0x80 {
        return Ok((u64::from(part0), 1));
    };
    b = unsafe { *bytes.get_unchecked(1) };
    part0 += u32::from(b) << 7;
    if b < 0x80 {
        return Ok((u64::from(part0), 2));
    };
    b = unsafe { *bytes.get_unchecked(2) };
    part0 += u32::from(b) << 14;
    if b < 0x80 {
        return Ok((u64::from(part0), 3));
    };
    b = unsafe { *bytes.get_unchecked(3) };
    part0 += u32::from(b) << 21;
    if b < 0x80 {
        return Ok((u64::from(part0), 4));
    };
    let value = u64::from(part0);

    b = unsafe { *bytes.get_unchecked(4) };
    let mut part1: u32 = u32::from(b);
    if b < 0x80 {
        return Ok((value + (u64::from(part1) << 28), 5));
    };
    b = unsafe { *bytes.get_unchecked(5) };
    part1 += u32::from(b) << 7;
    if b < 0x80 {
        return Ok((value + (u64::from(part1) << 28), 6));
    };
    b = unsafe { *bytes.get_unchecked(6) };
    part1 += u32::from(b) << 14;
    if b < 0x80 {
        return Ok((value + (u64::from(part1) << 28), 7));
    };
    b = unsafe { *bytes.get_unchecked(7) };
    part1 += u32::from(b) << 21;
    if b < 0x80 {
        return Ok((value + (u64::from(part1) << 28), 8));
    };
    let value = value + ((u64::from(part1)) << 28);

    b = unsafe { *bytes.get_unchecked(8) };
    if (b as u32) + ((value >> 56) as u32) > 0xff {
        Err(DecodeError::new(InvalidVarint))
    } else {
        Ok((value + (u64::from(b) << 56), 9))
    }
}
#[cfg(feature = "forbid-unsafe")]
#[inline(always)]
fn decode_varint_slice(bytes: &[u8]) -> Result<(u64, usize), DecodeError> {
    // Fully unrolled varint decoding loop. Splitting into 32-bit pieces gives better performance.

    // Use assertions to ensure memory safety, but it should always be optimized after inline.
    assert!(!bytes.is_empty());
    // If the varint is 9 bytes long, the last byte may have its MSB set.
    assert!(bytes.len() >= 9 || bytes[bytes.len() - 1] < 0x80);

    let mut b: u8 = bytes[0];
    let mut part0: u32 = u32::from(b);
    if b < 0x80 {
        return Ok((u64::from(part0), 1));
    };
    b = bytes[1];
    part0 += u32::from(b) << 7;
    if b < 0x80 {
        return Ok((u64::from(part0), 2));
    };
    b = bytes[2];
    part0 += u32::from(b) << 14;
    if b < 0x80 {
        return Ok((u64::from(part0), 3));
    };
    b = bytes[3];
    part0 += u32::from(b) << 21;
    if b < 0x80 {
        return Ok((u64::from(part0), 4));
    };
    let value = u64::from(part0);

    b = bytes[4];
    let mut part1: u32 = u32::from(b);
    if b < 0x80 {
        return Ok((value + (u64::from(part1) << 28), 5));
    };
    b = bytes[5];
    part1 += u32::from(b) << 7;
    if b < 0x80 {
        return Ok((value + (u64::from(part1) << 28), 6));
    };
    b = bytes[6];
    part1 += u32::from(b) << 14;
    if b < 0x80 {
        return Ok((value + (u64::from(part1) << 28), 7));
    };
    b = bytes[7];
    part1 += u32::from(b) << 21;
    if b < 0x80 {
        return Ok((value + (u64::from(part1) << 28), 8));
    };
    let value = value + ((u64::from(part1)) << 28);

    b = bytes[8];
    if (b as u32) + ((value >> 56) as u32) > 0xff {
        Err(DecodeError::new(InvalidVarint))
    } else {
        Ok((value + (u64::from(b) << 56), 9))
    }
}

/// Decodes a LEB128-encoded variable length integer from the buffer, advancing the buffer as
/// necessary.
#[inline(never)]
#[cold]
fn decode_varint_slow<B: Buf + ?Sized>(buf: &mut B) -> Result<u64, DecodeError> {
    let mut value = 0;
    for count in 0..min(8, buf.remaining()) {
        let byte = buf.get_u8();
        value += u64::from(byte) << (count * 7);
        if byte < 0x80 {
            return Ok(value);
        }
    }
    // We only reach here if every byte so far had its high bit set. We've either reached the end of
    // the buffer or the ninth byte. If it's the former, the varint qualifies as truncated.
    if !buf.has_remaining() {
        return Err(DecodeError::new(Truncated));
    }
    // The decoding process for bijective varints is largely the same as for non-bijective, except
    // we simply don't remove the MSB from each byte before adding it to the decoded value. Thus,
    // all 64 bits are already spoken for after the 9th byte (56 from the lower 7 of the first 8
    // bytes and 8 more from the 9th byte) and we can check for uint64 overflow after reading the
    // 9th byte; the 10th byte that would be obligated by the encoding if we cared about
    // generalizing the encoding to more than 64 bit numbers would always be zero, and if there is a
    // desire to encode varints greater than 64 bits in size it is more efficient to use a
    // length-prefixed encoding, which is just the blob wiretype.
    u64::checked_add(value, u64::from(buf.get_u8()) << 56).ok_or(DecodeError::new(InvalidVarint))
    // There is probably a reason why using u64::checked_add here seems to cause decoding even
    // smaller varints to bench faster, while using it in the fast-path in decode_varint_slice
    // causes a 5x pessimization. Probably best not to worry about it too much.
}

/// Additional information passed to every decode/merge function.
///
/// The context should be passed by value and can be freely cloned. When passing
/// to a function which is decoding a nested object, then use `enter_recursion`.
#[derive(Clone, Debug)]
pub struct DecodeContext {
    /// How many times we can recurse in the current decode stack before we hit
    /// the recursion limit.
    ///
    /// The recursion limit is defined by `RECURSION_LIMIT` and cannot be
    /// customized. The recursion limit can be ignored by building the Bilrost
    /// crate with the `no-recursion-limit` feature.
    #[cfg(not(feature = "no-recursion-limit"))]
    recurse_count: u32,
}

impl Default for DecodeContext {
    #[inline]
    fn default() -> DecodeContext {
        DecodeContext {
            #[cfg(not(feature = "no-recursion-limit"))]
            recurse_count: crate::RECURSION_LIMIT,
        }
    }
}

impl DecodeContext {
    /// Call this function before recursively decoding.
    ///
    /// There is no `exit` function since this function creates a new `DecodeContext`
    /// to be used at the next level of recursion. Continue to use the old context
    // at the previous level of recursion.
    #[inline]
    pub fn enter_recursion(&self) -> DecodeContext {
        DecodeContext {
            #[cfg(not(feature = "no-recursion-limit"))]
            recurse_count: self.recurse_count - 1,
        }
    }

    /// Checks whether the recursion limit has been reached in the stack of
    /// decodes described by the `DecodeContext` at `self.ctx`.
    ///
    /// Returns `Ok<()>` if it is ok to continue recursing.
    /// Returns `Err<DecodeError>` if the recursion limit has been reached.
    #[inline]
    pub fn limit_reached(&self) -> Result<(), DecodeError> {
        #[cfg(not(feature = "no-recursion-limit"))]
        if self.recurse_count == 0 {
            return Err(DecodeError::new(DecodeErrorKind::RecursionLimitReached));
        }
        Ok(())
    }
}

/// Additional information passed to every distinguished decode/merge function.
///
/// The context should be passed by value and can be freely cloned. When passing
/// to a function which is decoding a nested object, then use `enter_recursion`.
#[derive(Clone, Debug)]
pub struct RestrictedDecodeContext {
    context: DecodeContext,
    min_canonicity: Canonicity,
}

impl RestrictedDecodeContext {
    /// Creates a new context with a given minimum canonicity.
    pub fn new(min_canonicity: Canonicity) -> Self {
        Self {
            context: DecodeContext::default(),
            min_canonicity,
        }
    }

    /// Call this function before recursively decoding.
    ///
    /// There is no `exit` function since this function creates a new `DecodeContext`
    /// to be used at the next level of recursion. Continue to use the old context
    // at the previous level of recursion.
    #[inline]
    pub fn enter_recursion(&self) -> Self {
        Self {
            context: self.context.enter_recursion(),
            ..*self
        }
    }

    /// Checks whether the recursion limit has been reached in the stack of
    /// decodes described by the `DecodeContext` at `self.ctx`.
    ///
    /// Returns `Ok<()>` if it is ok to continue recursing.
    /// Returns `Err<DecodeError>` if the recursion limit has been reached.
    #[inline]
    pub fn limit_reached(&self) -> Result<(), DecodeError> {
        self.context.limit_reached()
    }

    /// Returns the inner non-restricted context for relaxed decoding.
    pub fn into_inner(self) -> DecodeContext {
        self.context
    }

    /// Checks the given canonicity against the minimum constraint that this context has.
    ///
    /// This must be called and checked at a few specific times, whenever the canonicity is
    /// (possibly) being reduced and it hasn't already been checked by some source that returned
    /// that canonicity value:
    ///
    /// 1. When decoding, and a non-canonical state is observed (such as a value that is represented
    ///    in a non-canonical form, or an unknown field in the encoding), this can be called with a
    ///    literal `Canonicity` value.
    /// 2. After calling one of the distinguished helper trait methods that does not have a
    ///    restricted context in its parameters to check against, and therefore could not possibly
    ///    have converted a non-canonical state into an error yet:
    ///    2a. `DistinguishedProxiable::decode_proxy_distinguished`
    ///    2b. `DistinguishedCollection::insert_distinguished`
    ///
    /// After these canonicity values have been checked, and at all other times, it should be safe
    /// to directly update the canonicity that an implementation will itself return since each value
    /// it receives should already be tolerated by the context.
    #[inline]
    pub fn check(&self, canon: Canonicity) -> Result<Canonicity, DecodeError> {
        match (canon < self.min_canonicity, canon) {
            (true, Canonicity::NotCanonical) => Err(DecodeError::new(NotCanonical)),
            (true, Canonicity::HasExtensions) => Err(DecodeError::new(UnknownField)),
            _ => Ok(canon),
        }
    }
}

/// Returns the encoded length of the value in LEB128-bijective variable length format.
/// The returned value will be between 1 and 9, inclusive.
#[inline(always)]
pub const fn encoded_len_varint(value: u64) -> usize {
    if value < VARINT_LIMIT[1] {
        1
    } else if value < VARINT_LIMIT[5] {
        if value < VARINT_LIMIT[3] {
            if value < VARINT_LIMIT[2] {
                2
            } else {
                3
            }
        } else if value < VARINT_LIMIT[4] {
            4
        } else {
            5
        }
    } else if value < VARINT_LIMIT[7] {
        if value < VARINT_LIMIT[6] {
            6
        } else {
            7
        }
    } else if value < VARINT_LIMIT[8] {
        8
    } else {
        9
    }
}

/// Represents one of the four opaque field types of a Bilrost message field on the wire.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum WireType {
    Varint = 0,
    LengthDelimited = 1,
    ThirtyTwoBit = 2,
    SixtyFourBit = 3,
}

impl From<u8> for WireType {
    #[inline]
    fn from(value: u8) -> Self {
        match value & 0b11 {
            0 => WireType::Varint,
            1 => WireType::LengthDelimited,
            2 => WireType::ThirtyTwoBit,
            3 => WireType::SixtyFourBit,
            _ => unreachable!(),
        }
    }
}

impl WireType {
    const fn fixed_size(self) -> Option<usize> {
        match self {
            WireType::SixtyFourBit => Some(8),
            WireType::ThirtyTwoBit => Some(4),
            WireType::Varint | WireType::LengthDelimited => None,
        }
    }
}

/// Writes keys for the provided tags.
#[derive(Default)]
pub struct TagWriter {
    last_tag: u32,
}

impl TagWriter {
    pub fn new() -> Self {
        Default::default()
    }

    /// Encode the key delta to the given key into the buffer.
    ///
    /// All fields must be encoded in order; this is enforced in the encoding by encoding each
    /// field's tag as a non-negative delta from the previously encoded field's tag. The tag delta
    /// is encoded in the bits above the lowest two bits in the key delta, which encode the wire
    /// type. When decoding, the wire type is taken as-is, and the tag delta added to the tag of the
    /// last field decoded.
    #[inline(always)]
    pub fn encode_key<B: BufMut + ?Sized>(&mut self, tag: u32, wire_type: WireType, buf: &mut B) {
        let tag_delta = tag
            .checked_sub(self.last_tag)
            .expect("fields encoded out of order");
        self.last_tag = tag;
        encode_varint(((tag_delta as u64) << 2) | (wire_type as u64), buf);
    }
}

/// Writes keys for the provided tags into a prepend-only buffer.
#[derive(Default)]
pub struct TagRevWriter {
    current_key: Option<(u32, WireType)>,
}

impl TagRevWriter {
    pub fn new() -> Self {
        Default::default()
    }

    /// Encode the key delta to the given key into the buffer.
    ///
    /// All fields must be encoded in order; this is enforced in the encoding by encoding each
    /// field's tag as a non-negative delta from the previously encoded field's tag. The tag delta
    /// is encoded in the bits above the lowest two bits in the key delta, which encode the wire
    /// type. When decoding, the wire type is taken as-is, and the tag delta added to the tag of the
    /// last field decoded.
    #[inline(always)]
    pub fn begin_field<B: ReverseBuf + ?Sized>(
        &mut self,
        tag: u32,
        wire_type: WireType,
        buf: &mut B,
    ) {
        if let Some((current_tag, current_wire_type)) = self.current_key {
            let tag_delta = current_tag
                .checked_sub(tag)
                .expect("fields prepended out of order");
            prepend_varint(((tag_delta as u64) << 2) | (current_wire_type as u64), buf);
        }
        self.current_key = Some((tag, wire_type));
    }

    /// Finishes writing the current message by encoding the key of the first field that appeared.
    #[inline(always)]
    pub fn finalize<B: ReverseBuf + ?Sized>(&mut self, buf: &mut B) {
        let Some((tag_delta, wire_type)) = self.current_key else {
            return;
        };
        prepend_varint(((tag_delta as u64) << 2) | (wire_type as u64), buf);
        self.current_key = None;
    }
}

/// Trait for simulating the writing of tags in order to measure the length that an encoding would
/// be.
pub trait TagMeasurer {
    fn key_len(&mut self, tag: u32) -> usize;
}

/// Simulator for writing tags, capable of outputting their encoded length.
#[derive(Default)]
pub struct RuntimeTagMeasurer {
    last_tag: u32,
}

impl RuntimeTagMeasurer {
    pub fn new() -> Self {
        Self::default()
    }
}

impl TagMeasurer for RuntimeTagMeasurer {
    /// Returns the number of bytes that would be written if the given tag was encoded next, and
    /// also advances the state of the encoder as if that tag was written.
    #[inline(always)]
    fn key_len(&mut self, tag: u32) -> usize {
        let tag_delta = tag
            .checked_sub(self.last_tag)
            .expect("fields encoded out of order");
        self.last_tag = tag;
        encoded_len_varint((tag_delta as u64) << 2)
    }
}

/// Simulator for writing tags which assumes that tags will never need to be encoded in more than
/// a single byte. This holds true in a number of message types that can't output large tag numbers,
/// such as tuples.
#[derive(Default)]
pub struct TrivialTagMeasurer {
    #[cfg(debug_assertions)]
    last_tag: u32,
}

impl TrivialTagMeasurer {
    pub fn new() -> Self {
        Self::default()
    }
}

impl TagMeasurer for TrivialTagMeasurer {
    #[inline(always)]
    fn key_len(&mut self, _tag: u32) -> usize {
        #[cfg(debug_assertions)]
        {
            assert!(_tag >= self.last_tag, "fields encoded out of order");
            assert!(_tag < 32);
            self.last_tag = _tag;
        }
        1
    }
}

/// Reads tags from a buffer.
#[derive(Default)]
pub struct TagReader {
    last_tag: u32,
}

impl TagReader {
    pub fn new() -> Self {
        Default::default()
    }

    #[inline(always)]
    pub fn decode_key<B: Buf + ?Sized>(
        &mut self,
        mut buf: Capped<B>,
    ) -> Result<(u32, WireType), DecodeError> {
        let key = buf.decode_varint()?;
        let tag_delta = u32::try_from(key >> 2).map_err(|_| DecodeError::new(TagOverflowed))?;
        let tag = self
            .last_tag
            .checked_add(tag_delta)
            .ok_or_else(|| DecodeError::new(TagOverflowed))?;
        let wire_type = WireType::from(key as u8);
        self.last_tag = tag;
        Ok((tag, wire_type))
    }
}

/// Checks that the expected wire type matches the actual wire type,
/// or returns an error result.
#[inline(always)]
pub fn check_wire_type(expected: WireType, actual: WireType) -> Result<(), DecodeError> {
    if expected != actual {
        return Err(DecodeError::new(WrongWireType));
    }
    Ok(())
}

/// A soft-limited wrapper for `impl Buf` that doesn't invoke extra work whenever the buffer is read
/// from, only when the remaining bytes are checked. This means it can be nested arbitrarily without
/// adding extra work every time.
pub struct Capped<'a, B: 'a + Buf + ?Sized> {
    buf: &'a mut B,
    extra_bytes_remaining: usize,
}

impl<'a, B: 'a + Buf + ?Sized> Capped<'a, B> {
    /// Creates a Capped instance with a cap at the very end of the given buffer.
    pub fn new(buf: &'a mut B) -> Self {
        Self {
            buf,
            extra_bytes_remaining: 0,
        }
    }

    /// Reads a length from the beginning of the given buffer, then returns a Capped instance
    /// with its cap at the end of the delimited range.
    pub fn new_length_delimited(buf: &'a mut B) -> Result<Self, DecodeError> {
        let len = decode_length_delimiter(&mut *buf)?;
        let remaining = buf.remaining();
        if len > remaining {
            return Err(DecodeError::new(Truncated));
        }
        Ok(Self {
            buf,
            extra_bytes_remaining: remaining - len,
        })
    }

    #[inline(always)]
    pub fn lend(&mut self) -> Capped<'_, B> {
        Capped {
            buf: self.buf,
            extra_bytes_remaining: self.extra_bytes_remaining,
        }
    }

    /// Reads a length delimiter from the beginning of the wrapped buffer, then returns a subsidiary
    /// Capped instance for the delineated bytes if it does not overrun the underlying buffer or
    /// this instance's cap.
    #[inline(always)]
    pub fn take_length_delimited(&mut self) -> Result<Capped<'_, B>, DecodeError> {
        let len = decode_length_delimiter(&mut *self.buf)?;
        // Rather than checking that len + extra_bytes_remaining fits in remaining, we subtract and
        // compare the smaller values to avoid situations that may overflow.
        let remaining = self.buf.remaining();
        if len > remaining {
            return Err(DecodeError::new(Truncated));
        }
        let extra_bytes_remaining = remaining - len;
        if extra_bytes_remaining < self.extra_bytes_remaining {
            return Err(DecodeError::new(Truncated));
        }
        Ok(Capped {
            buf: self.buf,
            extra_bytes_remaining,
        })
    }

    #[inline]
    pub fn buf(&mut self) -> &mut B {
        self.buf
    }

    #[inline(always)]
    pub fn take_all(self) -> Take<&'a mut B> {
        let len = self.remaining_before_cap();
        self.buf.take(len)
    }

    #[inline(always)]
    pub fn decode_varint(&mut self) -> Result<u64, DecodeError> {
        decode_varint(self.buf).map_err(|err| {
            // Varints are always decoded greedily from the underlying buffer, so we want to
            // transform any non-truncation errors into Truncated to pretend that we stopped sooner.
            if err.kind() == InvalidVarint && self.over_cap() {
                DecodeError::new(Truncated)
            } else {
                err
            }
        })
    }

    /// Returns the number of bytes left before the cap.
    #[inline(always)]
    pub fn remaining_before_cap(&self) -> usize {
        self.buf
            .remaining()
            .saturating_sub(self.extra_bytes_remaining)
    }

    #[inline(always)]
    fn over_cap(&self) -> bool {
        self.buf.remaining() < self.extra_bytes_remaining
    }

    #[inline(always)]
    pub fn has_remaining(&self) -> Result<bool, DecodeErrorKind> {
        match self.buf.remaining().cmp(&self.extra_bytes_remaining) {
            Ordering::Less => Err(Truncated),
            Ordering::Equal => Ok(false),
            Ordering::Greater => Ok(true),
        }
    }
}

impl<'a> Capped<'_, &'a [u8]> {
    /// Reads a length delimiter from the beginning of the wrapped slice, then advances that inner
    /// slice past the delineated bytes and returns them borrowed with lifetime if the instance's
    /// cap is not overrun.
    #[inline(always)]
    pub fn take_borrowed_length_delimited(&mut self) -> Result<&'a [u8], DecodeError> {
        let len = decode_length_delimiter(&mut *self.buf)?;
        // Rather than checking that len + extra_bytes_remaining fits in remaining, we subtract and
        // compare the smaller values to avoid situations that may overflow.
        let remaining = self.buf.remaining();
        if len > remaining {
            return Err(DecodeError::new(Truncated));
        }
        let extra_bytes_remaining = remaining - len;
        if extra_bytes_remaining < self.extra_bytes_remaining {
            return Err(DecodeError::new(Truncated));
        }

        // Unlike the non-borrowed impl, we advance the buf and give the slice directly as a result.
        let taken;
        // MSRV: this could be `split_at_unchecked` (1.79)
        (taken, *self.buf) = {
            #[cfg(not(feature = "forbid-unsafe"))]
            // SAFETY: we checked above that `self.buf` is of at least length `len`
            unsafe {
                (self.buf.get_unchecked(..len), self.buf.get_unchecked(len..))
            }
            #[cfg(feature = "forbid-unsafe")]
            {
                (&self.buf[..len], &self.buf[len..])
            }
        };

        Ok(taken)
    }
}

impl<B: Buf + ?Sized> Deref for Capped<'_, B> {
    type Target = B;

    fn deref(&self) -> &B {
        self.buf
    }
}

impl<B: Buf + ?Sized> DerefMut for Capped<'_, B> {
    fn deref_mut(&mut self) -> &mut B {
        self.buf
    }
}

/// Returns `Some` if there are more bytes in the buffer and the next data in the buffer begins
/// with a "repeated" field key (a key with a tag delta of zero). If the repeated field key is found
/// it is consumed; if it does not exist, the buffer is unchanged.
#[inline(always)]
fn peek_repeated_field<B: Buf + ?Sized>(buf: &mut Capped<B>) -> Option<WireType> {
    if buf.remaining_before_cap() == 0 {
        return None;
    }
    // Peek the first byte of the next field's key.
    let peek_key = buf.chunk()[0];
    if peek_key >= 4 {
        return None; // The next field has a different tag than this one.
    }
    // The next field's key has a repeated tag (its delta is zero). Consume the peeked key and
    // return its wire type
    buf.advance(1);
    Some(WireType::from(peek_key))
}

/// Consumes and discards the value of a field that has the given key, as well as the keys and
/// values of every following field with the same tag. The key of the field should be consumed
/// before this function is called.
pub fn skip_field<B: Buf + ?Sized>(
    mut wire_type: WireType,
    mut buf: Capped<B>,
) -> Result<(), DecodeError> {
    loop {
        let len = match wire_type {
            WireType::Varint => buf.decode_varint().map(|_| 0)?,
            WireType::ThirtyTwoBit => 4,
            WireType::SixtyFourBit => 8,
            WireType::LengthDelimited => {
                usize::try_from(buf.decode_varint()?).map_err(|_| DecodeError::new(Oversize))?
            }
        };

        if len > buf.remaining() {
            return Err(DecodeError::new(Truncated));
        }
        buf.advance(len);

        match peek_repeated_field(&mut buf) {
            None => break,
            Some(next_wire_type) => {
                wire_type = next_wire_type;
            }
        }
    }
    Ok(())
}

/// Indicator of the "canonicity" of a decoded value or a decoding process that was performed.
///
/// See documentation on `RestrictedDecodeContext::check` for details on when this should be checked
/// for returning canonicity errors.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
#[must_use]
pub enum Canonicity {
    /// The decoded data was not represented in its canonical form.
    NotCanonical,
    /// All known fields were represented canonically, but some unknown fields were present.
    HasExtensions,
    /// The decoded data was fully canonical.
    Canonical,
}

impl Canonicity {
    /// Update this value to the lowest (least-canonical) state.
    #[inline(always)]
    pub fn update(&mut self, other: Self) {
        *self = min(*self, other);
    }
}

impl FromIterator<Canonicity> for Canonicity {
    #[inline(always)]
    fn from_iter<T: IntoIterator<Item = Canonicity>>(iter: T) -> Self {
        iter.into_iter().min().unwrap_or(Canonicity::Canonical)
    }
}

/// Trait for values and results bearing canonicity information (represented by the `Canonicity`
/// enum).
pub trait WithCanonicity {
    /// The type of the value without any canonicity information.
    type Value;
    // Type the value is turned into when non-canonical states are turned into error states or
    // removed.
    type WithoutCanonicity;

    /// Get the value if it is fully canonical, otherwise returning an error.
    fn canonical(self) -> Result<Self::Value, DecodeErrorKind>;

    /// Get the value as long as its known fields are canonical, otherwise returning an error.
    fn canonical_with_extensions(self) -> Result<Self::Value, DecodeErrorKind>;

    /// Discards the canonicity.
    ///
    /// If this method is always being used and canonicity information is always discarded,
    /// distinguished decoding may not be needed, and the program can be made more efficient by
    /// simply using relaxed decoding mode.
    fn value(self) -> Self::WithoutCanonicity;
}

impl WithCanonicity for Canonicity {
    type Value = ();
    type WithoutCanonicity = Self::Value;

    fn canonical(self) -> Result<(), DecodeErrorKind> {
        match self {
            Canonicity::NotCanonical => Err(NotCanonical),
            Canonicity::HasExtensions => Err(UnknownField),
            Canonicity::Canonical => Ok(()),
        }
    }

    fn canonical_with_extensions(self) -> Result<(), DecodeErrorKind> {
        match self {
            Canonicity::NotCanonical => Err(NotCanonical),
            Canonicity::HasExtensions | Canonicity::Canonical => Ok(()),
        }
    }

    fn value(self) {}
}

impl WithCanonicity for &Canonicity {
    type Value = ();
    type WithoutCanonicity = Self::Value;

    fn canonical(self) -> Result<(), DecodeErrorKind> {
        match self {
            Canonicity::NotCanonical => Err(NotCanonical),
            Canonicity::HasExtensions => Err(UnknownField),
            Canonicity::Canonical => Ok(()),
        }
    }

    fn canonical_with_extensions(self) -> Result<(), DecodeErrorKind> {
        match self {
            Canonicity::NotCanonical => Err(NotCanonical),
            Canonicity::HasExtensions | Canonicity::Canonical => Ok(()),
        }
    }

    fn value(self) {}
}

impl<T> WithCanonicity for (T, Canonicity) {
    type Value = T;
    type WithoutCanonicity = Self::Value;

    fn canonical(self) -> Result<T, DecodeErrorKind> {
        self.1.canonical()?;
        Ok(self.0)
    }

    fn canonical_with_extensions(self) -> Result<T, DecodeErrorKind> {
        self.1.canonical_with_extensions()?;
        Ok(self.0)
    }

    fn value(self) -> T {
        self.0
    }
}

impl<'a, T> WithCanonicity for &'a (T, Canonicity) {
    type Value = &'a T;
    type WithoutCanonicity = Self::Value;

    fn canonical(self) -> Result<&'a T, DecodeErrorKind> {
        self.1.canonical()?;
        Ok(&self.0)
    }

    fn canonical_with_extensions(self) -> Result<&'a T, DecodeErrorKind> {
        self.1.canonical_with_extensions()?;
        Ok(&self.0)
    }

    fn value(self) -> &'a T {
        &self.0
    }
}

impl<T, E> WithCanonicity for Result<T, E>
where
    T: WithCanonicity,
    DecodeErrorKind: From<E>,
{
    type Value = T::Value;
    type WithoutCanonicity = Result<T::WithoutCanonicity, DecodeErrorKind>;

    fn canonical(self) -> Result<T::Value, DecodeErrorKind> {
        self?.canonical()
    }

    fn canonical_with_extensions(self) -> Result<T::Value, DecodeErrorKind> {
        self?.canonical_with_extensions()
    }

    fn value(self) -> Result<T::WithoutCanonicity, DecodeErrorKind> {
        Ok(self?.value())
    }
}

/// Trait used by derived enumeration helper functions to provide getters and setters for integer
/// fields via their associated `Enumeration` type.
pub trait EnumerationHelper<FieldType> {
    type Input;
    type Output;

    fn help_set(enum_val: Self::Input) -> FieldType;
    fn help_get(field_val: FieldType) -> Self::Output;
}

impl<T> EnumerationHelper<u32> for T
where
    T: Enumeration,
{
    type Input = T;
    type Output = Result<T, u32>;

    fn help_set(enum_val: Self) -> u32 {
        enum_val.to_number()
    }

    fn help_get(field_val: u32) -> Result<T, u32> {
        T::try_from_number(field_val)
    }
}

impl<T> EnumerationHelper<Option<u32>> for T
where
    T: Enumeration,
{
    type Input = Option<T>;
    type Output = Option<Result<T, u32>>;

    fn help_set(enum_val: Option<T>) -> Option<u32> {
        enum_val.map(|e| e.to_number())
    }

    fn help_get(field_val: Option<u32>) -> Option<Result<T, u32>> {
        field_val.map(Enumeration::try_from_number)
    }
}
