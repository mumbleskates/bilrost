//! Utility functions and types for encoding and decoding Protobuf types.
//!
//! Meant to be used only from `Message` implementations.

#![allow(clippy::implicit_hasher, clippy::ptr_arg)]

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::cmp::{
    min,
    Ordering::{Equal, Greater, Less},
};
use core::convert::TryFrom;
use core::iter::Extend;
use core::mem;
use core::ops::{Deref, DerefMut};
use core::str;

use ::bytes::buf::Take;
use ::bytes::{Buf, BufMut, Bytes};

use crate::Message;
use crate::{decode_length_delimiter, DecodeError};

/// Encodes an integer value into LEB128-bijective variable length format, and writes it to the
/// buffer. The buffer must have enough remaining space (maximum 9 bytes).
#[inline]
pub fn encode_varint<B: BufMut>(mut value: u64, buf: &mut B) {
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

/// Decodes a LEB128-bijective-encoded variable length integer from the buffer.
#[inline]
pub fn decode_varint<B: Buf>(buf: &mut B) -> Result<u64, DecodeError> {
    let bytes = buf.chunk();
    let len = bytes.len();
    if len == 0 {
        return Err(DecodeError::new("invalid varint"));
    }

    let byte = bytes[0];
    if byte < 0x80 {
        buf.advance(1);
        Ok(u64::from(byte))
    } else if len >= 9 || bytes[len - 1] < 0x80 {
        let (value, advance) = decode_varint_slice(bytes)?;
        buf.advance(advance);
        Ok(value)
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
#[inline]
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
        Err(DecodeError::new("overflowed varint"))
    } else {
        Ok((value + (u64::from(b) << 56), 9))
    }
}

/// Decodes a LEB128-encoded variable length integer from the buffer, advancing the buffer as
/// necessary.
#[inline(never)]
#[cold]
fn decode_varint_slow<B>(buf: &mut B) -> Result<u64, DecodeError>
where
    B: Buf,
{
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
        return Err(DecodeError::new("truncated varint"));
    }
    // The decoding process for bijective varints is largely the same as for non-bijective, except
    // we simply don't remove the MSB from each byte before adding it to the decoded value. Thus,
    // all 64 bits are already spoken for after the 9th byte (56 from the lower 7 of the first 8
    // bytes and 8 more from the 9th byte) and we can check for uint64 overflow after reading the
    // 9th byte; the 10th byte that would be obligated by the encoding if we cared about
    // generalizing the encoding to more than 64 bit numbers would always be zero, and if there is a
    // desire to encode varints greater than 64 bits in size it is more efficient to use a
    // length-prefixed encoding, which is just the blob wiretype.
    return u64::checked_add(value, u64::from(buf.get_u8()) << 56)
        .ok_or(DecodeError::new("overflowed varint"));
    // There is probably a reason why using u64::checked_add here seems to cause decoding even
    // smaller varints to bench faster, while using it in the fast-path in decode_varint_slice
    // causes a 5x pessimization. Probably best not to worry about it too much.
}

/// Additional information passed to every decode/merge function.
///
/// The context should be passed by value and can be freely cloned. When passing
/// to a function which is decoding a nested object, then use `enter_recursion`.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "no-recursion-limit", derive(Default))]
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

#[cfg(not(feature = "no-recursion-limit"))]
impl Default for DecodeContext {
    #[inline]
    fn default() -> DecodeContext {
        DecodeContext {
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
    #[cfg(not(feature = "no-recursion-limit"))]
    #[inline]
    pub(crate) fn enter_recursion(&self) -> DecodeContext {
        DecodeContext {
            recurse_count: self.recurse_count - 1,
        }
    }

    #[cfg(feature = "no-recursion-limit")]
    #[inline]
    pub(crate) fn enter_recursion(&self) -> DecodeContext {
        DecodeContext {}
    }

    /// Checks whether the recursion limit has been reached in the stack of
    /// decodes described by the `DecodeContext` at `self.ctx`.
    ///
    /// Returns `Ok<()>` if it is ok to continue recursing.
    /// Returns `Err<DecodeError>` if the recursion limit has been reached.
    #[cfg(not(feature = "no-recursion-limit"))]
    #[inline]
    pub(crate) fn limit_reached(&self) -> Result<(), DecodeError> {
        if self.recurse_count == 0 {
            Err(DecodeError::new("recursion limit reached"))
        } else {
            Ok(())
        }
    }

    #[cfg(feature = "no-recursion-limit")]
    #[inline]
    #[allow(clippy::unnecessary_wraps)] // needed in other features
    pub(crate) fn limit_reached(&self) -> Result<(), DecodeError> {
        Ok(())
    }
}

/// Returns the encoded length of the value in LEB128-bijective variable length format.
/// The returned value will be between 1 and 9, inclusive.
#[inline]
pub fn encoded_len_varint(value: u64) -> usize {
    const LIMIT: [u64; 9] = [
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
    if value < LIMIT[1] {
        1
    } else if value < LIMIT[5] {
        if value < LIMIT[3] {
            if value < LIMIT[2] {
                2
            } else {
                3
            }
        } else if value < LIMIT[4] {
            4
        } else {
            5
        }
    } else if value < LIMIT[7] {
        if value < LIMIT[6] {
            6
        } else {
            7
        }
    } else if value < LIMIT[8] {
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
    SixtyFourBit = 2,
    ThirtyTwoBit = 3,
}

impl TryFrom<u64> for WireType {
    type Error = DecodeError;

    #[inline]
    fn try_from(value: u64) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(WireType::Varint),
            1 => Ok(WireType::LengthDelimited),
            2 => Ok(WireType::SixtyFourBit),
            3 => Ok(WireType::ThirtyTwoBit),
            _ => Err(DecodeError::new(format!(
                "invalid wire type value: {}",
                value
            ))),
        }
    }
}

impl WireType {
    const fn encoded_size_alignment(self) -> usize {
        match self {
            WireType::Varint => 1,
            WireType::LengthDelimited => 1,
            WireType::SixtyFourBit => 8,
            WireType::ThirtyTwoBit => 4,
        }
    }
}

pub struct TagWriter {
    last_tag: u32,
}

impl TagWriter {
    pub fn new() -> Self {
        Self { last_tag: 0 }
    }

    /// Encode the key delta to the given key into the buffer.
    ///
    /// All fields must be encoded in order; this is enforced in the encoding by encoding each
    /// field's tag as a non-negative delta from the previously encoded field's tag. The tag delta
    /// is encoded in the bits above the lowest two bits in the key delta, which encode the wire
    /// type. When decoding, the wire type is taken as-is, and the tag delta added to the tag of the
    /// last field decoded.
    #[inline]
    pub fn encode_key<B: BufMut>(&mut self, tag: u32, wire_type: WireType, buf: &mut B) {
        match tag.cmp(&self.last_tag) {
            Greater => {
                let key_delta = (((tag - self.last_tag) as u64) << 2) | (wire_type as u64);
                encode_varint(key_delta, buf);
                self.last_tag = tag;
            }
            Equal => {
                // Write the wire type as a single-byte varint.
                buf.put_u8(wire_type as u8);
            }
            Less => panic!(
                "fields encoded out of order: last was {:?}, new is {:?}",
                self.last_tag, tag
            ),
        }
    }

    #[inline]
    pub fn measurer(&self) -> TagMeasurer {
        TagMeasurer {
            last_tag: self.last_tag,
        }
    }
}

/// Simulator for writing tags, capable of outputting their encoded length.
pub struct TagMeasurer {
    last_tag: u32,
}

impl TagMeasurer {
    pub fn new() -> Self {
        Self { last_tag: 0 }
    }

    /// Returns the number of bytes that would be written if the given tag was encoded next, and
    /// also advances the state of the encoder as if that tag was written.
    #[inline]
    pub fn key_len(&mut self, tag: u32) -> usize {
        let len = match tag.cmp(&self.last_tag) {
            Greater => encoded_len_varint(((tag - self.last_tag) as u64) << 2),
            Equal => 1,
            Less => panic!(
                "fields encoded out of order: last was {:?}, new is {:?}",
                self.last_tag, tag
            ),
        };
        self.last_tag = tag;
        len
    }
}

pub struct TagReader {
    last_tag: u32,
}

impl TagReader {
    pub fn new() -> Self {
        Self { last_tag: 0 }
    }

    #[inline(always)]
    pub fn decode_key<B: Buf>(&mut self, buf: &mut B) -> Result<(u32, WireType), DecodeError> {
        let key_delta = decode_varint(buf)?;
        let tag_delta = key_delta >> 2;
        let tag = (self.last_tag as u64) + tag_delta;
        if tag > u64::from(u32::MAX) {
            return Err(DecodeError::new("tag overflowed"));
        }
        let wire_type = WireType::try_from(key_delta & 0b11)?;
        self.last_tag = tag as u32;
        Ok((tag as u32, wire_type))
    }
}

/// Checks that the expected wire type matches the actual wire type,
/// or returns an error result.
#[inline]
pub fn check_wire_type(expected: WireType, actual: WireType) -> Result<(), DecodeError> {
    if expected != actual {
        return Err(DecodeError::new(format!(
            "invalid wire type: {:?} (expected {:?})",
            actual, expected
        )));
    }
    Ok(())
}

/// A soft-limited wrapper for `impl Buf` that doesn't invoke extra work whenever the buffer is read
/// from, only when the remaining bytes are checked. This means it can be nested arbitrarily without
/// adding extra work every time.
pub struct Capped<'a, B: Buf> {
    buf: &'a mut B,
    extra_bytes_remaining: usize,
}

impl<'a, B: Buf> Capped<'a, B> {
    /// Creates a Capped instance with a cap at the very end of the given buffer.
    pub fn new(buf: &'a mut B) -> Self {
        Self {
            buf,
            extra_bytes_remaining: 0,
        }
    }

