//! This is the module that defines the core encoding implementation for bilrost, including the
//! traits that dispatch it.
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
//!     * `Encoder<E>`
//!     * `Decoder<E>`
//!     * `DistinguishedDecoder<E>`
//!     * `BorrowDecoder<'a, E>`
//!     * `DistinguishedBorrowDecoder<'a, E>`
//! * Any supported value:
//!     * `ValueEncoder<E>`
//!     * `ValueDecoder<E>`
//!     * `DistinguishedValueDecoder<E>`
//!     * `ValueBorrowDecoder<'a, E>`
//!     * `DistinguishedValueBorrowDecoder<'a, E>`
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
//! * `General`, the default encoding
//! * `Map<KE, VE>`, which encodes key/value mappings where the keys are encoded by the given
//!   encodings KE and VE
//! * `Packed<E>`, which encodes homogenous containers as a value packed in a single field with the
//!   given encoding E
//! * `PlainBytes`, which implements encodings for `[u8]`-like types
//! * `Proxied<E>`, which encodes values with the given encoding E after translating them to and
//!   from a proxy type using the Proxiable traits
//! * `(T1, T2, ...)`, which implements encoding for tuples which have corresponding fields
//! * `Unpacked<E>`, which encodes homogenous containers as zero or more values each encoded as
//!   their own message field
//! * `Varint`, which encodes all integers in the varint format (even u8 and i8)
//!
//! Type support for third party types and for many common aspects of core type implementations can
//! be found in the `type_support` sub-module tree.

use crate::buf::ReverseBuf;
use crate::DecodeErrorKind::{
    InvalidVarint, NotCanonical, Oversize, TagOverflowed, Truncated, UnexpectedlyRepeated,
    UnknownField, WrongWireType,
};
use crate::{decode_length_delimiter, DecodeError, DecodeErrorKind};
use bytes::buf::Take;
use bytes::{Buf, BufMut};
use core::cmp::{min, Eq, Ordering, PartialEq};
use core::default::Default;
use core::fmt::Debug;
use core::ops::{Deref, DerefMut};

pub(crate) mod decoding_modes;
mod fixed;
mod general;
mod local_proxy;
mod map;
pub(crate) mod message;
mod oneof;
/// Tools for opaque encoding and decoding of any valid bilrost data.
pub mod opaque;
mod packed;
mod plain_bytes;
mod proxy;
mod tuple;
mod type_support;
mod underived;
mod unpacked;
mod value_traits;
mod varint;

pub use message::{
    RawDistinguishedMessageBorrowDecoder, RawDistinguishedMessageDecoder, RawMessage,
    RawMessageBorrowDecoder, RawMessageDecoder,
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
    Collection, DistinguishedCollection, DistinguishedMapping, EmptyState, Enumeration,
    ForOverwrite, Mapping,
};

/// Fixed-size encoder. Encodes integers in fixed-size format.
pub use fixed::Fixed;
/// General encoder. Encodes strings and byte blobs, numbers as varints, floats as fixed size,
/// repeated types unpacked, maps with its own encoding for keys and values, and message types.
pub use general::General;
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

