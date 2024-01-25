#![allow(clippy::implicit_hasher, clippy::ptr_arg)]

use alloc::format;
use core::cmp::{min, Eq, PartialEq};
use core::convert::TryFrom;
use core::default::Default;
use core::fmt::Debug;
use core::ops::{Deref, DerefMut};

use bytes::buf::Take;
use bytes::{Buf, BufMut};

use crate::{decode_length_delimiter, DecodeError};

mod fixed;
mod general;
mod map;
/// Tools for opaque encoding and decoding of any valid bilrost data.
pub mod opaque;
mod packed;
mod unpacked;
mod value_traits;
mod vec_blob;

pub use value_traits::{
    Collection, DistinguishedCollection, DistinguishedMapping, Mapping, NewForOverwrite,
};

/// Fixed-size encoder. Encodes integers in fixed-size format.
pub use fixed::Fixed;
/// General encoder. Encodes numbers as varints and repeated types un-packed.
pub use general::General;
/// Encoder for mapping types. Encodes alternating keys and values in packed format.
pub use map::Map;
/// Packed encoder. Encodes repeated types in packed format.
pub use packed::Packed;
/// Unpacked encoder. Encodes repeated types in unpacked format, writing repeated fields.
pub use unpacked::Unpacked;
/// Encoder that decodes bytes data directly into `Vec<u8>`, rather than requiring it to be wrapped
/// in `Blob`.
pub use vec_blob::VecBlob;

/// Encodes an integer value into LEB128-bijective variable length format, and writes it to the
/// buffer. The buffer must have enough remaining space (maximum 9 bytes).
#[inline]
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

/// Decodes a LEB128-bijective-encoded variable length integer from the buffer.
#[inline]
pub fn decode_varint<B: Buf + ?Sized>(buf: &mut B) -> Result<u64, DecodeError> {
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
    u64::checked_add(value, u64::from(buf.get_u8()) << 56)
        .ok_or(DecodeError::new("overflowed varint"))
    // There is probably a reason why using u64::checked_add here seems to cause decoding even
    // smaller varints to bench faster, while using it in the fast-path in decode_varint_slice
    // causes a 5x pessimization. Probably best not to worry about it too much.
}

/// Zig-zag encoding: These functions implement storing signed in unsigned integers by encoding the
/// sign bit in the least significant bit.

#[inline]
fn i32_to_unsigned(value: i32) -> u32 {
    ((value << 1) ^ (value >> 31)) as u32
}

#[inline]
fn u32_to_signed(value: u32) -> i32 {
    ((value >> 1) as i32) ^ (-((value & 1) as i32))
}

#[inline]
fn i64_to_unsigned(value: i64) -> u64 {
    ((value << 1) ^ (value >> 63)) as u64
}