    /// Reads a length delimiter from the beginning of the wrapped buffer, then returns a subsidiary
    /// Capped instance for the delineated bytes if it does not overrun the underlying buffer or
    /// this instance's cap.
    fn take_length_delimited(&mut self) -> Result<Capped<B>, DecodeError> {
        let len = decode_length_delimiter(&mut *self.buf)?;
        let remaining = self.buf.remaining();
        if len > remaining {
            return Err(DecodeError::new("field truncated"));
        }
        let extra_bytes_remaining = remaining - len;
        if extra_bytes_remaining < self.extra_bytes_remaining {
            return Err(DecodeError::new("field truncated"));
        }
        Ok(Capped {
            buf: self.buf,
            extra_bytes_remaining,
        })
    }

    /// Passes the wrapped buffer to the provided continuation until an error occurs, the buffer
    /// is consumed up to the cap, or the buffer is consumed *beyond* the cap (which is an error).
    #[inline]
    fn consume_to_cap<F>(&mut self, mut read_with: F) -> Result<(), DecodeError>
    where
        F: FnMut(&mut Self) -> Result<(), DecodeError>,
    {
        while self.buf.remaining() > self.extra_bytes_remaining {
            read_with(self)?;
        }
        if self.buf.remaining() < self.extra_bytes_remaining {
            return Err(DecodeError::new("delimited length exceeded"));
        }
        Ok(())
    }

    #[inline]
    pub fn buf(&mut self) -> &mut B {
        self.buf
    }

    #[inline]
    fn take_all(self) -> Take<&'a mut B> {
        let len = self.remaining_before_cap();
        self.buf.take(len)
    }

    #[inline]
    fn decode_varint(&mut self) -> Result<u64, DecodeError> {
        decode_varint(self.buf)
    }

    /// Returns the number of bytes left before the cap.
    #[inline]
    pub fn remaining_before_cap(&self) -> usize {
        self.buf
            .remaining()
            .saturating_sub(self.extra_bytes_remaining)
    }
}

impl<'a, B: Buf> Deref for Capped<'a, B> {
    type Target = B;

    fn deref(&self) -> &B {
        self.buf
    }
}

impl<'a, B: Buf> DerefMut for Capped<'a, B> {
    fn deref_mut(&mut self) -> &mut B {
        self.buf
    }
}

pub fn skip_field<B: Buf>(wire_type: WireType, buf: &mut Capped<B>) -> Result<(), DecodeError> {
    let len = match wire_type {
        WireType::Varint => buf.decode_varint().map(|_| 0)?,
        WireType::ThirtyTwoBit => 4,
        WireType::SixtyFourBit => 8,
        WireType::LengthDelimited => buf.decode_varint()?,
    };

    if len > buf.remaining() as u64 {
        return Err(DecodeError::new("field truncated"));
    }

    buf.advance(len as usize);
    Ok(())
}