/// Proxied is an encoding that provides value-encoding implementations for types that implement
/// their encoded representations by first translating to another type that is already supported.
///
/// This encoding is not yet made available outside the crate.
pub(crate) use proxy::{DistinguishedProxiable, Proxiable, Proxied};

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
            return Err(DecodeError::new(
                crate::DecodeErrorKind::RecursionLimitReached,
            ));
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
    #[inline]
    pub fn check(&self, canon: Canonicity) -> Result<Canonicity, DecodeError> {
        match (canon < self.min_canonicity, canon) {
            (true, Canonicity::NotCanonical) => Err(DecodeError::new(NotCanonical)),
            (true, Canonicity::HasExtensions) => Err(DecodeError::new(UnknownField)),
            _ => Ok(canon),
        }
    }

    /// Shorthand call to simultaneously update the given canonicity with the new value and fail if
    /// it is below the minimum for this context.
    #[inline]
    pub fn update(&self, canon: &mut Canonicity, new: Canonicity) -> Result<(), DecodeError> {
        canon.update(self.check(new)?);
        Ok(())
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
    pub fn lend(&mut self) -> Capped<B> {
        Capped {
            buf: self.buf,
            extra_bytes_remaining: self.extra_bytes_remaining,
        }
    }

    /// Reads a length delimiter from the beginning of the wrapped buffer, then returns a subsidiary
    /// Capped instance for the delineated bytes if it does not overrun the underlying buffer or
    /// this instance's cap.
    #[inline(always)]
    pub fn take_length_delimited(&mut self) -> Result<Capped<B>, DecodeError> {
        let len = decode_length_delimiter(&mut *self.buf)?;
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

#[allow(clippy::let_unit_value)]
#[cfg(test)]
mod with_canonicity {
    use super::{
        Canonicity::{self, *},
        DecodeError, DecodeErrorKind, WithCanonicity,
    };

    #[test]
    fn usability() {
        // `Canonicity`
        assert_eq!(Canonical.canonical(), Ok(()));
        assert_eq!(
            HasExtensions.canonical(),
            Err(DecodeErrorKind::UnknownField)
        );
        assert_eq!(NotCanonical.canonical(), Err(DecodeErrorKind::NotCanonical));
        assert_eq!(Canonical.canonical_with_extensions(), Ok(()));
        assert_eq!(HasExtensions.canonical_with_extensions(), Ok(()));
        assert_eq!(
            NotCanonical.canonical_with_extensions(),
            Err(DecodeErrorKind::NotCanonical)
        );
        let _: () = Canonical.value();
        let _: () = HasExtensions.value();
        let _: () = NotCanonical.value();

        // `&Canonicity`
        assert_eq!((&Canonical).canonical(), Ok(()));
        assert_eq!(
            (&HasExtensions).canonical(),
            Err(DecodeErrorKind::UnknownField)
        );
        assert_eq!(
            (&NotCanonical).canonical(),
            Err(DecodeErrorKind::NotCanonical)
        );
        assert_eq!((&Canonical).canonical_with_extensions(), Ok(()));
        assert_eq!((&HasExtensions).canonical_with_extensions(), Ok(()));
        assert_eq!(
            (&NotCanonical).canonical_with_extensions(),
            Err(DecodeErrorKind::NotCanonical)
        );
        let _: () = (&Canonical).value();
        let _: () = (&HasExtensions).value();
        let _: () = (&NotCanonical).value();

        // `(T, Canonicity)`
        assert_eq!(("foo", Canonical).canonical(), Ok("foo"));
        assert_eq!(
            ("foo", HasExtensions).canonical(),
            Err(DecodeErrorKind::UnknownField)
        );
        assert_eq!(
            ("foo", NotCanonical).canonical(),
            Err(DecodeErrorKind::NotCanonical)
        );
        assert_eq!(("foo", Canonical).canonical_with_extensions(), Ok("foo"));
        assert_eq!(
            ("foo", HasExtensions).canonical_with_extensions(),
            Ok("foo")
        );
        assert_eq!(
            ("foo", NotCanonical).canonical_with_extensions(),
            Err(DecodeErrorKind::NotCanonical)
        );
        assert_eq!(("foo", Canonical).value(), "foo");
        assert_eq!(("foo", HasExtensions).value(), "foo");
        assert_eq!(("foo", NotCanonical).value(), "foo");

        // `&(T, Canonicity)`
        assert_eq!((&("foo", Canonical)).canonical(), Ok(&"foo"));
        assert_eq!(
            (&("foo", HasExtensions)).canonical(),
            Err(DecodeErrorKind::UnknownField)
        );
        assert_eq!(
            (&("foo", NotCanonical)).canonical(),
            Err(DecodeErrorKind::NotCanonical)
        );
        assert_eq!(
            (&("foo", Canonical)).canonical_with_extensions(),
            Ok(&"foo")
        );
        assert_eq!(
            (&("foo", HasExtensions)).canonical_with_extensions(),
            Ok(&"foo")
        );
        assert_eq!(
            (&("foo", NotCanonical)).canonical_with_extensions(),
            Err(DecodeErrorKind::NotCanonical)
        );
        assert_eq!((&("foo", Canonical)).value(), &"foo");
        assert_eq!((&("foo", HasExtensions)).value(), &"foo");
        assert_eq!((&("foo", NotCanonical)).value(), &"foo");

        // `Result<(T, Canonicity), DecodeError>` with Ok
        assert_eq!(
            Result::<_, DecodeError>::Ok(("foo", Canonical)).canonical(),
            Ok("foo")
        );
        assert_eq!(
            Result::<_, DecodeError>::Ok(("foo", HasExtensions)).canonical(),
            Err(DecodeErrorKind::UnknownField)
        );
        assert_eq!(
            Result::<_, DecodeError>::Ok(("foo", NotCanonical)).canonical(),
            Err(DecodeErrorKind::NotCanonical)
        );
        assert_eq!(
            Result::<_, DecodeError>::Ok(("foo", Canonical)).canonical_with_extensions(),
            Ok("foo")
        );
        assert_eq!(
            Result::<_, DecodeError>::Ok(("foo", HasExtensions)).canonical_with_extensions(),
            Ok("foo")
        );
        assert_eq!(
            Result::<_, DecodeError>::Ok(("foo", NotCanonical)).canonical_with_extensions(),
            Err(DecodeErrorKind::NotCanonical)
        );
        assert_eq!(
            Result::<_, DecodeError>::Ok(("foo", Canonical)).value(),
            Ok("foo")
        );
        assert_eq!(
            Result::<_, DecodeError>::Ok(("foo", HasExtensions)).value(),
            Ok("foo")
        );
        assert_eq!(
            Result::<_, DecodeError>::Ok(("foo", NotCanonical)).value(),
            Ok("foo")
        );

        // `Result<&(T, Canonicity), &DecodeError>` with Ok
        assert_eq!(
            Result::<_, DecodeError>::Ok(("foo", Canonical))
                .as_ref()
                .canonical(),
            Ok(&"foo")
        );
        assert_eq!(
            Result::<_, DecodeError>::Ok(("foo", HasExtensions))
                .as_ref()
                .canonical(),
            Err(DecodeErrorKind::UnknownField)
        );
        assert_eq!(
            Result::<_, DecodeError>::Ok(("foo", NotCanonical))
                .as_ref()
                .canonical(),
            Err(DecodeErrorKind::NotCanonical)
        );
        assert_eq!(
            Result::<_, DecodeError>::Ok(("foo", Canonical))
                .as_ref()
                .canonical_with_extensions(),
            Ok(&"foo")
        );
        assert_eq!(
            Result::<_, DecodeError>::Ok(("foo", HasExtensions))
                .as_ref()
                .canonical_with_extensions(),
            Ok(&"foo")
        );
        assert_eq!(
            Result::<_, DecodeError>::Ok(("foo", NotCanonical))
                .as_ref()
                .canonical_with_extensions(),
            Err(DecodeErrorKind::NotCanonical)
        );
        assert_eq!(
            Result::<_, DecodeError>::Ok(("foo", Canonical))
                .as_ref()
                .value(),
            Ok(&"foo")
        );
        assert_eq!(
            Result::<_, DecodeError>::Ok(("foo", HasExtensions))
                .as_ref()
                .value(),
            Ok(&"foo")
        );
        assert_eq!(
            Result::<_, DecodeError>::Ok(("foo", NotCanonical))
                .as_ref()
                .value(),
            Ok(&"foo")
        );

        // `Result<_, DecodeError>` with Err
        assert_eq!(
            Result::<Canonicity, DecodeError>::Err(DecodeError::new(DecodeErrorKind::Other))
                .canonical(),
            Err(DecodeErrorKind::Other)
        );
        assert_eq!(
            Result::<Canonicity, DecodeError>::Err(DecodeError::new(DecodeErrorKind::Other))
                .canonical_with_extensions(),
            Err(DecodeErrorKind::Other)
        );
        assert_eq!(
            Result::<Canonicity, DecodeError>::Err(DecodeError::new(DecodeErrorKind::Other))
                .value(),
            Err(DecodeErrorKind::Other)
        );

        // `Result<&_, &DecodeError>` with Err
        assert_eq!(
            Result::<Canonicity, DecodeError>::Err(DecodeError::new(DecodeErrorKind::Other))
                .as_ref()
                .canonical(),
            Err(DecodeErrorKind::Other)
        );
        assert_eq!(
            Result::<Canonicity, DecodeError>::Err(DecodeError::new(DecodeErrorKind::Other))
                .as_ref()
                .canonical_with_extensions(),
            Err(DecodeErrorKind::Other)
        );
        assert_eq!(
            Result::<Canonicity, DecodeError>::Err(DecodeError::new(DecodeErrorKind::Other))
                .as_ref()
                .value(),
            Err(DecodeErrorKind::Other)
        );

        // `Result<(T, Canonicity), DecodeErrorKind>` with Ok
        assert_eq!(
            Result::<_, DecodeErrorKind>::Ok(("foo", Canonical)).canonical(),
            Ok("foo")
        );
        assert_eq!(
            Result::<_, DecodeErrorKind>::Ok(("foo", HasExtensions)).canonical(),
            Err(DecodeErrorKind::UnknownField)
        );
        assert_eq!(
            Result::<_, DecodeErrorKind>::Ok(("foo", NotCanonical)).canonical(),
            Err(DecodeErrorKind::NotCanonical)
        );
        assert_eq!(
            Result::<_, DecodeErrorKind>::Ok(("foo", Canonical)).canonical_with_extensions(),
            Ok("foo")
        );
        assert_eq!(
            Result::<_, DecodeErrorKind>::Ok(("foo", HasExtensions)).canonical_with_extensions(),
            Ok("foo")
        );
        assert_eq!(
            Result::<_, DecodeErrorKind>::Ok(("foo", NotCanonical)).canonical_with_extensions(),
            Err(DecodeErrorKind::NotCanonical)
        );
        assert_eq!(
            Result::<_, DecodeErrorKind>::Ok(("foo", Canonical)).value(),
            Ok("foo")
        );
        assert_eq!(
            Result::<_, DecodeErrorKind>::Ok(("foo", HasExtensions)).value(),
            Ok("foo")
        );
        assert_eq!(
            Result::<_, DecodeErrorKind>::Ok(("foo", NotCanonical)).value(),
            Ok("foo")
        );

        // `Result<&(T, Canonicity), &DecodeErrorKind>` with Ok
        assert_eq!(
            Result::<_, DecodeErrorKind>::Ok(("foo", Canonical))
                .as_ref()
                .canonical(),
            Ok(&"foo")
        );
        assert_eq!(
            Result::<_, DecodeErrorKind>::Ok(("foo", HasExtensions))
                .as_ref()
                .canonical(),
            Err(DecodeErrorKind::UnknownField)
        );
        assert_eq!(
            Result::<_, DecodeErrorKind>::Ok(("foo", NotCanonical))
                .as_ref()
                .canonical(),
            Err(DecodeErrorKind::NotCanonical)
        );
        assert_eq!(
            Result::<_, DecodeErrorKind>::Ok(("foo", Canonical))
                .as_ref()
                .canonical_with_extensions(),
            Ok(&"foo")
        );
        assert_eq!(
            Result::<_, DecodeErrorKind>::Ok(("foo", HasExtensions))
                .as_ref()
                .canonical_with_extensions(),
            Ok(&"foo")
        );
        assert_eq!(
            Result::<_, DecodeErrorKind>::Ok(("foo", NotCanonical))
                .as_ref()
                .canonical_with_extensions(),
            Err(DecodeErrorKind::NotCanonical)
        );
        assert_eq!(
            Result::<_, DecodeErrorKind>::Ok(("foo", Canonical))
                .as_ref()
                .value(),
            Ok(&"foo")
        );
        assert_eq!(
            Result::<_, DecodeErrorKind>::Ok(("foo", HasExtensions))
                .as_ref()
                .value(),
            Ok(&"foo")
        );
        assert_eq!(
            Result::<_, DecodeErrorKind>::Ok(("foo", NotCanonical))
                .as_ref()
                .value(),
            Ok(&"foo")
        );

        // `Result<_, DecodeErrorKind>` with Err
        assert_eq!(
            Result::<Canonicity, DecodeErrorKind>::Err(DecodeErrorKind::Other).canonical(),
            Err(DecodeErrorKind::Other)
        );
        assert_eq!(
            Result::<Canonicity, DecodeErrorKind>::Err(DecodeErrorKind::Other)
                .canonical_with_extensions(),
            Err(DecodeErrorKind::Other)
        );
        assert_eq!(
            Result::<Canonicity, DecodeErrorKind>::Err(DecodeErrorKind::Other).value(),
            Err(DecodeErrorKind::Other)
        );

        // `Result<&_, &DecodeErrorKind>` with Err
        assert_eq!(
            Result::<Canonicity, DecodeErrorKind>::Err(DecodeErrorKind::Other)
                .as_ref()
                .canonical(),
            Err(DecodeErrorKind::Other)
        );
        assert_eq!(
            Result::<Canonicity, DecodeErrorKind>::Err(DecodeErrorKind::Other)
                .as_ref()
                .canonical_with_extensions(),
            Err(DecodeErrorKind::Other)
        );
        assert_eq!(
            Result::<Canonicity, DecodeErrorKind>::Err(DecodeErrorKind::Other)
                .as_ref()
                .value(),
            Err(DecodeErrorKind::Other)
        );
    }
}

/// Marker trait indicating that a type always decodes to its owned form. When implemented, borrowed
/// decoding will delegate to the owned implementation for the ValueDecoder traits for marked
/// encodings.
pub trait AlwaysOwned {}

/// Marker trait indicating that an encoder always delegate encoding for values marked with
/// AlwaysOwned.
pub trait AlwaysOwnedDelegatingEncoder {}

/// The core trait for encoding bilrost data.
pub trait Encoder<E> {
    /// Encodes the a field with the given tag and value.
    fn encode<B: BufMut + ?Sized>(tag: u32, value: &Self, buf: &mut B, tw: &mut TagWriter);

    /// Prepends the encoding of the field with the given tag and value.
    fn prepend_encode<B: ReverseBuf + ?Sized>(
        tag: u32,
        value: &Self,
        buf: &mut B,
        tw: &mut TagRevWriter,
    );

    /// Returns the encoded length of the field, including the key.
    fn encoded_len(tag: u32, value: &Self, tm: &mut impl TagMeasurer) -> usize;
}

// The core trait for decoding bilrost data. Data must always be copied from the buffer.
pub trait Decoder<E>: Encoder<E> {
    /// Decodes a field's value with the given wire type; the field's key should have already been
    /// consumed from the buffer.
    fn decode<B: Buf + ?Sized>(
        wire_type: WireType,
        duplicated: bool,
        value: &mut Self,
        buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError>;
}

/// Extension trait for canonical encoding and decoding. Distinguished decoding is available via
/// this trait, and any type that implements this trait is guaranteed to always emit canonical data
/// via `Encoder`.
pub trait DistinguishedDecoder<E>: Encoder<E> {
    /// Decodes a field for the value, returning a value indicating how canonical the encoding was.
    fn decode_distinguished<B: Buf + ?Sized>(
        wire_type: WireType,
        duplicated: bool,
        value: &mut Self,
        buf: Capped<B>,
        ctx: RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError>;
}

pub trait BorrowDecoder<'a, E>: Encoder<E> {
    fn borrow_decode(
        wire_type: WireType,
        duplicated: bool,
        value: &mut Self,
        buf: Capped<&'a [u8]>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError>;
}

pub trait DistinguishedBorrowDecoder<'a, E>: Encoder<E> {
    /// Decodes a field for the value, returning a value indicating how canonical the encoding was.
    fn borrow_decode_distinguished(
        wire_type: WireType,
        duplicated: bool,
        value: &mut Self,
        buf: Capped<&'a [u8]>,
        ctx: RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError>;
}

/// Encoders' wire-type is relied upon by both relaxed and distinguished encoders, but it is written
/// to be a separate trait so that distinguished decoders don't necessarily implement relaxed
/// decoding. This isn't important in general; it's very unlikely anything would implement
/// distinguished decoding without also implementing the corresponding relaxed decoding, but
/// this means that it can become a typo to use the relaxed decoding functions by accident when
/// implementing the distinguished decoders, which could cause serious mishaps.
pub trait Wiretyped<E> {
    const WIRE_TYPE: WireType;
}

/// The core trait for encoding implementations for raw values that always encode to a single value.
/// This is the basis for all the other plain, optional, and repeated encodings.
pub trait ValueEncoder<E>: Wiretyped<E> {
    /// Encodes the given value unconditionally. This is guaranteed to emit data to the buffer.
    fn encode_value<B: BufMut + ?Sized>(value: &Self, buf: &mut B);

    /// Prepends the given value unconditionally. This is guaranteed to emit data to the buffer.
    fn prepend_value<B: ReverseBuf + ?Sized>(value: &Self, buf: &mut B);

    /// Returns the number of bytes the given value would be encoded as.
    fn value_encoded_len(value: &Self) -> usize;

    /// Returns the number of total bytes to encode all the values in the given container.
    #[inline]
    fn many_values_encoded_len<I>(values: I) -> usize
    where
        I: ExactSizeIterator,
        I::Item: Deref<Target = Self>,
    {
        let len = values.len();
        Self::WIRE_TYPE.fixed_size().map_or_else(
            || values.map(|val| Self::value_encoded_len(&val)).sum(),
            |fixed_size| fixed_size * len, // Shortcut when values have a fixed size
        )
    }
}

/// The core trait for decoding single values in relaxed mode. Data is always copies when it is
/// read from the buffer.
pub trait ValueDecoder<E>: ValueEncoder<E> {
    /// Decodes a field assuming the encoder's wire type directly from the buffer.
    fn decode_value<B: Buf + ?Sized>(
        value: &mut Self,
        buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError>;
}

/// The core trait for decoding single values in distinguished mode. Data is always copies when it
/// is read from the buffer.
pub trait DistinguishedValueDecoder<E>: ValueEncoder<E> + Eq {
    /// Indicates whether the `ALLOW_EMPTY` argument in `decode_value_distinguished` has any effect.
    /// Some decoder implementations can more cheaply determine whether they were empty during
    /// decoding, and will return `NotCanonical` if `ALLOW_EMPTY` was false; for these
    /// implementations, `CHECKS_EMPTY` should be set to `true`. When `CHECKS_EMPTY` is `false`, the
    /// caller must invoke `EmptyState::is_empty` after the call if empty states are non-canonical.
    const CHECKS_EMPTY: bool;

    /// Decodes a field assuming the encoder's wire type directly from the buffer, also performing
    /// any additional validation required to guarantee that the value would be re-encoded into the
    /// exact same bytes.
    fn decode_value_distinguished<const ALLOW_EMPTY: bool>(
        value: &mut Self,
        buf: Capped<impl Buf + ?Sized>,
        ctx: RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError>;
}

pub trait ValueBorrowDecoder<'a, E>: ValueEncoder<E> {
    /// Decodes a field assuming the encoder's wire type directly from the buffer.
    fn borrow_decode_value(
        value: &mut Self,
        buf: Capped<&'a [u8]>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError>;
}

pub trait DistinguishedValueBorrowDecoder<'a, E>: ValueEncoder<E> + Eq {
    /// Indicates whether the `ALLOW_EMPTY` argument in `decode_value_distinguished` has any effect.
    /// Some decoder implementations can more cheaply determine whether they were empty during
    /// decoding, and will return `NotCanonical` if `ALLOW_EMPTY` was false; for these
    /// implementations, `CHECKS_EMPTY` should be set to `true`. When `CHECKS_EMPTY` is `false`, the
    /// caller must invoke `EmptyState::is_empty` after the call if empty states are non-canonical.
    const CHECKS_EMPTY: bool;

    /// Decodes a field assuming the encoder's wire type directly from the buffer, also performing
    /// any additional validation required to guarantee that the value would be re-encoded into the
    /// exact same bytes.
    fn borrow_decode_value_distinguished<const ALLOW_EMPTY: bool>(
        value: &mut Self,
        buf: Capped<&'a [u8]>,
        ctx: RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError>;
}

impl<'a, E, T> ValueBorrowDecoder<'a, E> for T
where
    T: AlwaysOwned + ValueDecoder<E>,
    E: AlwaysOwnedDelegatingEncoder,
{
    #[inline]
    fn borrow_decode_value(
        value: &mut Self,
        buf: Capped<&'a [u8]>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        ValueDecoder::<E>::decode_value(value, buf, ctx)
    }
}

impl<'a, E, T> DistinguishedValueBorrowDecoder<'a, E> for T
where
    T: AlwaysOwned + DistinguishedValueDecoder<E>,
    E: AlwaysOwnedDelegatingEncoder,
{
    const CHECKS_EMPTY: bool = T::CHECKS_EMPTY;

    #[inline]
    fn borrow_decode_value_distinguished<const ALLOW_EMPTY: bool>(
        value: &mut Self,
        buf: Capped<&'a [u8]>,
        ctx: RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError> {
        DistinguishedValueDecoder::<E>::decode_value_distinguished::<ALLOW_EMPTY>(value, buf, ctx)
    }
}

/// Affiliated helper trait for ValueEncoder that provides obligate implementations for handling
/// field keys and wire types.
pub trait FieldEncoder<E>: ValueEncoder<E> {
    /// Encodes exactly one field with the given tag and value into the buffer.
    fn encode_field<B: BufMut + ?Sized>(tag: u32, value: &Self, buf: &mut B, tw: &mut TagWriter);

    /// Prepends exactly one field with the given tag and value into the buffer.
    fn prepend_field<B: ReverseBuf + ?Sized>(
        tag: u32,
        value: &Self,
        buf: &mut B,
        tw: &mut TagRevWriter,
    );

    /// Returns the encoded length of the field including its key.
    fn field_encoded_len(tag: u32, value: &Self, tm: &mut impl TagMeasurer) -> usize;
}

/// Affiliated helper trait for ValueDecoder that provides obligate implementations for handling
/// field keys and wire types.
pub trait FieldDecoder<E>: ValueDecoder<E> {
    /// Decodes a field directly from the buffer, also checking the wire type.
    fn decode_field<B: Buf + ?Sized>(
        wire_type: WireType,
        value: &mut Self,
        buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError>;
}

/// Affiliated helper trait for DistinguishedValueDecoder that provides obligate implementations for
/// handling field keys and wire types.
pub trait DistinguishedFieldDecoder<E>: DistinguishedValueDecoder<E> {
    /// Decodes a field directly from the buffer, also checking the wire type.
    fn decode_field_distinguished<const ALLOW_EMPTY: bool>(
        wire_type: WireType,
        value: &mut Self,
        buf: Capped<impl Buf + ?Sized>,
        ctx: RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError>;
}

/// Affiliated helper trait for BorrowDecoder that provides obligate implementations for handling
/// field keys and wire types.
pub trait FieldBorrowDecoder<'a, E>: ValueBorrowDecoder<'a, E> {
    /// Decodes a field directly from the buffer, also checking the wire type.
    fn borrow_decode_field(
        wire_type: WireType,
        value: &mut Self,
        buf: Capped<&'a [u8]>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError>;
}

/// Affiliated helper trait for DistinguishedBorrowDecoder that provides obligate implementations
/// for handling field keys and wire types.
pub trait DistinguishedFieldBorrowDecoder<'a, E>: DistinguishedValueBorrowDecoder<'a, E> {
    /// Decodes a field directly from the buffer, also checking the wire type.
    fn borrow_decode_field_distinguished<const ALLOW_EMPTY: bool>(
        wire_type: WireType,
        value: &mut Self,
        buf: Capped<&'a [u8]>,
        ctx: RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError>;
}

impl<T, E> FieldEncoder<E> for T
where
    T: ValueEncoder<E>,
{
    #[inline]
    fn encode_field<B: BufMut + ?Sized>(tag: u32, value: &Self, buf: &mut B, tw: &mut TagWriter) {
        tw.encode_key(tag, Self::WIRE_TYPE, buf);
        Self::encode_value(value, buf);
    }

    #[inline]
    fn prepend_field<B: ReverseBuf + ?Sized>(
        tag: u32,
        value: &Self,
        buf: &mut B,
        tw: &mut TagRevWriter,
    ) {
        tw.begin_field(tag, Self::WIRE_TYPE, buf);
        Self::prepend_value(value, buf);
    }

    #[inline]
    fn field_encoded_len(tag: u32, value: &Self, tm: &mut impl TagMeasurer) -> usize {
        tm.key_len(tag) + Self::value_encoded_len(value)
    }
}

impl<T, E> FieldDecoder<E> for T
where
    T: ValueDecoder<E>,
{
    #[inline]
    fn decode_field<B: Buf + ?Sized>(
        wire_type: WireType,
        value: &mut Self,
        buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        check_wire_type(Self::WIRE_TYPE, wire_type)?;
        Self::decode_value(value, buf, ctx)
    }
}

impl<T, E> DistinguishedFieldDecoder<E> for T
where
    T: DistinguishedValueDecoder<E>,
{
    #[inline(always)]
    fn decode_field_distinguished<const ALLOW_EMPTY: bool>(
        wire_type: WireType,
        value: &mut T,
        buf: Capped<impl Buf + ?Sized>,
        ctx: RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError> {
        check_wire_type(Self::WIRE_TYPE, wire_type)?;
        Self::decode_value_distinguished::<ALLOW_EMPTY>(value, buf, ctx)
    }
}

impl<'a, T, E> FieldBorrowDecoder<'a, E> for T
where
    T: ValueBorrowDecoder<'a, E>,
{
    #[inline]
    fn borrow_decode_field(
        wire_type: WireType,
        value: &mut Self,
        buf: Capped<&'a [u8]>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        check_wire_type(Self::WIRE_TYPE, wire_type)?;
        Self::borrow_decode_value(value, buf, ctx)
    }
}

impl<'a, T, E> DistinguishedFieldBorrowDecoder<'a, E> for T
where
    T: DistinguishedValueBorrowDecoder<'a, E>,
{
    #[inline(always)]
    fn borrow_decode_field_distinguished<const ALLOW_EMPTY: bool>(
        wire_type: WireType,
        value: &mut T,
        buf: Capped<&'a [u8]>,
        ctx: RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError> {
        check_wire_type(Self::WIRE_TYPE, wire_type)?;
        Self::borrow_decode_value_distinguished::<ALLOW_EMPTY>(value, buf, ctx)
    }
}

/// Different value encoders may dispatch encoding their plain values slightly differently, but
/// values wrapped in Option are always encoded & decoded the same.
///
/// This would perhaps, in theory, need to be broken up if a value type whose values may be encoded
/// with different wire-types could be implemented. However, this can never happen: It is
/// essentially forbidden for any type to value-encode with differing wire types, because *value*
/// decoding does not get to know the wire type; when values are encoded packed end to end the wire
/// type for each is not stored.
mod generic_optional {
    use super::*;

    impl<T, E> Encoder<E> for Option<T>
    where
        T: ForOverwrite + ValueEncoder<E>,
    {
        #[inline]
        fn encode<B: BufMut + ?Sized>(tag: u32, value: &Self, buf: &mut B, tw: &mut TagWriter) {
            if let Some(value) = value {
                <T as FieldEncoder<E>>::encode_field(tag, value, buf, tw);
            }
        }

        #[inline]
        fn prepend_encode<B: ReverseBuf + ?Sized>(
            tag: u32,
            value: &Self,
            buf: &mut B,
            tw: &mut TagRevWriter,
        ) {
            if let Some(value) = value {
                <T as FieldEncoder<E>>::prepend_field(tag, value, buf, tw)
            }
        }

        #[inline]
        fn encoded_len(tag: u32, value: &Self, tm: &mut impl TagMeasurer) -> usize {
            if let Some(value) = value {
                <T as FieldEncoder<E>>::field_encoded_len(tag, value, tm)
            } else {
                0
            }
        }
    }

    impl<T, E> Decoder<E> for Option<T>
    where
        T: ForOverwrite + ValueDecoder<E>,
    {
        #[inline]
        fn decode<B: Buf + ?Sized>(
            wire_type: WireType,
            duplicated: bool,
            value: &mut Self,
            buf: Capped<B>,
            ctx: DecodeContext,
        ) -> Result<(), DecodeError> {
            if duplicated {
                return Err(DecodeError::new(UnexpectedlyRepeated));
            }
            <T as FieldDecoder<E>>::decode_field(
                wire_type,
                value.get_or_insert_with(T::for_overwrite),
                buf,
                ctx,
            )
        }
    }

    impl<T, E> DistinguishedDecoder<E> for Option<T>
    where
        Self: Decoder<E>,
        T: DistinguishedValueDecoder<E> + ForOverwrite + Eq,
    {
        #[inline]
        fn decode_distinguished<B: Buf + ?Sized>(
            wire_type: WireType,
            duplicated: bool,
            value: &mut Option<T>,
            buf: Capped<B>,
            ctx: RestrictedDecodeContext,
        ) -> Result<Canonicity, DecodeError> {
            if duplicated {
                return Err(DecodeError::new(UnexpectedlyRepeated));
            }
            check_wire_type(T::WIRE_TYPE, wire_type)?;
            T::decode_value_distinguished::<true>(
                value.get_or_insert_with(T::for_overwrite),
                buf,
                ctx,
            )
        }
    }

    impl<'a, T, E> BorrowDecoder<'a, E> for Option<T>
    where
        T: ForOverwrite + ValueBorrowDecoder<'a, E>,
    {
        #[inline]
        fn borrow_decode(
            wire_type: WireType,
            duplicated: bool,
            value: &mut Self,
            buf: Capped<&'a [u8]>,
            ctx: DecodeContext,
        ) -> Result<(), DecodeError> {
            if duplicated {
                return Err(DecodeError::new(UnexpectedlyRepeated));
            }
            <T as FieldBorrowDecoder<E>>::borrow_decode_field(
                wire_type,
                value.get_or_insert_with(T::for_overwrite),
                buf,
                ctx,
            )
        }
    }

    impl<'a, T, E> DistinguishedBorrowDecoder<'a, E> for Option<T>
    where
        Self: Decoder<E>,
        T: DistinguishedValueBorrowDecoder<'a, E> + ForOverwrite + Eq,
    {
        #[inline]
        fn borrow_decode_distinguished(
            wire_type: WireType,
            duplicated: bool,
            value: &mut Option<T>,
            buf: Capped<&'a [u8]>,
            ctx: RestrictedDecodeContext,
        ) -> Result<Canonicity, DecodeError> {
            if duplicated {
                return Err(DecodeError::new(UnexpectedlyRepeated));
            }
            check_wire_type(T::WIRE_TYPE, wire_type)?;
            T::borrow_decode_value_distinguished::<true>(
                value.get_or_insert_with(T::for_overwrite),
                buf,
                ctx,
            )
        }
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

/// Macro rules for expressly delegating from one encoder to another.
macro_rules! delegate_encoding {
    (
        delegate from ($from_ty:ty) to ($to_ty:ty) for type ($value_ty:ty)
        $(with where clause ($($where_clause:tt)*))?
        $(with generics ($($value_generics:tt)*))?
    ) => {
        impl$(<$($value_generics)*>)? $crate::encoding::Encoder<$from_ty> for $value_ty
        where
            Self: $crate::encoding::Encoder<$to_ty>,
            $($($where_clause)*)?
        {
            #[inline(always)]
            fn encode<B: $crate::bytes::BufMut + ?Sized>(
                tag: u32,
                value: &$value_ty,
                buf: &mut B,
                tw: &mut $crate::encoding::TagWriter,
            ) {
                $crate::encoding::Encoder::<$to_ty>::encode(tag, value, buf, tw)
            }

            #[inline(always)]
            fn prepend_encode<B: $crate::buf::ReverseBuf + ?Sized>(
                tag: u32,
                value: &$value_ty,
                buf: &mut B,
                tw: &mut $crate::encoding::TagRevWriter,
            ) {
                $crate::encoding::Encoder::<$to_ty>::prepend_encode(tag, value, buf, tw)
            }

            #[inline(always)]
            fn encoded_len(
                tag: u32,
                value: &$value_ty,
                tm: &mut impl $crate::encoding::TagMeasurer,
            ) -> usize {
                $crate::encoding::Encoder::<$to_ty>::encoded_len(tag, value, tm)
            }
        }

        impl$(<$($value_generics)*>)? $crate::encoding::Decoder<$from_ty> for $value_ty
        where
            Self: $crate::encoding::Decoder<$to_ty>,
            $($($where_clause)*)?
        {
            #[inline(always)]
            fn decode<B: $crate::bytes::Buf + ?Sized>(
                wire_type: $crate::encoding::WireType,
                duplicated: bool,
                value: &mut $value_ty,
                buf: $crate::encoding::Capped<B>,
                ctx: $crate::encoding::DecodeContext,
            ) -> Result<(), $crate::DecodeError> {
                $crate::encoding::Decoder::<$to_ty>::decode(
                    wire_type,
                    duplicated,
                    value,
                    buf,
                    ctx,
                )
            }
        }

        impl<'__a$(, $($value_generics)*)?>
        $crate::encoding::BorrowDecoder<'__a, $from_ty> for $value_ty
        where
            Self: $crate::encoding::BorrowDecoder<'__a, $to_ty>,
            $($($where_clause)*)?
        {
            #[inline(always)]
            fn borrow_decode(
                wire_type: $crate::encoding::WireType,
                duplicated: bool,
                value: &mut $value_ty,
                buf: $crate::encoding::Capped<&'__a [u8]>,
                ctx: $crate::encoding::DecodeContext,
            ) -> Result<(), $crate::DecodeError> {
                $crate::encoding::BorrowDecoder::<$to_ty>::borrow_decode(
                    wire_type,
                    duplicated,
                    value,
                    buf,
                    ctx,
                )
            }
        }
    };

    (
        delegate from ($from_ty:ty) to ($to_ty:ty) for type ($value_ty:ty) including distinguished
        $(with where clause ($($where_clause:tt)*))?
        $(with generics ($($value_generics:tt)*))?
    ) => {
        delegate_encoding!(
            delegate from ($from_ty) to ($to_ty) for type ($value_ty)
            $(with where clause ($($where_clause)*))?
            $(with generics ($($value_generics)*))?
        );

        impl$(<$($value_generics)*>)? $crate::encoding::DistinguishedDecoder<$from_ty>
        for $value_ty
        where
            Self: $crate::encoding::DistinguishedDecoder<$to_ty>
                + $crate::encoding::Encoder<$to_ty>,
            $($($where_clause)*)?
        {
            #[inline(always)]
            fn decode_distinguished<B: $crate::bytes::Buf + ?Sized>(
                wire_type: $crate::encoding::WireType,
                duplicated: bool,
                value: &mut $value_ty,
                buf: $crate::encoding::Capped<B>,
                ctx: $crate::encoding::RestrictedDecodeContext,
            ) -> Result<$crate::Canonicity, $crate::DecodeError> {
                $crate::encoding::DistinguishedDecoder::<$to_ty>::decode_distinguished(
                    wire_type,
                    duplicated,
                    value,
                    buf,
                    ctx,
                )
            }
        }

        impl<'__a$(, $($value_generics)*)?>
        $crate::encoding::DistinguishedBorrowDecoder<'__a, $from_ty>
        for $value_ty
        where
            Self: $crate::encoding::DistinguishedBorrowDecoder<'__a, $to_ty>
                + $crate::encoding::Encoder<$to_ty>,
            $($($where_clause)*)?
        {
            #[inline(always)]
            fn borrow_decode_distinguished(
                wire_type: $crate::encoding::WireType,
                duplicated: bool,
                value: &mut $value_ty,
                buf: $crate::encoding::Capped<&'__a [u8]>,
                ctx: $crate::encoding::RestrictedDecodeContext,
            ) -> Result<$crate::Canonicity, $crate::DecodeError> {
                $crate::encoding::DistinguishedBorrowDecoder::<$to_ty>::borrow_decode_distinguished(
                    wire_type,
                    duplicated,
                    value,
                    buf,
                    ctx,
                )
            }
        }
    };
}
pub(crate) use delegate_encoding;

/// This macro creates delegated `ValueEncoder` impls for a given type from one encoder to another.
macro_rules! delegate_value_encoding {
    (
        delegate from ($from_ty:ty) to ($to_ty:ty) for type ($value_ty:ty)
        $(with where clause ($($where_clause:tt)+))?
        $(with generics ($($value_generics:tt)*))?
    ) => {
        impl$(<$($value_generics)*>)? $crate::encoding::Wiretyped<$from_ty> for $value_ty
        where
            Self: $crate::encoding::Wiretyped<$to_ty>,
            $($($where_clause)+ ,)?
        {
            const WIRE_TYPE: $crate::encoding::WireType =
                <Self as $crate::encoding::Wiretyped<$to_ty>>::WIRE_TYPE;
        }

        impl$(<$($value_generics)*>)? $crate::encoding::ValueEncoder<$from_ty> for $value_ty
        where
            Self: $crate::encoding::ValueEncoder<$to_ty>,
            $($($where_clause)+ ,)?
        {
            #[inline(always)]
            fn encode_value<__B: $crate::bytes::BufMut + ?Sized>(value: &$value_ty, buf: &mut __B) {
                $crate::encoding::ValueEncoder::<$to_ty>::encode_value(value, buf)
            }

            #[inline(always)]
            fn prepend_value<__B: $crate::buf::ReverseBuf + ?Sized>(
                value: &$value_ty,
                buf: &mut __B,
            ) {
                $crate::encoding::ValueEncoder::<$to_ty>::prepend_value(value, buf)
            }

            #[inline(always)]
            fn value_encoded_len(value: &$value_ty) -> usize {
                $crate::encoding::ValueEncoder::<$to_ty>::value_encoded_len(value)
            }

            #[inline(always)]
            fn many_values_encoded_len<__I>(values: __I) -> usize
            where
                __I: ExactSizeIterator,
                __I::Item: core::ops::Deref<Target = $value_ty>,
            {
                $crate::encoding::ValueEncoder::<$to_ty>::many_values_encoded_len(values)
            }
        }

        impl$(<$($value_generics)*>)? $crate::encoding::ValueDecoder<$from_ty> for $value_ty
        where
            Self: $crate::encoding::ValueDecoder<$to_ty>,
            $($($where_clause)+ ,)?
        {
            #[inline(always)]
            fn decode_value<__B: $crate::bytes::Buf + ?Sized>(
                value: &mut $value_ty,
                buf: $crate::encoding::Capped<__B>,
                ctx: $crate::encoding::DecodeContext,
            ) -> Result<(), $crate::DecodeError> {
                $crate::encoding::ValueDecoder::<$to_ty>::decode_value(value, buf, ctx)
            }
        }
        // TODO(widders): borrowed
    };

    (
        delegate from ($from_ty:ty) to ($to_ty:ty) for type ($value_ty:ty) including distinguished
        $(with where clause for relaxed ($($relaxed_where:tt)+))?
        $(with where clause for distinguished ($($distinguished_where:tt)+))?
        $(with generics ($($value_generics:tt)*))?
    ) => {
        delegate_value_encoding!(
            delegate from ($from_ty) to ($to_ty) for type ($value_ty)
            $(with where clause ($($relaxed_where)+))?
            $(with generics ($($value_generics)*))?
        );

        impl$(<$($value_generics)*>)? $crate::encoding::DistinguishedValueDecoder<$from_ty>
        for $value_ty
        where
            Self: $crate::encoding::DistinguishedValueDecoder<$to_ty>,
            $($($relaxed_where)+ ,)?
            $($($distinguished_where)+ ,)?
        {
            const CHECKS_EMPTY: bool =
                <$value_ty as $crate::encoding::DistinguishedValueDecoder<$to_ty>>::CHECKS_EMPTY;

            #[inline(always)]
            fn decode_value_distinguished<const ALLOW_EMPTY: bool>(
                value: &mut $value_ty,
                buf: $crate::encoding::Capped<impl $crate::bytes::Buf + ?Sized>,
                ctx: $crate::encoding::RestrictedDecodeContext,
            ) -> Result<$crate::Canonicity, $crate::DecodeError> {
                $crate::encoding::DistinguishedValueDecoder::<$to_ty>::
                    decode_value_distinguished::<ALLOW_EMPTY>
                (
                    value,
                    buf,
                    ctx,
                )
            }
        }
        // TODO(widders): borrowed
    };
}
pub(crate) use delegate_value_encoding;

/// Most kinds of encodings want to act as field decoders for bare values in any situation where
/// they also implement value decoding. Only a couple encodings want to do anything fancy, like
/// accepting alternate wire-types in relaxed mode; the rest want to use this to blanket those
/// definitions.
macro_rules! __impl_decoder_where_value_decoder {
    (
        mode: $mode:ident,
        relaxed: $relaxed:ident::$relaxed_method:ident,
        relaxed_value: $relaxed_value:ident::$relaxed_value_method:ident,
        relaxed_field: $relaxed_field:ident::$relaxed_field_method:ident,
        distinguished: $distinguished:ident::$distinguished_method:ident,
        distinguished_value: $distinguished_value:ident::$distinguished_value_method:ident,
        distinguished_field: $distinguished_field:ident::$distinguished_field_method:ident,
        buf_ty: $buf_ty:ty,
        impl_buf_ty: $impl_buf_ty:ty,
        $(buf_generic: ($($buf_generic:tt)*),)?
        $(lifetime: $lifetime:lifetime,)?
        encoding: $encoding:ty,
        $(with where clause ($($where_clause:tt)*),)?
        $(with generics ($($generics:tt)*),)?
    ) => {
        /// Decodes plain values encoded as whole fields.
        impl<$($lifetime,)? T $(, $($generics)*)?>
        $crate::encoding::$relaxed <$($lifetime,)? $encoding> for T
        where
            T: $crate::encoding::EmptyState
                + $crate::encoding::$relaxed_value <$($lifetime,)? $encoding>,
            $($($where_clause)*)?
        {
            #[inline(always)]
            fn $relaxed_method $($($buf_generic)*)? (
                wire_type: WireType,
                duplicated: bool,
                value: &mut T,
                buf: Capped<$buf_ty>,
                ctx: DecodeContext,
            ) -> Result<(), $crate::DecodeError> {
                if duplicated {
                    return Err(
                        $crate::DecodeError::new($crate::DecodeErrorKind::UnexpectedlyRepeated)
                    );
                }
                $crate::encoding::$relaxed_field::<$encoding>::$relaxed_field_method(
                    wire_type, value, buf, ctx)
            }
        }

        /// Canonical encoding for plain values forbids encoding empty values. This includes
        /// directly-nested message types, which are not emitted when all their fields are default.
        /// If an empty value is decoded it is considered fully non-canonical.
        impl<$($lifetime,)? T $(, $($generics)*)?>
        $crate::encoding::$distinguished <$($lifetime,)? $encoding> for T
        where
            T: Eq
                + $crate::encoding::EmptyState
                + $crate::encoding::$distinguished_value <$($lifetime,)? $encoding>,
            $($($where_clause)*)?
        {
            #[inline(always)]
            fn $distinguished_method $($($buf_generic)*)? (
                wire_type: $crate::encoding::WireType,
                duplicated: bool,
                value: &mut T,
                buf: $crate::encoding::Capped<$buf_ty>,
                ctx: $crate::encoding::RestrictedDecodeContext,
            ) -> Result<$crate::Canonicity, $crate::DecodeError> {
                if duplicated {
                    return Err(
                        $crate::DecodeError::new(crate::DecodeErrorKind::UnexpectedlyRepeated)
                    );
                }
                // decoding a value as a whole message field, empty values are unacceptable
                let mut canon = $crate::encoding::$distinguished_field::<$encoding>
                    ::$distinguished_field_method::<false>(
                        wire_type,
                        value,
                        buf,
                        ctx.clone(),
                    )?;
                if !T::CHECKS_EMPTY && value.is_empty() {
                    ctx.update(&mut canon, crate::Canonicity::NotCanonical)?;
                }
                Ok(canon)
            }
        }
    };
}
pub(crate) use __impl_decoder_where_value_decoder;

macro_rules! decoder_where_value_decoder {
    (
        $encoding:ty
        $(, with where clause ($($where_clause:tt)*))?
        $(, with generics ($($generics:tt)*))?
    ) => {
        /// Encodes plain values only when they are non-default.
        impl<T $(, $($generics)*)?> $crate::encoding::Encoder<$encoding> for T
        where
            T: $crate::encoding::EmptyState + $crate::encoding::ValueEncoder<$encoding>,
            $($($where_clause)*)?
        {
            #[inline(always)]
            fn encode<B: BufMut + ?Sized>(
                tag: u32,
                value: &T,
                buf: &mut B,
                tw: &mut $crate::encoding::TagWriter,
            ) {
                if !$crate::encoding::EmptyState::is_empty(value) {
                    $crate::encoding::FieldEncoder::<$encoding>::encode_field(
                        tag, value, buf, tw);
                }
            }

            #[inline(always)]
            fn prepend_encode<B: $crate::buf::ReverseBuf + ?Sized>(
                tag: u32,
                value: &T,
                buf: &mut B,
                tw: &mut $crate::encoding::TagRevWriter,
            ) {
                if !$crate::encoding::EmptyState::is_empty(value) {
                    $crate::encoding::FieldEncoder::<$encoding>::prepend_field(
                        tag, value, buf, tw);
                }
            }

            #[inline(always)]
            fn encoded_len(
                tag: u32,
                value: &T,
                tm: &mut impl $crate::encoding::TagMeasurer,
            ) -> usize {
                if !$crate::encoding::EmptyState::is_empty(value) {
                    $crate::encoding::FieldEncoder::<$encoding>::field_encoded_len(
                        tag, value, tm)
                } else {
                    0
                }
            }
        }

        $crate::encoding::decoding_modes::invoke!(
            $crate::encoding::__impl_decoder_where_value_decoder,
            owned,
            encoding: $encoding,
            $(with where clause ($($where_clause)*),)?
            $(with generics ($($generics)*),)?
        );
        $crate::encoding::decoding_modes::invoke!(
            $crate::encoding::__impl_decoder_where_value_decoder,
            borrowed,
            encoding: $encoding,
            $(with where clause ($($where_clause)*),)?
            $(with generics ($($generics)*),)?
        );
    };
}
pub(crate) use decoder_where_value_decoder;

#[cfg(test)]
mod test {
    use alloc::collections::{BTreeMap, BTreeSet};
    use alloc::format;
    use alloc::string::{String, ToString};
    use alloc::vec::Vec;
    use core::borrow::Borrow;
    use core::fmt::Debug;

    use proptest::{prelude::*, test_runner::TestCaseResult};

    use super::*;
    use crate::Blob;
    use crate::DecodeErrorKind::OutOfDomainValue;

    /// Generalized proptest macro. Kind must be either `relaxed` or `distinguished`.
    macro_rules! check_type_test {
        ($encoder:ty, $kind:ident, $ty:ty, $wire_type:expr) => {
            crate::encoding::test::check_type_test!($encoder, $kind, from $ty, into $ty,
            converter(value) { value }, $wire_type);
        };
        ($encoder:ty, $kind:ident, from $from_ty:ty, into $into_ty:ty, $wire_type:expr) => {
            crate::encoding::test::check_type_test!($encoder, $kind, from $from_ty, into $into_ty,
                converter(value) { <$into_ty>::from(value) }, $wire_type);
        };
        (
            $encoder:ty,
            $kind:ident,
            from $from_ty:ty,
            into $into_ty:ty,
            converter($from_value:ident) $convert:expr,
            $wire_type:expr
        ) => {
            #[cfg(test)]
            mod $kind {
                use proptest::prelude::*;

                use crate::encoding::test::$kind::check_type;
                #[allow(unused_imports)]
                use crate::encoding::WireType;
                #[allow(unused_imports)]
                use super::*;

                proptest! {
                    #[test]
                    fn check($from_value: $from_ty, tag: u32) {
                        check_type::<$into_ty, $encoder>($convert, tag, $wire_type)?;
                    }
                    #[test]
                    fn check_optional(opt_value: Option<$from_ty>, tag: u32) {
                        check_type::<Option<$into_ty>, $encoder>(
                            opt_value.map(|$from_value| $convert),
                            tag,
                            $wire_type,
                        )?;
                    }
                }
            }
        };
    }
    pub(crate) use check_type_test;

    fn check_legal_remaining(tag: u32, wire_type: WireType, remaining: usize) -> TestCaseResult {
        match wire_type {
            WireType::SixtyFourBit => 8..=8,
            WireType::ThirtyTwoBit => 4..=4,
            WireType::Varint => 1..=9,
            WireType::LengthDelimited => 1..=usize::MAX,
        }
        .contains(&remaining)
        .then_some(())
        .ok_or_else(|| {
            TestCaseError::fail(format!(
                "{wire_type:?} wire type illegal remaining: {remaining}, tag: {tag}"
            ))
        })
    }

    macro_rules! check_type {
        ($kind:ident, $decoder_trait:ident, $context:expr, $decode:ident) => {
            pub mod $kind {
                use super::*;
                use crate::buf::ReverseBuffer;

                pub fn check_type<T, E>(value: T, tag: u32, wire_type: WireType) -> TestCaseResult
                where
                    T: Debug + ForOverwrite + PartialEq + $decoder_trait<E>,
                {
                    let expected_len =
                        <T as Encoder<E>>::encoded_len(tag, &value, &mut RuntimeTagMeasurer::new());

                    let mut forward_encoded = Vec::with_capacity(expected_len);
                    <T as Encoder<E>>::encode(
                        tag,
                        &value,
                        &mut forward_encoded,
                        &mut TagWriter::new(),
                    );
                    prop_assert_eq!(
                        expected_len,
                        forward_encoded.len(),
                        "forward encoded length was wrong"
                    );

                    let mut prepend_buf = ReverseBuffer::new();
                    let mut trw = TagRevWriter::new();
                    <T as Encoder<E>>::prepend_encode(tag, &value, &mut prepend_buf, &mut trw);
                    trw.finalize(&mut prepend_buf);
                    prop_assert_eq!(
                        expected_len,
                        prepend_buf.len(),
                        "prepend encoded length was wrong"
                    );

                    let mut prepended = Vec::new();
                    prepended.put(prepend_buf);

                    if check_type_prepend_must_match_forward::$kind::VALUE {
                        prop_assert_eq!(
                            &forward_encoded,
                            &prepended,
                            "prepend did not match append",
                        );
                    }

                    if forward_encoded.len() == 0 {
                        // Short circuit for omitted fields, which do not get decoded.
                        return Ok(());
                    }

                    for encoded in [forward_encoded, prepended] {
                        let mut slice = encoded.as_slice();
                        let mut buf = Capped::new(&mut slice);
                        let mut tr = TagReader::new();

                        let (decoded_tag, decoded_wire_type) = tr
                            .decode_key(buf.lend())
                            .map_err(|error| TestCaseError::fail(error.to_string()))?;
                        prop_assert_eq!(
                            tag,
                            decoded_tag,
                            "decoded tag does not match; expected: {}, actual: {}",
                            tag,
                            decoded_tag
                        );

                        prop_assert_eq!(
                            wire_type,
                            decoded_wire_type,
                            "decoded wire type does not match; expected: {:?}, actual: {:?}",
                            wire_type,
                            decoded_wire_type,
                        );

                        check_legal_remaining(tag, wire_type, buf.remaining())?;

                        let mut roundtrip_value = T::for_overwrite();
                        _ = <T as $decoder_trait<E>>::$decode(
                            wire_type,
                            false,
                            &mut roundtrip_value,
                            buf.lend(),
                            $context,
                        )
                        .map_err(|error| TestCaseError::fail(error.to_string()))?;

                        prop_assert!(
                            !buf.remaining() > 0,
                            "expected buffer to be empty, remaining: {}",
                            buf.remaining()
                        );

                        prop_assert_eq!(&value, &roundtrip_value);
                    }

                    Ok(())
                }

                pub fn check_type_unpacked<T, E>(
                    value: T,
                    tag: u32,
                    wire_type: WireType,
                ) -> TestCaseResult
                where
                    T: Debug + ForOverwrite + PartialEq + $decoder_trait<E>,
                {
                    let expected_len = <T as Encoder<E>>::encoded_len(
                        tag,
                        value.borrow(),
                        &mut RuntimeTagMeasurer::new(),
                    );

                    let mut forward_encoded = Vec::with_capacity(expected_len);
                    <T as Encoder<E>>::encode(
                        tag,
                        value.borrow(),
                        &mut forward_encoded,
                        &mut TagWriter::new(),
                    );

                    prop_assert_eq!(
                        expected_len,
                        forward_encoded.len(),
                        "forward encoded length was wrong",
                    );

                    let mut prepend_buf = ReverseBuffer::new();
                    let mut trw = TagRevWriter::new();
                    <T as Encoder<E>>::prepend_encode(tag, &value, &mut prepend_buf, &mut trw);
                    trw.finalize(&mut prepend_buf);
                    prop_assert_eq!(
                        expected_len,
                        prepend_buf.len(),
                        "prepend encoded length was wrong",
                    );

                    let mut prepended = Vec::new();
                    prepended.put(prepend_buf);

                    if check_type_prepend_must_match_forward::$kind::VALUE {
                        prop_assert_eq!(
                            &forward_encoded,
                            &prepended,
                            "prepend did not match append",
                        );
                    }

                    if forward_encoded.len() == 0 {
                        // Short circuit for omitted fields, which do not get decoded.
                        return Ok(());
                    }

                    for encoded in [forward_encoded, prepended] {
                        let mut slice = encoded.as_slice();
                        let mut buf = Capped::new(&mut slice);
                        let mut tr = TagReader::new();

                        let mut roundtrip_value = T::for_overwrite();
                        let (decoded_tag, decoded_wire_type) = tr
                            .decode_key(buf.lend())
                            .map_err(|error| TestCaseError::fail(error.to_string()))?;

                        prop_assert_eq!(
                            tag,
                            decoded_tag,
                            "decoded tag does not match; expected: {}, actual: {}",
                            tag,
                            decoded_tag
                        );

                        prop_assert_eq!(
                            wire_type,
                            decoded_wire_type,
                            "decoded wire type does not match; expected: {:?}, actual: {:?}",
                            wire_type,
                            decoded_wire_type
                        );

                        _ = <T as $decoder_trait<E>>::$decode(
                            wire_type,
                            false,
                            &mut roundtrip_value,
                            buf.lend(),
                            $context,
                        )
                        .map_err(|error| TestCaseError::fail(error.to_string()))?;

                        prop_assert!(
                            !buf.remaining() > 0,
                            "expected buffer to be empty, remaining: {}",
                            buf.remaining()
                        );
                        prop_assert_eq!(&value, &roundtrip_value);
                    }

                    Ok(())
                }
            }
        };
    }
    // Non-distinguished types either contain floating-point numbers (which are prepend-encoded
    // trivially similarly to how they are forward-encoded) or hash-based collections and mappings.
    // The latter don't have distinct "reversed" iterators, so if they have multiple items they
    // won't necessarily prepend-encode the exact same bytes that they forward-encode and we needn't
    // assert that they do.
    mod check_type_prepend_must_match_forward {
        pub(crate) mod relaxed {
            pub(crate) const VALUE: bool = false;
        }
        pub(crate) mod distinguished {
            pub(crate) const VALUE: bool = true;
        }
    }
    check_type!(relaxed, Decoder, DecodeContext::default(), decode);
    check_type!(
        distinguished,
        DistinguishedDecoder,
        RestrictedDecodeContext::new(Canonicity::Canonical),
        decode_distinguished
    );

    /// Types that have an empty state should have a consistent implementation; this can be
    /// especially important for types that use proxied encoding, which may want to keep emptiness
    /// of the original and proxy types in sync. That's what this tests right now, though it may be
    /// possible that this could be loosened somewhat in the future. There's some possibility that
    /// eventually EmptyState may become subsidiary to value encoding rather than an enforced global
    /// semantic, but we don't need to get ahead of ourselves.
    macro_rules! check_type_empty {
        ($ty:ty) => {
            #[test]
            fn check_type_empty() {
                $crate::encoding::test::check_type_empty_impl::<$ty>();
            }
        };

        ($ty:ty, via proxy) => {
            #[test]
            fn check_type_empty_via_proxy() {
                $crate::encoding::test::check_type_empty_proxied_impl::<$ty>();
                $crate::encoding::test::check_proxy_round_trip::<$ty>();
            }
        };

        ($ty:ty, via distinguished proxy) => {
            #[test]
            fn check_type_empty_via_distinguished_proxy() {
                $crate::encoding::test::check_type_empty_proxied_impl::<$ty>();
                $crate::encoding::test::check_proxy_round_trip_distinguished::<$ty>();
            }
        };
    }
    pub(crate) use check_type_empty;

    pub(crate) fn check_type_empty_impl<T>()
    where
        T: Debug + EmptyState + PartialEq,
    {
        let mut empty = T::empty();
        assert!(empty.is_empty());
        empty.clear();
        assert!(empty.is_empty());
        assert_eq!(empty, T::empty());
    }

    pub(crate) fn check_type_empty_proxied_impl<T>()
    where
        T: Debug + EmptyState + PartialEq + Proxiable,
        T::Proxy: Debug + EmptyState + PartialEq,
    {
        check_type_empty_impl::<T>();
        check_type_empty_impl::<T::Proxy>();
    }

    pub(crate) fn check_proxy_round_trip<T>()
    where
        T: Debug + EmptyState + PartialEq + Proxiable,
        T::Proxy: Debug + EmptyState + PartialEq,
    {
        let start = T::empty();
        let proxy = start.encode_proxy();
        assert!(proxy.is_empty());
        let mut end = T::for_overwrite();
        end.decode_proxy(proxy).unwrap();
        assert!(end.is_empty());
        assert_eq!(start, end);
    }

    pub(crate) fn check_proxy_round_trip_distinguished<T>()
    where
        T: Debug + EmptyState + Eq + DistinguishedProxiable,
        T::Proxy: Debug + EmptyState + Eq,
    {
        let start = T::empty();
        let proxy = start.encode_proxy();
        assert!(proxy.is_empty());
        let mut end = T::for_overwrite();
        let canon = end.decode_proxy_distinguished(proxy).unwrap();
        assert_eq!(canon, Canonicity::Canonical);
        assert!(end.is_empty());
        assert_eq!(start, end);
    }

    fn present_empty_not_canon<T, E>()
    where
        T: EmptyState + Eq + DistinguishedDecoder<E> + ValueEncoder<E>,
    {
        let mut encoded = <Vec<u8>>::new();
        Encoder::<E>::encode(123, &Some(T::empty()), &mut encoded, &mut TagWriter::new());
        let mut buf = &*encoded;
        let mut capped = Capped::new(&mut buf);
        let (tag, wire_type) = TagReader::new().decode_key(capped.lend()).unwrap();
        assert_eq!(tag, 123);
        let mut decoded = T::for_overwrite();
        assert_eq!(
            DistinguishedDecoder::<E>::decode_distinguished(
                wire_type,
                false,
                &mut decoded,
                capped,
                RestrictedDecodeContext::new(Canonicity::NotCanonical),
            ),
            Ok(Canonicity::NotCanonical)
        );
        assert!(decoded.is_empty());
    }

    #[test]
    fn test_present_and_empty() {
        // Any value that's present not in an `Option` that is not omitted must err when decoded in
        // distinguished mode
        present_empty_not_canon::<u32, General>();
        present_empty_not_canon::<u64, General>();
        present_empty_not_canon::<i32, General>();
        present_empty_not_canon::<i64, General>();
        present_empty_not_canon::<u32, Fixed>();
        present_empty_not_canon::<u64, Fixed>();
        present_empty_not_canon::<i32, Fixed>();
        present_empty_not_canon::<i64, Fixed>();
        present_empty_not_canon::<bool, General>();
        present_empty_not_canon::<String, General>();
        present_empty_not_canon::<Blob, General>();
        present_empty_not_canon::<Vec<u8>, PlainBytes>();

        present_empty_not_canon::<Vec<u32>, Packed<General>>();
        present_empty_not_canon::<Vec<u64>, Packed<General>>();
        present_empty_not_canon::<Vec<i32>, Packed<General>>();
        present_empty_not_canon::<Vec<i64>, Packed<General>>();
        present_empty_not_canon::<Vec<u32>, Packed<Fixed>>();
        present_empty_not_canon::<Vec<u64>, Packed<Fixed>>();
        present_empty_not_canon::<Vec<i32>, Packed<Fixed>>();
        present_empty_not_canon::<Vec<i64>, Packed<Fixed>>();
        present_empty_not_canon::<Vec<bool>, Packed<General>>();
        present_empty_not_canon::<Vec<String>, Packed<General>>();
        present_empty_not_canon::<Vec<Blob>, Packed<General>>();
        present_empty_not_canon::<Vec<Vec<u8>>, Packed<PlainBytes>>();
        present_empty_not_canon::<[u32; 5], Packed<General>>();
        present_empty_not_canon::<[(u32, String); 5], Packed<General>>();

        present_empty_not_canon::<BTreeSet<u32>, Packed<General>>();
        present_empty_not_canon::<BTreeSet<u64>, Packed<General>>();
        present_empty_not_canon::<BTreeSet<i32>, Packed<General>>();
        present_empty_not_canon::<BTreeSet<i64>, Packed<General>>();
        present_empty_not_canon::<BTreeSet<u32>, Packed<Fixed>>();
        present_empty_not_canon::<BTreeSet<u64>, Packed<Fixed>>();
        present_empty_not_canon::<BTreeSet<i32>, Packed<Fixed>>();
        present_empty_not_canon::<BTreeSet<i64>, Packed<Fixed>>();
        present_empty_not_canon::<BTreeSet<bool>, Packed<General>>();
        present_empty_not_canon::<BTreeSet<String>, Packed<General>>();
        present_empty_not_canon::<BTreeSet<Blob>, Packed<General>>();
        present_empty_not_canon::<BTreeSet<Vec<u8>>, Packed<PlainBytes>>();

        present_empty_not_canon::<BTreeMap<u32, u32>, Map<General, General>>();
        present_empty_not_canon::<BTreeMap<u64, u64>, Map<General, General>>();
        present_empty_not_canon::<BTreeMap<i32, i32>, Map<General, General>>();
        present_empty_not_canon::<BTreeMap<i64, i64>, Map<General, General>>();
        present_empty_not_canon::<BTreeMap<u32, u32>, Map<Fixed, Fixed>>();
        present_empty_not_canon::<BTreeMap<u64, u64>, Map<Fixed, Fixed>>();
        present_empty_not_canon::<BTreeMap<i32, i32>, Map<Fixed, Fixed>>();
        present_empty_not_canon::<BTreeMap<i64, i64>, Map<Fixed, Fixed>>();
        present_empty_not_canon::<BTreeMap<bool, bool>, Map<General, General>>();
        present_empty_not_canon::<BTreeMap<String, String>, Map<General, General>>();
        present_empty_not_canon::<BTreeMap<Blob, Blob>, Map<General, General>>();
        present_empty_not_canon::<BTreeMap<Vec<u8>, Vec<u8>>, Map<PlainBytes, PlainBytes>>();

        present_empty_not_canon::<Vec<BTreeMap<u32, u32>>, Packed<Map<General, General>>>();
        present_empty_not_canon::<Vec<BTreeMap<u64, u64>>, Packed<Map<General, General>>>();
        present_empty_not_canon::<Vec<BTreeMap<i32, i32>>, Packed<Map<General, General>>>();
        present_empty_not_canon::<Vec<BTreeMap<i64, i64>>, Packed<Map<General, General>>>();
        present_empty_not_canon::<Vec<BTreeMap<u32, u32>>, Packed<Map<Fixed, Fixed>>>();
        present_empty_not_canon::<Vec<BTreeMap<u64, u64>>, Packed<Map<Fixed, Fixed>>>();
        present_empty_not_canon::<Vec<BTreeMap<i32, i32>>, Packed<Map<Fixed, Fixed>>>();
        present_empty_not_canon::<Vec<BTreeMap<i64, i64>>, Packed<Map<Fixed, Fixed>>>();
        present_empty_not_canon::<Vec<BTreeMap<bool, bool>>, Packed<Map<General, General>>>();
        present_empty_not_canon::<Vec<BTreeMap<String, String>>, Packed<Map<General, General>>>();
        present_empty_not_canon::<Vec<BTreeMap<Blob, Blob>>, Packed<Map<General, General>>>();
        present_empty_not_canon::<
            Vec<BTreeMap<Vec<u8>, Vec<u8>>>,
            Packed<Map<PlainBytes, PlainBytes>>,
        >();

        present_empty_not_canon::<(bool,), General>();
        present_empty_not_canon::<(bool, bool), General>();
        present_empty_not_canon::<(bool, bool, bool), General>();
        present_empty_not_canon::<(bool, bool, bool, bool), General>();
        present_empty_not_canon::<(bool, bool, bool, bool, bool), General>();
        present_empty_not_canon::<(bool, bool, bool, bool, bool, bool), General>();
        present_empty_not_canon::<(bool, bool, bool, bool, bool, bool, bool), General>();
        present_empty_not_canon::<(bool, bool, bool, bool, bool, bool, bool, bool), General>();
        present_empty_not_canon::<(bool, bool, bool, bool, bool, bool, bool, bool, bool), General>(
        );
        present_empty_not_canon::<(u16, u16, u16, u16, u16, u16, u16, u16, u16, u16), General>();
        present_empty_not_canon::<(u16, u16, u16, u16, u16, u16, u16, u16, u16, u16, u16), General>(
        );
        present_empty_not_canon::<
            (u16, u16, u16, u16, u16, u16, u16, u16, u16, u16, u16, u16),
            General,
        >();
        present_empty_not_canon::<(bool,), General>();
        present_empty_not_canon::<(bool, u32), General>();
        present_empty_not_canon::<(bool, bool, String), General>();
        present_empty_not_canon::<(bool, i64, Blob, bool), General>();
        present_empty_not_canon::<(bool, bool, bool, bool, bool), General>();
        present_empty_not_canon::<(bool, bool, (), bool, bool, bool), General>();
        present_empty_not_canon::<(bool, bool, bytes::Bytes, bool, bool, bool, bool), General>();
        present_empty_not_canon::<(bool, bool, u16, bool, i16, bool, bool, bool), General>();
        present_empty_not_canon::<(bool, String, bool, bool, bool, bool, bool, bool, bool), General>(
        );
        present_empty_not_canon::<(String, u16, u16, u16, u16, u16, i64, u16, u16, u16), General>();
        present_empty_not_canon::<(u16, u16, u16, bool, u16, u16, u16, u16, bool, u16, u16), General>(
        );
        present_empty_not_canon::<
            (
                u16,
                u16,
                u16,
                u16,
                u16,
                bool,
                u16,
                String,
                u64,
                u16,
                u16,
                u16,
            ),
            General,
        >();
    }

    #[test]
    fn unaligned_fixed64_packed() {
        // Construct a length-delineated field that is not a multiple of 8 bytes.
        let mut buf = Vec::<u8>::new();
        encode_varint(12, &mut buf);
        buf.extend([1; 12]);

        let mut parsed = Vec::<u64>::new();
        let res = ValueDecoder::<Packed<Fixed>>::decode_value(
            &mut parsed,
            Capped::new(&mut buf.as_slice()),
            DecodeContext::default(),
        );
        assert_eq!(
            res.expect_err("unaligned packed fixed64 decoded without error")
                .kind(),
            Truncated
        );
        let res = DistinguishedValueDecoder::<Packed<Fixed>>::decode_value_distinguished::<true>(
            &mut parsed,
            Capped::new(&mut buf.as_slice()),
            RestrictedDecodeContext::new(Canonicity::NotCanonical),
        );
        assert_eq!(
            res.expect_err("unaligned packed fixed64 decoded without error")
                .kind(),
            Truncated
        );
    }

    #[test]
    fn unaligned_fixed32_packed() {
        // Construct a length-delineated field that is not a multiple of 4 bytes.
        let mut buf = Vec::<u8>::new();
        encode_varint(17, &mut buf);
        buf.extend([1; 17]);

        let mut parsed = Vec::<u32>::new();
        let res = ValueDecoder::<Packed<Fixed>>::decode_value(
            &mut parsed,
            Capped::new(&mut buf.as_slice()),
            DecodeContext::default(),
        );
        assert_eq!(
            res.expect_err("unaligned packed fixed32 decoded without error")
                .kind(),
            Truncated
        );
        let res = DistinguishedValueDecoder::<Packed<Fixed>>::decode_value_distinguished::<true>(
            &mut parsed,
            Capped::new(&mut buf.as_slice()),
            RestrictedDecodeContext::new(Canonicity::NotCanonical),
        );
        assert_eq!(
            res.expect_err("unaligned packed fixed32 decoded without error")
                .kind(),
            Truncated
        );
    }

    #[test]
    fn unaligned_map_packed() {
        // Construct a length-delineated field that is not a multiple of the sum of fixed size key
        // and value in a map. In the case we are testing it is a fixed size 4+8 = 12 bytes per
        // entry.
        let mut buf = Vec::<u8>::new();
        encode_varint(16, &mut buf);
        buf.extend([1; 16]);

        // The entries for this map always consume 12 bytes each.
        let mut parsed = BTreeMap::<u32, u64>::new();
        let res = ValueDecoder::<Map<Fixed, Fixed>>::decode_value(
            &mut parsed,
            Capped::new(&mut buf.as_slice()),
            DecodeContext::default(),
        );
        assert_eq!(
            res.expect_err("unaligned 12-byte map decoded without error")
                .kind(),
            Truncated
        );
        let res = DistinguishedValueDecoder::<Map<Fixed, Fixed>>::decode_value_distinguished::<true>(
            &mut parsed,
            Capped::new(&mut buf.as_slice()),
            RestrictedDecodeContext::new(Canonicity::NotCanonical),
        );
        assert_eq!(
            res.expect_err("unaligned 12-byte map decoded without error")
                .kind(),
            Truncated
        );
    }

    #[test]
    fn string_merge_invalid_utf8() {
        let mut s = String::new();
        let buf = b"\x02\x80\x80";

        let r = ValueDecoder::<General>::decode_value(
            &mut s,
            Capped::new(&mut buf.as_slice()),
            DecodeContext::default(),
        );
        r.expect_err("must be an error");
        assert!(s.is_empty());
    }

    #[test]
    fn varint() {
        fn check(value: u64, encoded: &[u8]) {
            // Small buffer.
            let mut buf = Vec::with_capacity(1);
            encode_varint(value, &mut buf);
            assert_eq!(buf, encoded);

            // Large buffer.
            let mut buf = Vec::with_capacity(100);
            encode_varint(value, &mut buf);
            assert_eq!(buf, encoded);

            // Constant encoded.
            assert_eq!(const_varint(value).deref(), encoded);

            assert_eq!(encoded_len_varint(value), encoded.len());

            let roundtrip_value = decode_varint(&mut &*encoded).expect("decoding failed");
            assert_eq!(value, roundtrip_value);

            let roundtrip_value = decode_varint_slow(&mut &*encoded).expect("slow decoding failed");
            assert_eq!(value, roundtrip_value);
        }

        check(2u64.pow(0) - 1, &[0x00]);
        check(2u64.pow(0), &[0x01]);

        check(2u64.pow(7) - 1, &[0x7F]);
        check(256, &[0x80, 0x01]);
        check(128, &[0x80, 0x00]);
        check(300, &[0xAC, 0x01]);

        check(2u64.pow(14) - 1, &[0xFF, 0x7E]);
        check(2u64.pow(14), &[0x80, 0x7f]);

        check(0x407f, &[0xFF, 0x7F]);
        check(0x4080, &[0x80, 0x80, 0x00]);
        check(0x8080, &[0x80, 0x80, 0x01]);

        check(2u64.pow(21) - 1, &[0xFF, 0xFE, 0x7E]);
        check(2u64.pow(21), &[0x80, 0xFF, 0x7E]);

        check(0x20407f, &[0xFF, 0xFF, 0x7F]);
        check(0x204080, &[0x80, 0x80, 0x80, 0x00]);
        check(0x404080, &[0x80, 0x80, 0x80, 0x01]);

        check(2u64.pow(28) - 1, &[0xFF, 0xFE, 0xFE, 0x7E]);
        check(2u64.pow(28), &[0x80, 0xFF, 0xFE, 0x7E]);

        check(0x1020407f, &[0xFF, 0xFF, 0xFF, 0x7F]);
        check(0x10204080, &[0x80, 0x80, 0x80, 0x80, 0x00]);
        check(0x20204080, &[0x80, 0x80, 0x80, 0x80, 0x01]);

        check(2u64.pow(35) - 1, &[0xFF, 0xFE, 0xFE, 0xFE, 0x7E]);
        check(2u64.pow(35), &[0x80, 0xFF, 0xFE, 0xFE, 0x7E]);

        check(0x81020407f, &[0xFF, 0xFF, 0xFF, 0xFF, 0x7F]);
        check(0x810204080, &[0x80, 0x80, 0x80, 0x80, 0x80, 0x00]);
        check(0x1010204080, &[0x80, 0x80, 0x80, 0x80, 0x80, 0x01]);

        check(2u64.pow(42) - 1, &[0xFF, 0xFE, 0xFE, 0xFE, 0xFE, 0x7E]);
        check(2u64.pow(42), &[0x80, 0xFF, 0xFE, 0xFE, 0xFE, 0x7E]);

        check(0x4081020407f, &[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x7F]);
        check(0x40810204080, &[0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x00]);
        check(0x80810204080, &[0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x01]);

        check(
            2u64.pow(49) - 1,
            &[0xFF, 0xFE, 0xFE, 0xFE, 0xFE, 0xFE, 0x7E],
        );
        check(2u64.pow(49), &[0x80, 0xFF, 0xFE, 0xFE, 0xFE, 0xFE, 0x7E]);

        check(0x204081020407f, &[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x7F]);
        check(
            0x2040810204080,
            &[0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x00],
        );
        check(
            0x4040810204080,
            &[0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x01],
        );

        check(
            2u64.pow(56) - 1,
            &[0xFF, 0xFE, 0xFE, 0xFE, 0xFE, 0xFE, 0xFE, 0x7E],
        );
        check(
            2u64.pow(56),
            &[0x80, 0xFF, 0xFE, 0xFE, 0xFE, 0xFE, 0xFE, 0x7E],
        );

        check(
            0x10204081020407f,
            &[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x7F],
        );
        check(
            0x102040810204080,
            &[0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x00],
        );
        check(
            0x202040810204080,
            &[0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x01],
        );

        check(
            2u64.pow(63) - 1,
            &[0xFF, 0xFE, 0xFE, 0xFE, 0xFE, 0xFE, 0xFE, 0xFE, 0x7E],
        );
        check(
            2u64.pow(63),
            &[0x80, 0xFF, 0xFE, 0xFE, 0xFE, 0xFE, 0xFE, 0xFE, 0x7E],
        );

        check(
            0x810204081020407f,
            &[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x7F],
        );
        check(
            0x8102040810204080,
            &[0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80],
        );
        // check(
        //     0x10102040810204080, //
        //     &[0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x01],
        // );

        check(
            u64::MAX,
            &[0xFF, 0xFE, 0xFE, 0xFE, 0xFE, 0xFE, 0xFE, 0xFE, 0xFE],
        );
        check(
            i64::MAX as u64,
            &[0xFF, 0xFE, 0xFE, 0xFE, 0xFE, 0xFE, 0xFE, 0xFE, 0x7E],
        );
    }

    #[test]
    fn varint_overflow() {
        let u64_max_plus_one: &[u8] = &[0x80, 0xFF, 0xFE, 0xFE, 0xFE, 0xFE, 0xFE, 0xFE, 0xFE];

        assert_eq!(
            decode_varint(&mut &*u64_max_plus_one)
                .expect_err("decoding u64::MAX + 1 succeeded")
                .kind(),
            InvalidVarint
        );
        assert_eq!(
            decode_varint_slow(&mut &*u64_max_plus_one)
                .expect_err("slow decoding u64::MAX + 1 succeeded")
                .kind(),
            InvalidVarint
        );

        let u64_over_max: &[u8] = &[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];

        assert_eq!(
            decode_varint(&mut &*u64_over_max)
                .expect_err("decoding over-max succeeded")
                .kind(),
            InvalidVarint
        );
        assert_eq!(
            decode_varint_slow(&mut &*u64_over_max)
                .expect_err("slow decoding over-max succeeded")
                .kind(),
            InvalidVarint
        );
    }

    #[test]
    fn varint_truncated() {
        let truncated_one_byte: &[u8] = &[0x80];
        assert_eq!(
            decode_varint(&mut &*truncated_one_byte)
                .expect_err("decoding truncated 1 byte succeeded")
                .kind(),
            Truncated
        );
        assert_eq!(
            decode_varint_slow(&mut &*truncated_one_byte)
                .expect_err("slow decoding truncated 1 byte succeeded")
                .kind(),
            Truncated
        );

        let truncated_two_bytes: &[u8] = &[0x80, 0xFF];
        assert_eq!(
            decode_varint(&mut &*truncated_two_bytes)
                .expect_err("decoding truncated 6 bytes succeeded")
                .kind(),
            Truncated
        );
        assert_eq!(
            decode_varint_slow(&mut &*truncated_two_bytes)
                .expect_err("slow decoding truncated 6 bytes succeeded")
                .kind(),
            Truncated
        );

        let truncated_six_bytes: &[u8] = &[0x80, 0x81, 0x82, 0x8A, 0x8B, 0x8C];
        assert_eq!(
            decode_varint(&mut &*truncated_six_bytes)
                .expect_err("decoding truncated 6 bytes succeeded")
                .kind(),
            Truncated
        );
        assert_eq!(
            decode_varint_slow(&mut &*truncated_six_bytes)
                .expect_err("slow decoding truncated 6 bytes succeeded")
                .kind(),
            Truncated
        );

        let truncated_eight_bytes: &[u8] = &[0x80, 0x81, 0x82, 0x8A, 0x8B, 0x8C, 0xBE, 0xEF];
        assert_eq!(
            decode_varint(&mut &*truncated_eight_bytes)
                .expect_err("decoding truncated 8 bytes succeeded")
                .kind(),
            Truncated
        );
        assert_eq!(
            decode_varint_slow(&mut &*truncated_eight_bytes)
                .expect_err("slow decoding truncated 8 bytes succeeded")
                .kind(),
            Truncated
        );
    }

    fn check_rejects_wrong_wire_type<T: ForOverwrite + Decoder<E>, E>(wire_type: WireType) {
        let mut out = T::for_overwrite();
        assert_eq!(
            <T as Decoder<E>>::decode(
                wire_type,
                false,
                &mut out,
                Capped::new(&mut [0u8; 0].as_slice()),
                DecodeContext::default(),
            ),
            Err(DecodeError::new(WrongWireType))
        );
    }

    fn check_rejects_wrong_wire_type_distinguished<T, E>(wire_type: WireType)
    where
        T: ForOverwrite + Decoder<E> + DistinguishedDecoder<E>,
    {
        let mut out = T::for_overwrite();
        assert_eq!(
            <T as DistinguishedDecoder<E>>::decode_distinguished(
                wire_type,
                false,
                &mut out,
                Capped::new(&mut [0u8; 0].as_slice()),
                RestrictedDecodeContext::new(Canonicity::NotCanonical),
            ),
            Err(DecodeError::new(WrongWireType))
        );
        check_rejects_wrong_wire_type::<T, E>(wire_type);
    }

    #[test]
    fn varints_reject_wrong_wire_type() {
        for wire_type in [
            WireType::LengthDelimited,
            WireType::ThirtyTwoBit,
            WireType::SixtyFourBit,
        ] {
            check_rejects_wrong_wire_type_distinguished::<u32, General>(wire_type);
            check_rejects_wrong_wire_type_distinguished::<u64, General>(wire_type);
            check_rejects_wrong_wire_type_distinguished::<i32, General>(wire_type);
            check_rejects_wrong_wire_type_distinguished::<i64, General>(wire_type);
            check_rejects_wrong_wire_type_distinguished::<bool, General>(wire_type);
        }
    }

    #[test]
    fn floats_reject_wrong_wire_type() {
        for wire_type in [
            WireType::Varint,
            WireType::LengthDelimited,
            WireType::SixtyFourBit,
        ] {
            check_rejects_wrong_wire_type::<f32, General>(wire_type);
            check_rejects_wrong_wire_type::<f32, Fixed>(wire_type);
        }
        for wire_type in [
            WireType::Varint,
            WireType::LengthDelimited,
            WireType::ThirtyTwoBit,
        ] {
            check_rejects_wrong_wire_type::<f64, General>(wire_type);
            check_rejects_wrong_wire_type::<f64, Fixed>(wire_type);
        }
    }

    #[test]
    fn variable_length_values_reject_wrong_wire_type() {
        for wire_type in [
            WireType::Varint,
            WireType::ThirtyTwoBit,
            WireType::SixtyFourBit,
        ] {
            check_rejects_wrong_wire_type_distinguished::<String, General>(wire_type);
            check_rejects_wrong_wire_type_distinguished::<Blob, General>(wire_type);
            check_rejects_wrong_wire_type_distinguished::<Vec<u8>, PlainBytes>(wire_type);
        }
    }

    proptest! {
        #[test]
        fn u32_in_u64(value: u32) {
            let mut buf = Vec::<u8>::new();
            ValueEncoder::<General>::encode_value(&value, &mut buf);
            let mut out = 0u64;
            prop_assert!(ValueDecoder::<General>::decode_value(
                &mut out,
                Capped::new(&mut &*buf),
                DecodeContext::default(),
            ).is_ok());
            prop_assert_eq!(out, value as u64);
        }

        #[test]
        fn i32_in_i64(value: i32) {
            let mut buf = Vec::<u8>::new();
            ValueEncoder::<General>::encode_value(&value, &mut buf);
            let mut out = 0i64;
            prop_assert!(ValueDecoder::<General>::decode_value(
                &mut out,
                Capped::new(&mut &*buf),
                DecodeContext::default(),
            ).is_ok());
            prop_assert_eq!(out, value as i64);
        }

        #[test]
        fn u64_in_u32(value: u32) {
            let value = value as u64;
            let mut buf = Vec::<u8>::new();
            ValueEncoder::<General>::encode_value(&value, &mut buf);
            let mut out = 0u32;
            prop_assert!(ValueDecoder::<General>::decode_value(
                &mut out,
                Capped::new(&mut &*buf),
                DecodeContext::default(),
            ).is_ok());
            prop_assert_eq!(out as u64, value);
        }

        #[test]
        fn i64_in_i32(value: i32) {
            let value = value as i64;
            let mut buf = Vec::<u8>::new();
            ValueEncoder::<General>::encode_value(&value, &mut buf);
            let mut out = 0i32;
            prop_assert!(ValueDecoder::<General>::decode_value(
                &mut out,
                Capped::new(&mut &*buf),
                DecodeContext::default(),
            ).is_ok());
            prop_assert_eq!(out as i64, value);
        }

        #[test]
        fn u32_out_of_range(value in u32::MAX as u64 + 1..) {
            let mut buf = Vec::<u8>::new();
            ValueEncoder::<General>::encode_value(&value, &mut buf);
            let mut out = 0u32;
            prop_assert_eq!(
                ValueDecoder::<General>::decode_value(
                    &mut out,
                    Capped::new(&mut &*buf),
                    DecodeContext::default(),
                ),
                Err(DecodeError::new(OutOfDomainValue))
            );
        }

        #[test]
        fn i32_out_of_range(
            low_value in ..i32::MIN as i64 - 1,
            high_value in i32::MAX as i64 + 1..,
        ) {
            for value in [low_value, high_value] {
                let mut buf = Vec::<u8>::new();
                ValueEncoder::<General>::encode_value(&value, &mut buf);
                let mut out = 0i32;
                prop_assert_eq!(
                    ValueDecoder::<General>::decode_value(
                        &mut out,
                        Capped::new(&mut &*buf),
                        DecodeContext::default(),
                    ),
                    Err(DecodeError::new(OutOfDomainValue))
                );
            }
        }

        #[test]
        fn u16_out_of_range(value in u16::MAX as u64 + 1..) {
            let mut buf = Vec::<u8>::new();
            ValueEncoder::<General>::encode_value(&value, &mut buf);
            let mut out = 0u16;
            prop_assert_eq!(
                ValueDecoder::<General>::decode_value(
                    &mut out,
                    Capped::new(&mut &*buf),
                    DecodeContext::default(),
                ),
                Err(DecodeError::new(OutOfDomainValue))
            );
        }

        #[test]
        fn i16_out_of_range(
            low_value in ..i16::MIN as i64 - 1,
            high_value in i16::MAX as i64 + 1..,
        ) {
            for value in [low_value, high_value] {
                let mut buf = Vec::<u8>::new();
                ValueEncoder::<General>::encode_value(&value, &mut buf);
                let mut out = 0i16;
                prop_assert_eq!(
                    ValueDecoder::<General>::decode_value(
                        &mut out,
                        Capped::new(&mut &*buf),
                        DecodeContext::default(),
                    ),
                    Err(DecodeError::new(OutOfDomainValue))
                );
            }
        }

        #[test]
        fn u8_out_of_range(value in u8::MAX as u64 + 1..) {
            let mut buf = Vec::<u8>::new();
            ValueEncoder::<Varint>::encode_value(&value, &mut buf);
            let mut out = 0u8;
            prop_assert_eq!(
                ValueDecoder::<Varint>::decode_value(
                    &mut out,
                    Capped::new(&mut &*buf),
                    DecodeContext::default(),
                ),
                Err(DecodeError::new(OutOfDomainValue))
            );
        }

        #[test]
        fn i8_out_of_range(
            low_value in ..i8::MIN as i64 - 1,
            high_value in i8::MAX as i64 + 1..,
        ) {
            for value in [low_value, high_value] {
                let mut buf = Vec::<u8>::new();
                ValueEncoder::<Varint>::encode_value(&value, &mut buf);
                let mut out = 0i8;
                prop_assert_eq!(
                    ValueDecoder::<Varint>::decode_value(
                        &mut out,
                        Capped::new(&mut &*buf),
                        DecodeContext::default(),
                    ),
                    Err(DecodeError::new(OutOfDomainValue))
                );
            }
        }

        #[test]
        fn bool_out_of_range(varint in 2u64..) {
            let mut buf = Vec::<u8>::new();
            encode_varint(varint, &mut buf);
            let mut out = false;
            prop_assert_eq!(
                ValueDecoder::<General>::decode_value(
                    &mut out,
                    Capped::new(&mut &*buf),
                    DecodeContext::default(),
                ),
                Err(DecodeError::new(OutOfDomainValue))
            );
        }

        #[test]
        fn field_key_too_big(tag in u32::MAX as u64 + 1..) {
            let mut buf = Vec::<u8>::new();
            encode_varint(tag << 2, &mut buf);
            prop_assert_eq!(
                TagReader::new().decode_key(Capped::new(&mut buf.as_slice())),
                Err(DecodeError::new(TagOverflowed))
            );
        }
    }
}