#[inline]
fn u64_to_signed(value: u64) -> i64 {
    ((value >> 1) as i64) ^ (-((value & 1) as i64))
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
    pub(crate) fn enter_recursion(&self) -> DecodeContext {
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
    #[allow(clippy::unnecessary_wraps)] // needed in other features
    pub(crate) fn limit_reached(&self) -> Result<(), DecodeError> {
        #[cfg(not(feature = "no-recursion-limit"))]
        if self.recurse_count == 0 {
            return Err(DecodeError::new("recursion limit reached"));
        }
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
    ThirtyTwoBit = 2,
    SixtyFourBit = 3,
}

impl TryFrom<u64> for WireType {
    type Error = DecodeError;

    #[inline]
    fn try_from(value: u64) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(WireType::Varint),
            1 => Ok(WireType::LengthDelimited),
            2 => Ok(WireType::ThirtyTwoBit),
            3 => Ok(WireType::SixtyFourBit),
            _ => Err(DecodeError::new(format!(
                "invalid wire type value: {}",
                value
            ))),
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
    #[inline]
    pub fn encode_key<B: BufMut + ?Sized>(&mut self, tag: u32, wire_type: WireType, buf: &mut B) {
        let tag_delta = tag
            .checked_sub(self.last_tag)
            .expect("fields encoded out of order");
        self.last_tag = tag;
        encode_varint(((tag_delta as u64) << 2) | (wire_type as u64), buf);
    }
}

/// Simulator for writing tags, capable of outputting their encoded length.
#[derive(Default)]
pub struct TagMeasurer {
    last_tag: u32,
}

impl TagMeasurer {
    pub fn new() -> Self {
        Default::default()
    }

    /// Returns the number of bytes that would be written if the given tag was encoded next, and
    /// also advances the state of the encoder as if that tag was written.
    #[inline]
    pub fn key_len(&mut self, tag: u32) -> usize {
        let tag_delta = tag
            .checked_sub(self.last_tag)
            .expect("fields encoded out of order");
        self.last_tag = tag;
        encoded_len_varint((tag_delta as u64) << 2)
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
        buf: &mut B,
    ) -> Result<(u32, WireType), DecodeError> {
        let key = decode_varint(buf)?;
        let tag_delta =
            u32::try_from(key >> 2).map_err(|_| DecodeError::new("field key overflowed"))?;
        let tag = self
            .last_tag
            .checked_add(tag_delta)
            .ok_or_else(|| DecodeError::new("tag overflowed"))?;
        let wire_type = WireType::try_from(key & 0b11).unwrap();
        self.last_tag = tag;
        Ok((tag, wire_type))
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
            return Err(DecodeError::new("field truncated"));
        }
        Ok(Self {
            buf,
            extra_bytes_remaining: remaining - len,
        })
    }

    pub fn lend(&mut self) -> Capped<B> {
        Capped {
            buf: self.buf,
            extra_bytes_remaining: self.extra_bytes_remaining,
        }
    }

    /// Reads a length delimiter from the beginning of the wrapped buffer, then returns a subsidiary
    /// Capped instance for the delineated bytes if it does not overrun the underlying buffer or
    /// this instance's cap.
    pub fn take_length_delimited(&mut self) -> Result<Capped<B>, DecodeError> {
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

    /// Consume the buffer as an iterator.
    #[inline]
    pub fn consume<F, R, E>(self, read_with: F) -> CappedConsumer<'a, B, F>
    where
        F: FnMut(&mut Capped<B>) -> Result<R, E>,
    {
        CappedConsumer::new(self, read_with)
    }

    #[inline]
    pub fn buf(&mut self) -> &mut B {
        self.buf
    }

    #[inline]
    pub fn take_all(self) -> Take<&'a mut B> {
        let len = self.remaining_before_cap();
        self.buf.take(len)
    }

    #[inline]
    pub fn decode_varint(&mut self) -> Result<u64, DecodeError> {
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

pub struct CappedConsumer<'a, B: Buf + ?Sized, F> {
    capped: Capped<'a, B>,
    reader: F,
}

impl<'a, B: Buf + ?Sized, F> CappedConsumer<'a, B, F> {
    fn new(capped: Capped<'a, B>, reader: F) -> Self {
        Self { capped, reader }
    }
}

impl<'a, B: Buf + ?Sized, T, F> Iterator for CappedConsumer<'a, B, F>
where
    F: FnMut(&mut Capped<B>) -> Result<T, DecodeError>,
{
    type Item = Result<T, DecodeError>;

    fn next(&mut self) -> Option<Result<T, DecodeError>> {
        if self.capped.buf.remaining() == self.capped.extra_bytes_remaining {
            return None;
        }
        let res = (self.reader)(&mut self.capped);
        if res.is_ok() && self.capped.buf.remaining() < self.capped.extra_bytes_remaining {
            return Some(Err(DecodeError::new("delimited length exceeded")));
        }
        Some(res)
    }
}

impl<'a, B: Buf + ?Sized> Deref for Capped<'a, B> {
    type Target = B;

    fn deref(&self) -> &B {
        self.buf
    }
}

impl<'a, B: Buf + ?Sized> DerefMut for Capped<'a, B> {
    fn deref_mut(&mut self) -> &mut B {
        self.buf
    }
}

pub fn skip_field<B: Buf + ?Sized>(
    wire_type: WireType,
    mut buf: Capped<B>,
) -> Result<(), DecodeError> {
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

/// The core trait for encoding and decoding bilrost data.
pub trait Encoder<T> {
    /// Encodes the a field with the given tag and value.
    fn encode<B: BufMut + ?Sized>(tag: u32, value: &T, buf: &mut B, tw: &mut TagWriter);
    // TODO(widders): change to (or augment with) build-in-reverse-then-emit-forward and
    //  emit-reversed
    /// Returns the encoded length of the field, including the key.
    fn encoded_len(tag: u32, value: &T, tm: &mut TagMeasurer) -> usize;
    /// Decodes a field with the given wire type; the field's key should have already been consumed
    /// from the buffer.
    fn decode<B: Buf + ?Sized>(
        wire_type: WireType,
        duplicated: bool,
        value: &mut T,
        buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError>;
}

pub trait DistinguishedEncoder<T>: Encoder<T> {
    /// Decodes a field for the value, returning an error if it is not precisely the encoding that
    /// would have been emitted for the value.
    fn decode_distinguished<B: Buf + ?Sized>(
        wire_type: WireType,
        duplicated: bool,
        value: &mut T,
        buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError>;
}

/// Encoders' wire-type is relied upon by both relaxed and distinguished encoders, but it is written
/// to be a separate trait so that distinguished encoders don't necessarily implement relaxed
/// decoding. This isn't important in general; it's very unlikely anything would implement
/// distinguished decoding without also implementing the corresponding expedient encoding, but
/// this means that it can become a typo to use the relaxed decoding functions by accident when
/// implementing the distinguished encoders, which could cause serious mishaps.
pub trait Wiretyped<T> {
    const WIRE_TYPE: WireType;
}

/// Trait for encoding implementations for raw values that always encode to a single value. Used as
/// the basis for all the other plain, optional, and repeated encodings.
pub trait ValueEncoder<T>: Wiretyped<T> {
    /// Encodes the given value unconditionally. This is guaranteed to emit data to the buffer.
    fn encode_value<B: BufMut + ?Sized>(value: &T, buf: &mut B);
    // TODO(widders): change to (or augment with) build-in-reverse-then-emit-forward and
    //  emit-reversed
    /// Returns the number of bytes the given value would be encoded as.
    fn value_encoded_len(value: &T) -> usize;
    /// Returns the number of total bytes to encode all the values in the given container.
    fn many_values_encoded_len<I>(values: I) -> usize
    where
        I: ExactSizeIterator,
        I::Item: Deref<Target = T>,
    {
        let len = values.len();
        Self::WIRE_TYPE.fixed_size().map_or_else(
            || values.map(|val| Self::value_encoded_len(&val)).sum(),
            |fixed_size| fixed_size * len, // Shortcut when values have a fixed size
        )
    }
    /// Decodes a field assuming the encoder's wire type directly from the buffer.
    fn decode_value<B: Buf + ?Sized>(
        value: &mut T,
        buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError>;
}

pub trait DistinguishedValueEncoder<T>: Wiretyped<T>
where
    T: Eq,
{
    /// Decodes a field assuming the encoder's wire type directly from the buffer, also performing
    /// any additional validation required to guarantee that the value would be re-encoded into the
    /// exact same bytes.
    fn decode_value_distinguished<B: Buf + ?Sized>(
        value: &mut T,
        buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError>;
}

/// Affiliated helper trait for ValueEncoder that provides obligate implementations for handling
/// field keys and wire types.
pub trait FieldEncoder<T> {
    /// Encodes exactly one field with the given tag and value into the buffer.
    fn encode_field<B: BufMut + ?Sized>(tag: u32, value: &T, buf: &mut B, tw: &mut TagWriter);
    /// Returns the encoded length of the field including its key.
    fn field_encoded_len(tag: u32, value: &T, tm: &mut TagMeasurer) -> usize;
    /// Decodes a field directly from the buffer, also checking the wire type.
    fn decode_field<B: Buf + ?Sized>(
        wire_type: WireType,
        value: &mut T,
        buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError>;
}
impl<T, E> FieldEncoder<T> for E
where
    E: ValueEncoder<T>,
{
    #[inline]
    fn encode_field<B: BufMut + ?Sized>(tag: u32, value: &T, buf: &mut B, tw: &mut TagWriter) {
        tw.encode_key(tag, Self::WIRE_TYPE, buf);
        Self::encode_value(value, buf);
    }
    #[inline]
    fn field_encoded_len(tag: u32, value: &T, tm: &mut TagMeasurer) -> usize {
        tm.key_len(tag) + Self::value_encoded_len(value)
    }
    #[inline]
    fn decode_field<B: Buf + ?Sized>(
        wire_type: WireType,
        value: &mut T,
        buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        check_wire_type(Self::WIRE_TYPE, wire_type)?;
        Self::decode_value(value, buf, ctx)
    }
}

/// Affiliated helper trait for DistinguishedValueEncoder that provides obligate implementations for
/// handling field keys and wire types.
pub trait DistinguishedFieldEncoder<T> {
    /// Decodes a field directly from the buffer, also checking the wire type.
    fn decode_field_distinguished<B: Buf + ?Sized>(
        wire_type: WireType,
        value: &mut T,
        buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError>;
}
impl<T, E> DistinguishedFieldEncoder<T> for E
where
    E: DistinguishedValueEncoder<T>,
    T: Eq,
{
    #[inline]
    fn decode_field_distinguished<B: Buf + ?Sized>(
        wire_type: WireType,
        value: &mut T,
        buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        check_wire_type(Self::WIRE_TYPE, wire_type)?;
        Self::decode_value_distinguished(value, buf, ctx)
    }
}

/// Different value encoders may dispatch encoding their plain values slightly differently, but
/// values wrapped in Option are always encoded the same.
impl<T, E> Encoder<Option<T>> for E
where
    E: ValueEncoder<T>,
    T: NewForOverwrite,
{
    #[inline]
    fn encode<B: BufMut + ?Sized>(tag: u32, value: &Option<T>, buf: &mut B, tw: &mut TagWriter) {
        if let Some(value) = value {
            Self::encode_field(tag, value, buf, tw);
        }
    }

    #[inline]
    fn encoded_len(tag: u32, value: &Option<T>, tm: &mut TagMeasurer) -> usize {
        if let Some(value) = value {
            Self::field_encoded_len(tag, value, tm)
        } else {
            0
        }
    }

    #[inline]
    fn decode<B: Buf + ?Sized>(
        wire_type: WireType,
        duplicated: bool,
        value: &mut Option<T>,
        buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        if duplicated {
            return Err(DecodeError::new(
                "multiple occurrences of non-repeated field",
            ));
        }
        Self::decode_field(
            wire_type,
            value.get_or_insert_with(T::new_for_overwrite),
            buf,
            ctx,
        )
    }
}

/// Distinguished decoding for Option<T> is only different in that it calls the distinguished
/// decoding codepath.
impl<T, E> DistinguishedEncoder<Option<T>> for E
where
    E: DistinguishedValueEncoder<T> + Encoder<Option<T>>,
    T: NewForOverwrite + Eq,
{
    #[inline]
    fn decode_distinguished<B: Buf + ?Sized>(
        wire_type: WireType,
        duplicated: bool,
        value: &mut Option<T>,
        buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        if duplicated {
            return Err(DecodeError::new(
                "multiple occurrences of non-repeated field",
            ));
        }
        Self::decode_field_distinguished(
            wire_type,
            value.get_or_insert_with(T::new_for_overwrite),
            buf,
            ctx,
        )
    }
}

/// Trait to be implemented by (or more commonly derived for) oneofs, which have knowledge of their
/// variants' tags and encoding.
pub trait Oneof: Default {
    const FIELD_TAGS: &'static [u32];

    /// Encodes the fields of the oneof into the given buffer.
    fn oneof_encode<B: BufMut + ?Sized>(&self, buf: &mut B, tw: &mut TagWriter);

    /// Measures the number of bytes that would encode this oneof.
    fn oneof_encoded_len(&self, tm: &mut TagMeasurer) -> usize;

    /// Returns the current tag of the oneof, if any.
    fn oneof_current_tag(&self) -> Option<u32>;

    /// Decodes from the given buffer.
    fn oneof_decode_field<B: Buf + ?Sized>(
        &mut self,
        tag: u32,
        wire_type: WireType,
        duplicated: bool,
        buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError>;
}

/// Underlying trait for a oneof that has no inherent "empty" variant, opting instead to be wrapped
/// in an `Option`.
pub trait NonEmptyOneof: Sized {
    const FIELD_TAGS: &'static [u32];

    /// Encodes the fields of the oneof into the given buffer.
    fn oneof_encode<B: BufMut + ?Sized>(&self, buf: &mut B, tw: &mut TagWriter);

    /// Measures the number of bytes that would encode this oneof.
    fn oneof_encoded_len(&self, tm: &mut TagMeasurer) -> usize;

    /// Returns the current tag of the oneof, if any.
    fn oneof_current_tag(&self) -> u32;

    /// Decodes from the given buffer.
    fn oneof_decode_field<B: Buf + ?Sized>(
        value: &mut Option<Self>,
        tag: u32,
        wire_type: WireType,
        duplicated: bool,
        buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError>;
}

impl<T> Oneof for Option<T>
where
    T: NonEmptyOneof,
{
    const FIELD_TAGS: &'static [u32] = T::FIELD_TAGS;

    fn oneof_encode<B: BufMut + ?Sized>(&self, buf: &mut B, tw: &mut TagWriter) {
        if let Some(value) = self {
            value.oneof_encode(buf, tw);
        }
    }

    fn oneof_encoded_len(&self, tm: &mut TagMeasurer) -> usize {
        if let Some(value) = self {
            value.oneof_encoded_len(tm)
        } else {
            0
        }
    }

    fn oneof_current_tag(&self) -> Option<u32> {
        self.as_ref().map(NonEmptyOneof::oneof_current_tag)
    }

    fn oneof_decode_field<B: Buf + ?Sized>(
        &mut self,
        tag: u32,
        wire_type: WireType,
        duplicated: bool,
        buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        T::oneof_decode_field(self, tag, wire_type, duplicated, buf, ctx)
    }
}

/// Trait to be implemented by (or more commonly derived for) oneofs, which have knowledge of their
/// variants' tags and encoding.
pub trait DistinguishedOneof: Oneof {
    /// Decodes from the given buffer in distinguished mode.
    fn oneof_decode_field_distinguished<B: Buf + ?Sized>(
        &mut self,
        tag: u32,
        wire_type: WireType,
        duplicated: bool,
        buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError>;
}

/// Underlying trait for a oneof that has no inherent "empty" variant, opting instead to be wrapped
/// in an `Option`.
pub trait NonEmptyDistinguishedOneof: Sized {
    /// Decodes from the given buffer.
    fn oneof_decode_field_distinguished<B: Buf + ?Sized>(
        value: &mut Option<Self>,
        tag: u32,
        wire_type: WireType,
        duplicated: bool,
        buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError>;
}

impl<T> DistinguishedOneof for Option<T>
where
    T: NonEmptyDistinguishedOneof,
    Self: Oneof,
{
    fn oneof_decode_field_distinguished<B: Buf + ?Sized>(
        &mut self,
        tag: u32,
        wire_type: WireType,
        duplicated: bool,
        buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        T::oneof_decode_field_distinguished(self, tag, wire_type, duplicated, buf, ctx)
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
    T: Into<u32> + TryFrom<u32, Error = DecodeError>,
{
    type Input = T;
    type Output = Result<T, DecodeError>;

    fn help_set(enum_val: Self) -> u32 {
        enum_val.into()
    }

    fn help_get(field_val: u32) -> Result<T, DecodeError> {
        T::try_from(field_val)
    }
}
impl<T> EnumerationHelper<Option<u32>> for T
where
    T: Into<u32> + TryFrom<u32, Error = DecodeError>,
{
    type Input = Option<T>;
    type Output = Option<Result<T, DecodeError>>;

    fn help_set(enum_val: Option<T>) -> Option<u32> {
        enum_val.map(Into::into)
    }

    fn help_get(field_val: Option<u32>) -> Option<Result<T, DecodeError>> {
        field_val.map(TryFrom::try_from)
    }
}

/// Macro rules for expressly delegating from one encoder to another.
macro_rules! delegate_encoding {
    (
        delegate from ($from_ty:ty) to ($to_ty:ty) for type ($value_ty:ty)
        $(with generics <$($value_generics:ident),+>)?
    ) => {
        impl$(<$($value_generics),+>)? $crate::encoding::Encoder<$value_ty> for $from_ty
        where
            $to_ty: $crate::encoding::Encoder<$value_ty>,
        {
            #[inline]
            fn encode<B: $crate::bytes::BufMut + ?Sized>(
                tag: u32,
                value: &$value_ty,
                buf: &mut B,
                tw: &mut $crate::encoding::TagWriter,
            ) {
                <$to_ty>::encode(tag, value, buf, tw)
            }

            #[inline]
            fn encoded_len(
                tag: u32,
                value: &$value_ty,
                tm: &mut $crate::encoding::TagMeasurer,
            ) -> usize {
                <$to_ty>::encoded_len(tag, value, tm)
            }

            #[inline]
            fn decode<B: $crate::bytes::Buf + ?Sized>(
                wire_type: $crate::encoding::WireType,
                duplicated: bool,
                value: &mut $value_ty,
                buf: $crate::encoding::Capped<B>,
                ctx: $crate::encoding::DecodeContext,
            ) -> Result<(), DecodeError> {
                <$to_ty>::decode(wire_type, duplicated, value, buf, ctx)
            }
        }
    };

    (
        delegate from ($from_ty:ty) to ($to_ty:ty) for type ($value_ty:ty) including distinguished
        $(with generics <$($value_generics:ident),+>)?
    ) => {
        delegate_encoding!(
            delegate from ($from_ty) to ($to_ty) for type ($value_ty)
            $(with generics <$($value_generics),+>)?
        );

        impl$(<$($value_generics),+>)? $crate::encoding::DistinguishedEncoder<$value_ty>
        for $from_ty
        where
            $to_ty: $crate::encoding::DistinguishedEncoder<$value_ty>,
            Self: $crate::encoding::Encoder<$value_ty>,
        {
            #[inline]
            fn decode_distinguished<B: $crate::bytes::Buf + ?Sized>(
                wire_type: $crate::encoding::WireType,
                duplicated: bool,
                value: &mut $value_ty,
                buf: $crate::encoding::Capped<B>,
                ctx: $crate::encoding::DecodeContext,
            ) -> Result<(), $crate::DecodeError> {
                <$to_ty>::decode_distinguished(wire_type, duplicated, value, buf, ctx)
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
        $(with generics <$($value_generics:ident),+>)?
    ) => {
        impl$(<$($value_generics),+>)? $crate::encoding::Wiretyped<$value_ty> for $from_ty
        where
            $to_ty: $crate::encoding::Wiretyped<$value_ty>,
            $($($where_clause)+ ,)?
        {
            const WIRE_TYPE: $crate::encoding::WireType =
                <$to_ty as $crate::encoding::Wiretyped<$value_ty>>::WIRE_TYPE;
        }

        impl$(<$($value_generics),+>)? $crate::encoding::ValueEncoder<$value_ty> for $from_ty
        where
            $to_ty: $crate::encoding::ValueEncoder<$value_ty>,
            $($($where_clause)+ ,)?
        {
            #[inline]
            fn encode_value<B: $crate::bytes::BufMut + ?Sized>(value: &$value_ty, buf: &mut B) {
                <$to_ty>::encode_value(value, buf)
            }

            #[inline]
            fn value_encoded_len(value: &$value_ty) -> usize {
                <$to_ty>::value_encoded_len(value)
            }

            #[inline]
            fn many_values_encoded_len<I>(values: I) -> usize
            where
                I: ExactSizeIterator,
                I::Item: core::ops::Deref<Target = $value_ty>,
            {
                <$to_ty>::many_values_encoded_len(values)
            }

            #[inline]
            fn decode_value<B: $crate::bytes::Buf + ?Sized>(
                value: &mut $value_ty,
                buf: $crate::encoding::Capped<B>,
                ctx: $crate::encoding::DecodeContext,
            ) -> Result<(), $crate::DecodeError> {
                <$to_ty>::decode_value(value, buf, ctx)
            }
        }
    };

    (
        delegate from ($from_ty:ty) to ($to_ty:ty) for type ($value_ty:ty) including distinguished
        $(with where clause for expedient ($($expedient_where:tt)+))?
        $(with where clause for distinguished ($($distinguished_where:tt)+))?
        $(with generics <$($value_generics:ident),+>)?
    ) => {
        delegate_value_encoding!(
            delegate from ($from_ty) to ($to_ty) for type ($value_ty)
            $(with where clause ($($expedient_where)+))?
            $(with generics <$($value_generics),+>)?
        );

        impl$(<$($value_generics),+>)? $crate::encoding::DistinguishedValueEncoder<$value_ty>
        for $from_ty
        where
            $to_ty: $crate::encoding::DistinguishedValueEncoder<$value_ty>,
            $($($expedient_where)+ ,)?
            $($($distinguished_where)+ ,)?
        {
            #[inline]
            fn decode_value_distinguished<B: Buf + ?Sized>(
                value: &mut $value_ty,
                buf: $crate::encoding::Capped<B>,
                ctx: $crate::encoding::DecodeContext,
            ) -> Result<(), $crate::DecodeError> {
                <$to_ty>::decode_value_distinguished(value, buf, ctx)
            }
        }
    };
}
pub(crate) use delegate_value_encoding;

/// Generalized proptest macro. Kind must be either `expedient`, `hashable`, or `distinguished`.
macro_rules! check_type_test {
    ($encoder:ty, $kind:ident, $ty:ty, $wire_type:expr) => {
        crate::encoding::check_type_test!($encoder, $kind, from $ty, into $ty, $wire_type);
    };
    ($encoder:ty, $kind:ident, from $from_ty:ty, into $into_ty:ty, $wire_type:expr) => {
        #[cfg(test)]
        mod $kind {
            use alloc::vec::Vec;

            use proptest::prelude::*;

            use crate::encoding::test::$kind::{check_type, check_type_unpacked};
            #[allow(unused_imports)]
            use crate::encoding::{General, Fixed, Map, Packed, Unpacked, VecBlob, WireType};

            proptest! {
                #[test]
                fn check(value: $from_ty, tag: u32) {
                    check_type::<$into_ty, $encoder>(<$into_ty>::from(value), tag, $wire_type)?;
                }
                #[test]
                fn check_optional(value: Option<$from_ty>, tag: u32) {
                    check_type::<Option<$into_ty>, $encoder>(
                        value.map(<$into_ty>::from),
                        tag,
                        $wire_type,
                    )?;
                }
            }
            // TODO(widders): check the appropriate combination of ord (btree) and distinguished
            //  (not hash)
            crate::encoding::check_type_test_functions!(
                $encoder, from $from_ty, into $into_ty, in Vec, vec, $wire_type);
            // TODO(widders): check expedient decoding between numeric packed and unpacked
        }
    };
}
pub(crate) use check_type_test;
#[allow(unused_macros)]
macro_rules! check_type_test_functions {
    (
        $encoder:ty, from $from_ty:ty, into $into_ty:ty,
        in $container:ident, $module:ident, $wire_type:expr
    ) => {
        mod $module {
            use super::*;
            proptest! {
                #[test]
                fn check_unpacked(value: $container<$from_ty>, tag: u32) {
                    check_type_unpacked::<$container<$into_ty>, Unpacked<$encoder>>(
                        value.into_iter().map(|val| <$into_ty>::from(val)).collect(),
                        tag,
                        $wire_type,
                    )?;
                }
                #[test]
                fn check_packed(value: $container<$from_ty>, tag: u32) {
                    check_type::<$container<$into_ty>, Packed<$encoder>>(
                        value.into_iter().map(|val| <$into_ty>::from(val)).collect(),
                        tag,
                        WireType::LengthDelimited,
                    )?;
                }
            }
        }
    };
}
#[allow(unused_imports)]
pub(crate) use check_type_test_functions;

#[cfg(test)]
mod test {
    use alloc::collections::BTreeMap;
    use alloc::string::{String, ToString};
    use alloc::vec::Vec;
    use core::borrow::Borrow;
    use core::fmt::Debug;

    use proptest::{prelude::*, test_runner::TestCaseResult};

    use crate::encoding::*;

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
        ($kind:ident, $encoder_trait:ident, $decode:ident) => {
            pub mod $kind {
                use super::*;
                use bytes::BytesMut;

                pub fn check_type<T, E>(value: T, tag: u32, wire_type: WireType) -> TestCaseResult
                where
                    T: Debug + NewForOverwrite + PartialEq,
                    E: $encoder_trait<T>,
                {
                    let expected_len = E::encoded_len(tag, &value, &mut TagMeasurer::new());

                    let mut buf = BytesMut::with_capacity(expected_len);
                    E::encode(tag, &value, &mut buf, &mut TagWriter::new());

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

                    check_legal_remaining(tag, wire_type, buf.remaining())?;

                    let mut roundtrip_value = T::new_for_overwrite();
                    E::$decode(
                        wire_type,
                        false,
                        &mut roundtrip_value,
                        buf.lend(),
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

                pub fn check_type_unpacked<T, E>(
                    value: T,
                    tag: u32,
                    wire_type: WireType,
                ) -> TestCaseResult
                where
                    T: Debug + NewForOverwrite + PartialEq,
                    E: $encoder_trait<T>,
                {
                    let expected_len = E::encoded_len(tag, value.borrow(), &mut TagMeasurer::new());

                    let mut buf = BytesMut::with_capacity(expected_len);
                    E::encode(tag, value.borrow(), &mut buf, &mut TagWriter::new());

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

                    let mut roundtrip_value = T::new_for_overwrite();
                    let mut not_first = false;
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

                        E::$decode(
                            wire_type,
                            not_first,
                            &mut roundtrip_value,
                            buf.lend(),
                            DecodeContext::default(),
                        )
                        .map_err(|error| TestCaseError::fail(error.to_string()))?;
                        not_first = true;
                    }

                    prop_assert_eq!(value, roundtrip_value);

                    Ok(())
                }
            }
        };
    }
    check_type!(expedient, Encoder, decode);
    check_type!(distinguished, DistinguishedEncoder, decode_distinguished);

    #[test]
    fn unaligned_fixed64_packed() {
        // Construct a length-delineated field that is not a multiple of 8 bytes.
        let mut buf = Vec::<u8>::new();
        encode_varint(12, &mut buf);
        buf.extend([1; 12]);

        let mut parsed = Vec::<u64>::new();
        let res = <Packed<Fixed>>::decode_value(
            &mut parsed,
            Capped::new(&mut buf.as_slice()),
            DecodeContext::default(),
        );
        assert_eq!(
            res.expect_err("unaligned packed fixed64 decoded without error")
                .to_string(),
            "failed to decode Bilrost message: packed field is not a valid length"
        );
        let res = <Packed<Fixed>>::decode_value_distinguished(
            &mut parsed,
            Capped::new(&mut buf.as_slice()),
            DecodeContext::default(),
        );
        assert_eq!(
            res.expect_err("unaligned packed fixed64 decoded without error")
                .to_string(),
            "failed to decode Bilrost message: packed field is not a valid length"
        );
    }

    #[test]
    fn unaligned_fixed32_packed() {
        // Construct a length-delineated field that is not a multiple of 4 bytes.
        let mut buf = Vec::<u8>::new();
        encode_varint(17, &mut buf);
        buf.extend([1; 17]);

        let mut parsed = Vec::<u32>::new();
        let res = <Packed<Fixed>>::decode_value(
            &mut parsed,
            Capped::new(&mut buf.as_slice()),
            DecodeContext::default(),
        );
        assert_eq!(
            res.expect_err("unaligned packed fixed32 decoded without error")
                .to_string(),
            "failed to decode Bilrost message: packed field is not a valid length"
        );
        let res = <Packed<Fixed>>::decode_value_distinguished(
            &mut parsed,
            Capped::new(&mut buf.as_slice()),
            DecodeContext::default(),
        );
        assert_eq!(
            res.expect_err("unaligned packed fixed32 decoded without error")
                .to_string(),
            "failed to decode Bilrost message: packed field is not a valid length"
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
        let res = <Map<Fixed, Fixed>>::decode_value(
            &mut parsed,
            Capped::new(&mut buf.as_slice()),
            DecodeContext::default(),
        );
        assert_eq!(
            res.expect_err("unaligned 12-byte map decoded without error")
                .to_string(),
            "failed to decode Bilrost message: packed field is not a valid length"
        );
        let res = <Map<Fixed, Fixed>>::decode_value_distinguished(
            &mut parsed,
            Capped::new(&mut buf.as_slice()),
            DecodeContext::default(),
        );
        assert_eq!(
            res.expect_err("unaligned 12-byte map decoded without error")
                .to_string(),
            "failed to decode Bilrost message: packed field is not a valid length"
        );
    }

    #[test]
    fn string_merge_invalid_utf8() {
        let mut s = String::new();
        let buf = b"\x02\x80\x80";

        let r = General::decode_value(
            &mut s,
            Capped::new(&mut buf.as_slice()),
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
}