pub trait Encoder<T> {
    fn encode<B: BufMut>(tag: u32, value: &T, buf: &mut B, tw: &mut TagWriter);
    // TODO(widders): change to (or augment with) build-in-reverse-then-emit-forward and
    //  emit-reversed
    fn encoded_len(tag: u32, value: &T, tm: &mut TagMeasurer) -> usize;
    fn decode<B: Buf>(
        wire_type: WireType,
        value: &mut T,
        buf: &mut Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError>;
}

pub trait DistinguishedEncoder<T>: Encoder<T> {
    fn decode_distinguished<B: Buf>(
        wire_type: WireType,
        value: &mut T,
        buf: &mut Capped<B>,
        ctx: DecodeContext,
    )-> Result<(), DecodeError>;
}

/// Helper macro which emits an `encode_repeated` function for the type.
macro_rules! encode_repeated {
    ($ty:ty) => {
        pub fn encode_repeated<B: BufMut>(
            tag: u32,
            values: &[$ty],
            buf: &mut B,
            tw: &mut TagWriter,
        ) {
            for value in values {
                encode(tag, value, buf, tw);
            }
        }
    };
}

/// Helper macro which emits a `merge_repeated` function for the numeric type.
macro_rules! merge_repeated_numeric {
    ($ty:ty,
     $wire_type:expr,
     $merge:ident,
     $merge_repeated:ident) => {
        pub fn $merge_repeated<B: Buf>(
            wire_type: WireType,
            values: &mut Vec<$ty>,
            buf: &mut Capped<B>,
            ctx: DecodeContext,
        ) -> Result<(), DecodeError> {
            if wire_type == WireType::LengthDelimited {
                // Packed.
                let mut capped = buf.take_length_delimited()?;
                if capped.remaining_before_cap() % $wire_type.encoded_size_alignment() != 0 {
                    return Err(DecodeError::new("packed field is not a valid length"));
                }
                capped.consume_to_cap(|buf| {
                    let mut value = Default::default();
                    $merge($wire_type, &mut value, buf, ctx.clone())?;
                    values.push(value);
                    Ok(())
                })
            } else {
                // Unpacked.
                check_wire_type($wire_type, wire_type)?;
                let mut value = Default::default();
                $merge(wire_type, &mut value, buf, ctx)?;
                values.push(value);
                Ok(())
            }
        }
    };
}

/// Macro which emits a module containing a set of encoding functions for a
/// variable width numeric type.
macro_rules! varint {
    ($ty:ty,
     $proto_ty:ident) => (
        varint!($ty,
                $proto_ty,
                to_uint64(value) { *value as u64 },
                from_uint64(value) { value as $ty });
    );

    ($ty:ty,
     $proto_ty:ident,
     to_uint64($to_uint64_value:ident) $to_uint64:expr,
     from_uint64($from_uint64_value:ident) $from_uint64:expr) => (

         pub mod $proto_ty {
            use crate::encoding::*;

            pub fn encode<B: BufMut>(tag: u32, $to_uint64_value: &$ty, buf: &mut B, tw: &mut TagWriter) {
                tw.encode_key(tag, WireType::Varint, buf);
                encode_varint($to_uint64, buf);
            }

            pub fn merge<B: Buf>(
                wire_type: WireType,
                value: &mut $ty,
                buf: &mut Capped<B>,
                _ctx: DecodeContext,
            ) -> Result<(), DecodeError> {
                check_wire_type(WireType::Varint, wire_type)?;
                let $from_uint64_value = buf.decode_varint()?;
                *value = $from_uint64;
                Ok(())
            }

            encode_repeated!($ty);

            pub fn encode_packed<B: BufMut>(tag: u32, values: &[$ty], buf: &mut B, tw: &mut TagWriter) {
                if values.is_empty() { return; }

                tw.encode_key(tag, WireType::LengthDelimited, buf);
                let len: usize = values.iter().map(|$to_uint64_value| {
                    encoded_len_varint($to_uint64)
                }).sum();
                encode_varint(len as u64, buf);

                for $to_uint64_value in values {
                    encode_varint($to_uint64, buf);
                }
            }

            merge_repeated_numeric!($ty, WireType::Varint, merge, merge_repeated);

            #[inline]
            pub fn encoded_len(tag: u32, $to_uint64_value: &$ty, tm: &mut TagMeasurer) -> usize {
                tm.key_len(tag) + encoded_len_varint($to_uint64)
            }

            #[inline]
            pub fn encoded_len_repeated(tag: u32, values: &[$ty], tm: &mut TagMeasurer) -> usize {
                if values.is_empty() {
                    0
                } else {
                    // successive repeated keys always take up 1 byte
                    tm.key_len(tag) + values.len() - 1
                        + values
                            .iter()
                            .map(|$to_uint64_value| {
                                encoded_len_varint($to_uint64)
                            })
                            .sum::<usize>()
                }
            }

            #[inline]
            pub fn encoded_len_packed(tag: u32, values: &[$ty], tm: &mut TagMeasurer) -> usize {
                if values.is_empty() {
                    0
                } else {
                    let len = values.iter()
                                    .map(|$to_uint64_value| encoded_len_varint($to_uint64))
                                    .sum::<usize>();
                    tm.key_len(tag) + encoded_len_varint(len as u64) + len
                }
            }

            #[cfg(test)]
            mod test {
                use proptest::prelude::*;

                use crate::encoding::$proto_ty::*;
                use crate::encoding::test::{
                    check_collection_type,
                    check_type,
                };

                proptest! {
                    #[test]
                    fn check(value: $ty, tag: u32) {
                        check_type(value, tag, WireType::Varint,
                                   encode, merge, encoded_len)?;
                    }
                    #[test]
                    fn check_repeated(value: Vec<$ty>, tag: u32) {
                        check_collection_type(value, tag, WireType::Varint,
                                              encode_repeated, merge_repeated,
                                              encoded_len_repeated)?;
                    }
                    #[test]
                    fn check_packed(value: Vec<$ty>, tag: u32) {
                        check_type(value, tag, WireType::LengthDelimited,
                                   encode_packed, merge_repeated,
                                   encoded_len_packed)?;
                    }
                }
            }
         }

    );
}
varint!(bool, bool,
to_uint64(value) {
    u64::from(*value)
},
from_uint64(value) {
    match value {
        0 => false,
        1 => true,
        _ => return Err(DecodeError::new("invalid varint value for bool"))
    }
});
varint!(u32, uint32,
to_uint64(value) {
    *value as u64
},
from_uint64(value) {
    u32::try_from(value).map_err(|_| DecodeError::new("varint overflows range of uint32"))?
});
varint!(u64, uint64);
varint!(i32, sint32,
to_uint64(value) {
    ((value << 1) ^ (value >> 31)) as u32 as u64
},
from_uint64(value) {
    let value = u32::try_from(value)
        .map_err(|_| DecodeError::new("varint overflows range of sint32"))?;
    ((value >> 1) as i32) ^ (-((value & 1) as i32))
});
varint!(i64, sint64,
to_uint64(value) {
    ((value << 1) ^ (value >> 63)) as u64
},
from_uint64(value) {
    ((value >> 1) as i64) ^ (-((value & 1) as i64))
});

/// Macro which emits a module containing a set of encoding functions for a
/// fixed width numeric type.
macro_rules! fixed_width {
    ($ty:ty,
     $width:expr,
     $wire_type:expr,
     $proto_ty:ident,
     $put:ident,
     $get:ident) => {
        pub mod $proto_ty {
            use crate::encoding::*;

            pub fn encode<B: BufMut>(tag: u32, value: &$ty, buf: &mut B, tw: &mut TagWriter) {
                tw.encode_key(tag, $wire_type, buf);
                buf.$put(*value);
            }

            pub fn merge<B: Buf>(
                wire_type: WireType,
                value: &mut $ty,
                buf: &mut Capped<B>,
                _ctx: DecodeContext,
            ) -> Result<(), DecodeError> {
                check_wire_type($wire_type, wire_type)?;
                if buf.remaining() < $width {
                    return Err(DecodeError::new("field truncated"));
                }
                *value = buf.$get();
                Ok(())
            }

            encode_repeated!($ty);

            pub fn encode_packed<B: BufMut>(
                tag: u32,
                values: &[$ty],
                buf: &mut B,
                tw: &mut TagWriter,
            ) {
                if values.is_empty() {
                    return;
                }

                tw.encode_key(tag, WireType::LengthDelimited, buf);
                let len = values.len() as u64 * $width;
                encode_varint(len as u64, buf);

                for value in values {
                    buf.$put(*value);
                }
            }

            merge_repeated_numeric!($ty, $wire_type, merge, merge_repeated);

            #[inline]
            pub fn encoded_len(tag: u32, _: &$ty, tm: &mut TagMeasurer) -> usize {
                tm.key_len(tag) + $width
            }

            #[inline]
            pub fn encoded_len_repeated(tag: u32, values: &[$ty], tm: &mut TagMeasurer) -> usize {
                if values.is_empty() {
                    0
                } else {
                    // successive repeated keys always take up 1 byte
                    tm.key_len(tag) - 1 + (1 + $width) * values.len()
                }
            }

            #[inline]
            pub fn encoded_len_packed(tag: u32, values: &[$ty], tm: &mut TagMeasurer) -> usize {
                if values.is_empty() {
                    0
                } else {
                    let len = $width * values.len();
                    tm.key_len(tag) + encoded_len_varint(len as u64) + len
                }
            }

            #[cfg(test)]
            mod test {
                use proptest::prelude::*;

                use super::super::test::{check_collection_type, check_type};
                use super::*;

                proptest! {
                    #[test]
                    fn check(value: $ty, tag: u32) {
                        check_type(value, tag, $wire_type,
                                   encode, merge, encoded_len)?;
                    }
                    #[test]
                    fn check_repeated(value: Vec<$ty>, tag: u32) {
                        check_collection_type(value, tag, $wire_type,
                                              encode_repeated, merge_repeated,
                                              encoded_len_repeated)?;
                    }
                    #[test]
                    fn check_packed(value: Vec<$ty>, tag: u32) {
                        check_type(value, tag, WireType::LengthDelimited,
                                   encode_packed, merge_repeated,
                                   encoded_len_packed)?;
                    }
                }
            }
        }
    };
}
fixed_width!(
    f32,
    4,
    WireType::ThirtyTwoBit,
    float32,
    put_f32_le,
    get_f32_le
);
fixed_width!(
    f64,
    8,
    WireType::SixtyFourBit,
    float64,
    put_f64_le,
    get_f64_le
);
fixed_width!(
    u32,
    4,
    WireType::ThirtyTwoBit,
    ufixed32,
    put_u32_le,
    get_u32_le
);
fixed_width!(
    u64,
    8,
    WireType::SixtyFourBit,
    ufixed64,
    put_u64_le,
    get_u64_le
);
fixed_width!(
    i32,
    4,
    WireType::ThirtyTwoBit,
    sfixed32,
    put_i32_le,
    get_i32_le
);
fixed_width!(
    i64,
    8,
    WireType::SixtyFourBit,
    sfixed64,
    put_i64_le,
    get_i64_le
);

/// Macro which emits encoding functions for a length-delimited type.
macro_rules! length_delimited {
    ($ty:ty) => {
        encode_repeated!($ty);

        pub fn merge_repeated<B: Buf>(
            wire_type: WireType,
            values: &mut Vec<$ty>,
            buf: &mut Capped<B>,
            ctx: DecodeContext,
        ) -> Result<(), DecodeError> {
            check_wire_type(WireType::LengthDelimited, wire_type)?;
            let mut value = Default::default();
            merge(wire_type, &mut value, buf, ctx)?;
            values.push(value);
            Ok(())
        }

        #[inline]
        pub fn encoded_len(tag: u32, value: &$ty, tm: &mut TagMeasurer) -> usize {
            tm.key_len(tag) + encoded_len_varint(value.len() as u64) + value.len()
        }

        #[inline]
        pub fn encoded_len_repeated(tag: u32, values: &[$ty], tm: &mut TagMeasurer) -> usize {
            if values.is_empty() {
                0
            } else {
                // successive repeated keys always take up 1 byte
                tm.key_len(tag) + values.len() - 1
                    + values
                        .iter()
                        .map(|value| encoded_len_varint(value.len() as u64) + value.len())
                        .sum::<usize>()
            }
        }
    };
}

pub mod string {
    use super::*;

    pub fn encode<B>(tag: u32, value: &String, buf: &mut B, tw: &mut TagWriter)
    where
        B: BufMut,
    {
        tw.encode_key(tag, WireType::LengthDelimited, buf);
        encode_varint(value.len() as u64, buf);
        buf.put_slice(value.as_bytes());
    }

    pub fn merge<B: Buf>(
        wire_type: WireType,
        value: &mut String,
        buf: &mut Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        // ## Unsafety
        //
        // `string::merge` reuses `bytes::merge`, with an additional check of utf-8
        // well-formedness. If the utf-8 is not well-formed, or if any other error occurs, then the
        // string is cleared, so as to avoid leaking a string field with invalid data.
        //
        // This implementation uses the unsafe `String::as_mut_vec` method instead of the safe
        // alternative of temporarily swapping an empty `String` into the field, because it results
        // in up to 10% better performance on the protobuf message decoding benchmarks.
        //
        // It's required when using `String::as_mut_vec` that invalid utf-8 data not be leaked into
        // the backing `String`. To enforce this, even in the event of a panic in `bytes::merge` or
        // in the buf implementation, a drop guard is used.
        unsafe {
            struct DropGuard<'a>(&'a mut Vec<u8>);
            impl<'a> Drop for DropGuard<'a> {
                #[inline]
                fn drop(&mut self) {
                    self.0.clear();
                }
            }

            let drop_guard = DropGuard(value.as_mut_vec());
            bytes::merge_one_copy(wire_type, drop_guard.0, buf, ctx)?;
            match str::from_utf8(drop_guard.0) {
                Ok(_) => {
                    // Success; do not clear the bytes.
                    mem::forget(drop_guard);
                    Ok(())
                }
                Err(_) => Err(DecodeError::new(
                    "invalid string value: data is not UTF-8 encoded",
                )),
            }
        }
    }

    length_delimited!(String);

    #[cfg(test)]
    mod test {
        use proptest::prelude::*;

        use super::super::test::{check_collection_type, check_type};
        use super::*;

        proptest! {
            #[test]
            fn check(value: String, tag: u32) {
                super::test::check_type(value, tag, WireType::LengthDelimited,
                                        encode, merge, encoded_len)?;
            }
            #[test]
            fn check_repeated(value: Vec<String>, tag: u32) {
                super::test::check_collection_type(value, tag, WireType::LengthDelimited,
                                                   encode_repeated, merge_repeated,
                                                   encoded_len_repeated)?;
            }
        }
    }
}

pub trait BytesAdapter: sealed::BytesAdapter {}

mod sealed {
    use super::{Buf, BufMut};

    pub trait BytesAdapter: Default + Sized + 'static {
        fn len(&self) -> usize;

        /// Replace contents of this buffer with the contents of another buffer.
        fn replace_with<B>(&mut self, buf: B)
        where
            B: Buf;

        /// Appends this buffer to the (contents of) other buffer.
        fn append_to<B>(&self, buf: &mut B)
        where
            B: BufMut;

        fn is_empty(&self) -> bool {
            self.len() == 0
        }
    }
}

impl BytesAdapter for Bytes {}

impl sealed::BytesAdapter for Bytes {
    fn len(&self) -> usize {
        Buf::remaining(self)
    }

    fn replace_with<B>(&mut self, mut buf: B)
    where
        B: Buf,
    {
        *self = buf.copy_to_bytes(buf.remaining());
    }

    fn append_to<B>(&self, buf: &mut B)
    where
        B: BufMut,
    {
        buf.put(self.clone())
    }
}

impl BytesAdapter for Vec<u8> {}

impl sealed::BytesAdapter for Vec<u8> {
    fn len(&self) -> usize {
        Vec::len(self)
    }

    fn replace_with<B>(&mut self, buf: B)
    where
        B: Buf,
    {
        self.clear();
        self.reserve(buf.remaining());
        self.put(buf);
    }

    fn append_to<B>(&self, buf: &mut B)
    where
        B: BufMut,
    {
        buf.put(self.as_slice())
    }
}

pub mod bytes {
    use super::*;

    pub fn encode<A, B>(tag: u32, value: &A, buf: &mut B, tw: &mut TagWriter)
    where
        A: BytesAdapter,
        B: BufMut,
    {
        tw.encode_key(tag, WireType::LengthDelimited, buf);
        encode_varint(value.len() as u64, buf);
        value.append_to(buf);
    }

    pub fn merge<A, B>(
        wire_type: WireType,
        value: &mut A,
        buf: &mut Capped<B>,
        _ctx: DecodeContext,
    ) -> Result<(), DecodeError>
    where
        A: BytesAdapter,
        B: Buf,
    {
        check_wire_type(WireType::LengthDelimited, wire_type)?;
        let mut buf = buf.take_length_delimited()?;

        // Clear the existing value. This follows from the following rule in the encoding guide[1]:
        //
        // > Normally, an encoded message would never have more than one instance of a non-repeated
        // > field. However, parsers are expected to handle the case in which they do. For numeric
        // > types and strings, if the same field appears multiple times, the parser accepts the
        // > last value it sees.
        //
        // [1]: https://developers.google.com/protocol-buffers/docs/encoding#optional
        //
        // This is intended for A and B both being Bytes so it is zero-copy.
        // Some combinations of A and B types may cause a double-copy,
        // in which case merge_one_copy() should be used instead.
        // TODO(widders): never do this. we are blowing this away entirely. this will not happen in
        //  bilrost at all, because we can trivially detect incorrectly duplicated fields.
        let len = buf.remaining_before_cap();
        value.replace_with(buf.copy_to_bytes(len));
        Ok(())
    }

    pub(super) fn merge_one_copy<A, B>(
        wire_type: WireType,
        value: &mut A,
        buf: &mut Capped<B>,
        _ctx: DecodeContext,
    ) -> Result<(), DecodeError>
    where
        A: BytesAdapter,
        B: Buf,
    {
        check_wire_type(WireType::LengthDelimited, wire_type)?;
        let buf = buf.take_length_delimited()?;
        // If we must copy, make sure to copy only once.
        value.replace_with(buf.take_all());
        Ok(())
    }

    length_delimited!(impl BytesAdapter);

    #[cfg(test)]
    mod test {
        use proptest::prelude::*;

        use super::super::test::{check_collection_type, check_type};
        use super::*;

        proptest! {
            #[test]
            fn check_vec(value: Vec<u8>, tag: u32) {
                super::test::check_type::<Vec<u8>, Vec<u8>>(value, tag, WireType::LengthDelimited,
                                                            encode, merge, encoded_len)?;
            }

            #[test]
            fn check_bytes(value: Vec<u8>, tag: u32) {
                let value = Bytes::from(value);
                super::test::check_type::<Bytes, Bytes>(value, tag, WireType::LengthDelimited,
                                                        encode, merge, encoded_len)?;
            }

            #[test]
            fn check_repeated_vec(value: Vec<Vec<u8>>, tag: u32) {
                super::test::check_collection_type(value, tag, WireType::LengthDelimited,
                                                   encode_repeated, merge_repeated,
                                                   encoded_len_repeated)?;
            }

            #[test]
            fn check_repeated_bytes(value: Vec<Vec<u8>>, tag: u32) {
                let value = value.into_iter().map(Bytes::from).collect();
                super::test::check_collection_type(value, tag, WireType::LengthDelimited,
                                                   encode_repeated, merge_repeated,
                                                   encoded_len_repeated)?;
            }
        }
    }
}

pub mod message {
    use super::*;

    pub fn encode<M, B>(tag: u32, msg: &M, buf: &mut B, tw: &mut TagWriter)
    where
        M: Message,
        B: BufMut,
    {
        tw.encode_key(tag, WireType::LengthDelimited, buf);
        encode_varint(msg.encoded_len() as u64, buf);
        msg.encode_raw(buf);
    }

    pub fn merge<M, B>(
        wire_type: WireType,
        msg: &mut M,
        buf: &mut Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError>
    where
        M: Message,
        B: Buf,
    {
        check_wire_type(WireType::LengthDelimited, wire_type)?;
        ctx.limit_reached()?;
        let mut tr = TagReader::new();
        let inner_ctx = ctx.enter_recursion();
        let mut last_tag = None::<u32>;
        buf.take_length_delimited()?.consume_to_cap(|buf| {
            let (tag, wire_type) = tr.decode_key(buf.buf())?;
            let duplicated = last_tag == Some(tag);
            last_tag = Some(tag);
            msg.merge_field(tag, wire_type, duplicated, buf, inner_ctx.clone())
        })
    }

    pub fn encode_repeated<M, B>(tag: u32, messages: &[M], buf: &mut B, tw: &mut TagWriter)
    where
        M: Message,
        B: BufMut,
    {
        for msg in messages {
            encode(tag, msg, buf, tw);
        }
    }

    pub fn merge_repeated<M, B>(
        wire_type: WireType,
        messages: &mut Vec<M>,
        buf: &mut Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError>
    where
        M: Message + Default,
        B: Buf,
    {
        check_wire_type(WireType::LengthDelimited, wire_type)?;
        let mut msg = M::default();
        merge(WireType::LengthDelimited, &mut msg, buf, ctx)?;
        messages.push(msg);
        Ok(())
    }

    #[inline]
    pub fn encoded_len<M: Message>(tag: u32, msg: &M, tm: &mut TagMeasurer) -> usize {
        let len = msg.encoded_len();
        tm.key_len(tag) + encoded_len_varint(len as u64) + len
    }

    #[inline]
    pub fn encoded_len_repeated<M: Message>(
        tag: u32,
        messages: &[M],
        tm: &mut TagMeasurer,
    ) -> usize {
        if messages.is_empty() {
            0
        } else {
            // successive repeated keys always take up 1 byte
            tm.key_len(tag) + messages.len() - 1
                + messages
                    .iter()
                    .map(Message::encoded_len)
                    .map(|len| len + encoded_len_varint(len as u64))
                    .sum::<usize>()
        }
    }
}

/// Rust doesn't have a `Map` trait, so macros are currently the best way to be
/// generic over `HashMap` and `BTreeMap`.
macro_rules! map {
    // TODO(widders): change map configurations
    //  * map keys must not recur
    //  * maps should be packed! keys and values should directly alternate within a length-
    //    delineated field

    ($map_ty:ident) => {
        use crate::encoding::*;
        use core::hash::Hash;

        /// Generic protobuf map encode function.
        pub fn encode<K, V, B, KE, KL, VE, VL>(
            key_encode: KE,
            key_encoded_len: KL,
            val_encode: VE,
            val_encoded_len: VL,
            tag: u32,
            values: &$map_ty<K, V>,
            buf: &mut B,
            tw: &mut TagWriter,
        ) where
            K: Default + Eq + Hash + Ord,
            V: Default + PartialEq,
            B: BufMut,
            KE: Fn(u32, &K, &mut B, &mut TagWriter),
            KL: Fn(u32, &K, &mut TagMeasurer) -> usize,
            VE: Fn(u32, &V, &mut B, &mut TagWriter),
            VL: Fn(u32, &V, &mut TagMeasurer) -> usize,
        {
            encode_with_default(
                key_encode,
                key_encoded_len,
                val_encode,
                val_encoded_len,
                &V::default(),
                tag,
                values,
                buf,
                tw,
            )
        }

        /// Generic protobuf map merge function.
        pub fn merge<K, V, B, KM, VM>(
            key_merge: KM,
            val_merge: VM,
            values: &mut $map_ty<K, V>,
            buf: &mut Capped<B>,
            ctx: DecodeContext,
        ) -> Result<(), DecodeError>
        where
            K: Default + Eq + Hash + Ord,
            V: Default,
            B: Buf,
            KM: Fn(WireType, &mut K, &mut Capped<B>, DecodeContext) -> Result<(), DecodeError>,
            VM: Fn(WireType, &mut V, &mut Capped<B>, DecodeContext) -> Result<(), DecodeError>,
        {
            merge_with_default(key_merge, val_merge, V::default(), values, buf, ctx)
        }

        /// Generic protobuf map encode function.
        pub fn encoded_len<K, V, KL, VL>(
            key_encoded_len: KL,
            val_encoded_len: VL,
            tag: u32,
            values: &$map_ty<K, V>,
            tm: &mut TagMeasurer,
        ) -> usize
        where
            K: Default + Eq + Hash + Ord,
            V: Default + PartialEq,
            KL: Fn(u32, &K, &mut TagMeasurer) -> usize,
            VL: Fn(u32, &V, &mut TagMeasurer) -> usize,
        {
            encoded_len_with_default(
                key_encoded_len,
                val_encoded_len,
                &V::default(),
                tag,
                values,
                tm,
            )
        }

        /// Generic protobuf map encode function with an overridden value default.
        ///
        /// This is necessary because enumeration values can have a default value other
        /// than 0 in proto2.
        // TODO(widders): this probably isn't needed actually, due to the above. should enums all
        //  be optional-only?
        pub fn encode_with_default<K, V, B, KE, KL, VE, VL>(
            key_encode: KE,
            key_encoded_len: KL,
            val_encode: VE,
            val_encoded_len: VL,
            val_default: &V,
            tag: u32,
            values: &$map_ty<K, V>,
            buf: &mut B,
            tw: &mut TagWriter,
        ) where
            K: Default + Eq + Hash + Ord,
            V: PartialEq,
            B: BufMut,
            KE: Fn(u32, &K, &mut B, &mut TagWriter),
            KL: Fn(u32, &K, &mut TagMeasurer) -> usize,
            VE: Fn(u32, &V, &mut B, &mut TagWriter),
            VL: Fn(u32, &V, &mut TagMeasurer) -> usize,
        {
            for (key, val) in values.iter() {
                let skip_key = key == &K::default();
                let skip_val = val == val_default;
                let inner_tw = &mut TagWriter::new();
                let inner_tm = &mut inner_tw.measurer();

                let len = (if skip_key {
                    0
                } else {
                    key_encoded_len(1, key, inner_tm)
                }) + (if skip_val {
                    0
                } else {
                    val_encoded_len(2, val, inner_tm)
                });

                tw.encode_key(tag, WireType::LengthDelimited, buf);
                encode_varint(len as u64, buf);
                if !skip_key {
                    key_encode(1, key, buf, inner_tw);
                }
                if !skip_val {
                    val_encode(2, val, buf, inner_tw);
                }
            }
        }

        /// Generic protobuf map merge function with an overridden value default.
        ///
        /// This is necessary because enumeration values can have a default value other
        /// than 0 in proto2.
        // TODO(widders): this probably isn't needed actually, due to the above. should enums all
        //  be optional-only?
        pub fn merge_with_default<K, V, B, KM, VM>(
            key_merge: KM,
            val_merge: VM,
            val_default: V,
            values: &mut $map_ty<K, V>,
            buf: &mut Capped<B>,
            ctx: DecodeContext,
        ) -> Result<(), DecodeError>
        where
            K: Default + Eq + Hash + Ord,
            B: Buf,
            KM: Fn(WireType, &mut K, &mut Capped<B>, DecodeContext) -> Result<(), DecodeError>,
            VM: Fn(WireType, &mut V, &mut Capped<B>, DecodeContext) -> Result<(), DecodeError>,
        {
            let mut key: K = Default::default();
            let mut val = val_default;
            ctx.limit_reached()?;
            let mut tr = TagReader::new();
            buf.take_length_delimited()?.consume_to_cap(|buf| {
                let (tag, wire_type) = tr.decode_key(buf.buf())?;
                // TODO(widders): does this have correct behavior if k or v are incorrectly
                //  repeated?
                match tag {
                    1 => key_merge(wire_type, &mut key, buf, ctx.clone()),
                    2 => val_merge(wire_type, &mut val, buf, ctx.clone()),
                    _ => skip_field(wire_type, buf),
                }
            })?;
            values.insert(key, val);

            Ok(())
        }

        /// Generic protobuf map encode function with an overridden value default.
        ///
        /// This is necessary because enumeration values can have a default value other
        /// than 0 in proto2.
        pub fn encoded_len_with_default<K, V, KL, VL>(
            key_encoded_len: KL,
            val_encoded_len: VL,
            val_default: &V,
            tag: u32,
            values: &$map_ty<K, V>,
            tm: &mut TagMeasurer,
        ) -> usize
        where
            K: Default + Eq + Hash + Ord,
            V: PartialEq,
            KL: Fn(u32, &K, &mut TagMeasurer) -> usize,
            VL: Fn(u32, &V, &mut TagMeasurer) -> usize,
        {
            if values.is_empty() {
                0
            } else {
                // successive repeated keys always take up 1 byte
                tm.key_len(tag) + values.len() - 1
                    + values
                        .iter()
                        .map(|(key, val)| {
                            let inner_tm = &mut TagMeasurer::new();
                            let len = (if key == &K::default() {
                                0
                            } else {
                                key_encoded_len(1, key, inner_tm)
                            }) + (if val == val_default {
                                0
                            } else {
                                val_encoded_len(2, val, inner_tm)
                            });
                            encoded_len_varint(len as u64) + len
                        })
                        .sum::<usize>()
            }
        }
    };
}

#[cfg(feature = "std")]
pub mod hash_map {
    use std::collections::HashMap;
    map!(HashMap);
}

pub mod btree_map {
    map!(BTreeMap);
}

#[cfg(test)]
mod test {
    use alloc::string::ToString;
    use core::borrow::Borrow;
    use core::fmt::Debug;

    use ::bytes::{Bytes, BytesMut};
    use proptest::{prelude::*, test_runner::TestCaseResult};

    use crate::encoding::*;

    pub fn check_type<T, B>(
        value: T,
        tag: u32,
        wire_type: WireType,
        encode: fn(u32, &B, &mut BytesMut, &mut TagWriter),
        merge: fn(WireType, &mut T, &mut Capped<Bytes>, DecodeContext) -> Result<(), DecodeError>,
        encoded_len: fn(u32, &B, &mut TagMeasurer) -> usize,
    ) -> TestCaseResult
    where
        T: Debug + Default + PartialEq + Borrow<B>,
        B: ?Sized,
    {
        let mut tw = TagWriter::new();
        let mut tm = tw.measurer();
        let expected_len = encoded_len(tag, value.borrow(), &mut tm);

        let mut buf = BytesMut::with_capacity(expected_len);
        encode(tag, value.borrow(), &mut buf, &mut tw);

        let buf = &mut buf.freeze();
        let mut buf = Capped::new(buf);
        let mut tr = TagReader::new();

        prop_assert_eq!(
            buf.remaining(),
            expected_len,
            "encoded_len wrong; expected: {}, actual: {}",
            expected_len,
            buf.remaining()
        );

        if !buf.has_remaining() {
            // Short circuit for empty packed values.
            return Ok(());
        }

        let (decoded_tag, decoded_wire_type) = tr
            .decode_key(buf.buf())
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

        match wire_type {
            WireType::SixtyFourBit if buf.remaining() != 8 => Err(TestCaseError::fail(format!(
                "64bit wire type illegal remaining: {}, tag: {}",
                buf.remaining(),
                tag
            ))),
            WireType::ThirtyTwoBit if buf.remaining() != 4 => Err(TestCaseError::fail(format!(
                "32bit wire type illegal remaining: {}, tag: {}",
                buf.remaining(),
                tag
            ))),
            _ => Ok(()),
        }?;

        let mut roundtrip_value = T::default();
        merge(
            wire_type,
            &mut roundtrip_value,
            &mut buf,
            DecodeContext::default(),
        )
        .map_err(|error| TestCaseError::fail(error.to_string()))?;

        prop_assert!(
            !buf.has_remaining(),
            "expected buffer to be empty, remaining: {}",
            buf.remaining()
        );

        prop_assert_eq!(value, roundtrip_value);

        Ok(())
    }

    pub fn check_collection_type<T, B, E, M, L>(
        value: T,
        tag: u32,
        wire_type: WireType,
        encode: E,
        mut merge: M,
        encoded_len: L,
    ) -> TestCaseResult
    where
        T: Debug + Default + PartialEq + Borrow<B>,
        B: ?Sized,
        E: FnOnce(u32, &B, &mut BytesMut, &mut TagWriter),
        M: FnMut(WireType, &mut T, &mut Capped<Bytes>, DecodeContext) -> Result<(), DecodeError>,
        L: FnOnce(u32, &B, &mut TagMeasurer) -> usize,
    {
        let mut tw = TagWriter::new();
        let mut tm = tw.measurer();
        let expected_len = encoded_len(tag, value.borrow(), &mut tm);

        let mut buf = BytesMut::with_capacity(expected_len);
        encode(tag, value.borrow(), &mut buf, &mut tw);

        let mut tr = TagReader::new();
        let buf = &mut buf.freeze();
        let mut buf = Capped::new(buf);

        prop_assert_eq!(
            expected_len,
            buf.remaining(),
            "encoded_len wrong; expected: {}, actual: {}",
            expected_len,
            buf.remaining()
        );

        let mut roundtrip_value = Default::default();
        while buf.has_remaining() {
            let (decoded_tag, decoded_wire_type) = tr
                .decode_key(buf.buf())
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

            merge(
                wire_type,
                &mut roundtrip_value,
                &mut buf,
                DecodeContext::default(),
            )
            .map_err(|error| TestCaseError::fail(error.to_string()))?;
        }

        prop_assert_eq!(value, roundtrip_value);

        Ok(())
    }

    #[test]
    fn unaligned_fixed64_packed() {
        // Construct a length-delineated field that is not a multiple of 8 bytes.
        let mut buf = Vec::<u8>::new();
        let vals = [0u64, 1, 2, 3];
        encode_varint((vals.len() * 8 + 1) as u64, &mut buf);
        for val in vals {
            buf.put_u64_le(val);
        }
        buf.put_u8(42); // Write an extra byte as part of the field

        let mut parsed = Vec::<u64>::new();
        let res = ufixed64::merge_repeated(
            WireType::LengthDelimited,
            &mut parsed,
            &mut Capped::new(&mut &buf[..]),
            DecodeContext::default(),
        );
        assert!(res.is_err());
        assert_eq!(
            res.unwrap_err().to_string(),
            "failed to decode Bilrost message: packed field is not a valid length"
        );
    }

    #[test]
    fn unaligned_fixed32_packed() {
        // Construct a length-delineated field that is not a multiple of 4 bytes.
        let mut buf = Vec::<u8>::new();
        let vals = [0u32, 1, 2, 3];
        encode_varint((vals.len() * 4 + 1) as u64, &mut buf);
        for val in vals {
            buf.put_u32_le(val);
        }
        buf.put_u8(42); // Write an extra byte as part of the field

        let mut parsed = Vec::<u32>::new();
        let res = ufixed32::merge_repeated(
            WireType::LengthDelimited,
            &mut parsed,
            &mut Capped::new(&mut &buf[..]),
            DecodeContext::default(),
        );
        assert!(res.is_err());
        assert_eq!(
            res.unwrap_err().to_string(),
            "failed to decode Bilrost message: packed field is not a valid length"
        );
    }

    #[test]
    fn string_merge_invalid_utf8() {
        let mut s = String::new();
        let buf = b"\x02\x80\x80";

        let r = string::merge(
            WireType::LengthDelimited,
            &mut s,
            &mut Capped::new(&mut &buf[..]),
            DecodeContext::default(),
        );
        r.expect_err("must be an error");
        assert!(s.is_empty());
    }

    #[test]
    fn varint() {
        fn check(value: u64, mut encoded: &[u8]) {
            // Small buffer.
            let mut buf = Vec::with_capacity(1);
            encode_varint(value, &mut buf);
            assert_eq!(buf, encoded);

            // Large buffer.
            let mut buf = Vec::with_capacity(100);
            encode_varint(value, &mut buf);
            assert_eq!(buf, encoded);

            assert_eq!(encoded_len_varint(value), encoded.len());

            let roundtrip_value =
                decode_varint(&mut <&[u8]>::clone(&encoded)).expect("decoding failed");
            assert_eq!(value, roundtrip_value);

            let roundtrip_value = decode_varint_slow(&mut encoded).expect("slow decoding failed");
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
        let mut u64_max_plus_one: &[u8] = &[0x80, 0xFF, 0xFE, 0xFE, 0xFE, 0xFE, 0xFE, 0xFE, 0xFE];

        decode_varint(&mut u64_max_plus_one).expect_err("decoding u64::MAX + 1 succeeded");
        decode_varint_slow(&mut u64_max_plus_one)
            .expect_err("slow decoding u64::MAX + 1 succeeded");

        let mut u64_over_max: &[u8] = &[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];

        decode_varint(&mut u64_over_max).expect_err("decoding over-max succeeded");
        decode_varint_slow(&mut u64_over_max).expect_err("slow decoding over-max succeeded");
    }

    #[test]
    fn varint_truncated() {
        let mut truncated_one_byte: &[u8] = &[0x80];
        decode_varint(&mut truncated_one_byte).expect_err("decoding truncated 1 byte succeeded");
        decode_varint_slow(&mut truncated_one_byte)
            .expect_err("slow decoding truncated 1 byte succeeded");

        let mut truncated_two_bytes: &[u8] = &[0x80, 0xFF];
        decode_varint(&mut truncated_two_bytes).expect_err("decoding truncated 6 bytes succeeded");
        decode_varint_slow(&mut truncated_two_bytes)
            .expect_err("slow decoding truncated 6 bytes succeeded");

        let mut truncated_six_bytes: &[u8] = &[0x80, 0x81, 0x82, 0x8A, 0x8B, 0x8C];
        decode_varint(&mut truncated_six_bytes).expect_err("decoding truncated 6 bytes succeeded");
        decode_varint_slow(&mut truncated_six_bytes)
            .expect_err("slow decoding truncated 6 bytes succeeded");

        let mut truncated_eight_bytes: &[u8] = &[0x80, 0x81, 0x82, 0x8A, 0x8B, 0x8C, 0xBE, 0xEF];
        decode_varint(&mut truncated_eight_bytes)
            .expect_err("decoding truncated 8 bytes succeeded");
        decode_varint_slow(&mut truncated_eight_bytes)
            .expect_err("slow decoding truncated 8 bytes succeeded");
    }

    /// This big bowl o' macro soup generates an encoding property test for each combination of map
    /// type, scalar map key, and value type.
    /// TODO: these tests take a long time to compile, can this be improved?
    macro_rules! map_tests {
        (keys: $keys:tt,
         vals: $vals:tt) => {
            #[cfg(feature = "std")]
            mod hash_map {
                use ::std::collections::HashMap;
                map_tests!(@private HashMap, hash_map, $keys, $vals);
            }
            mod btree_map {
                use ::alloc::collections::BTreeMap;
                map_tests!(@private BTreeMap, btree_map, $keys, $vals);
            }
        };

        (@private $map_type:ident,
                  $mod_name:ident,
                  [$(($key_ty:ty, $key_proto:ident)),*],
                  $vals:tt) => {
            $(
                mod $key_proto {
                    use super::$map_type;

                    use proptest::prelude::*;

                    use crate::encoding::*;
                    use crate::encoding::test::check_collection_type;

                    map_tests!(@private $map_type, $mod_name, ($key_ty, $key_proto), $vals);
                }
            )*
        };

        (@private $map_type:ident,
                  $mod_name:ident,
                  ($key_ty:ty, $key_proto:ident),
                  [$(($val_ty:ty, $val_proto:ident)),*]) => {
            $(
                proptest! {
                    #[test]
                    fn $val_proto(values: $map_type<$key_ty, $val_ty>, tag: u32) {
                        check_collection_type(values, tag, WireType::LengthDelimited,
                                              |tag, values, buf, tw| {
                                                  $mod_name::encode($key_proto::encode,
                                                                    $key_proto::encoded_len,
                                                                    $val_proto::encode,
                                                                    $val_proto::encoded_len,
                                                                    tag,
                                                                    values,
                                                                    buf,
                                                                    tw)
                                              },
                                              |wire_type, values, buf, ctx| {
                                                  check_wire_type(WireType::LengthDelimited, wire_type)?;
                                                  $mod_name::merge($key_proto::merge,
                                                                   $val_proto::merge,
                                                                   values,
                                                                   buf,
                                                                   ctx)
                                              },
                                              |tag, values, tm| {
                                                  $mod_name::encoded_len($key_proto::encoded_len,
                                                                         $val_proto::encoded_len,
                                                                         tag,
                                                                         values,
                                                                         tm)
                                              })?;
                    }
                }
             )*
        };
    }

    map_tests!(keys: [
        (u32, uint32),
        (u64, uint64),
        (i32, sint32),
        (i64, sint64),
        (u32, ufixed32),
        (u64, ufixed64),
        (i32, sfixed32),
        (i64, sfixed64),
        (bool, bool),
        (String, string),
        (Vec<u8>, bytes)
    ],
    vals: [
        (f32, float32),
        (f64, float64),
        (u32, uint32),
        (u64, uint64),
        (i32, sint32),
        (i64, sint64),
        (u32, ufixed32),
        (u64, ufixed64),
        (i32, sfixed32),
        (i64, sfixed64),
        (bool, bool),
        (String, string),
        (Vec<u8>, bytes)
    ]);
}
