use crate::buf::ReverseBuf;
use crate::encoding::{
    check_wire_type, Capped, DecodeContext, ForOverwrite, RestrictedDecodeContext, TagMeasurer,
    TagRevWriter, TagWriter, WireType,
};
use crate::{Canonicity, DecodeError};
use bytes::{Buf, BufMut};
use core::ops::Deref;

/// The core trait for encoding bilrost data.
pub trait Encoder<E, T: ?Sized> {
    /// Encodes the a field with the given tag and value.
    fn encode<B: BufMut + ?Sized>(tag: u32, value: &T, buf: &mut B, tw: &mut TagWriter);

    /// Prepends the encoding of the field with the given tag and value.
    fn prepend_encode<B: ReverseBuf + ?Sized>(
        tag: u32,
        value: &T,
        buf: &mut B,
        tw: &mut TagRevWriter,
    );

    /// Returns the encoded length of the field, including the key.
    fn encoded_len(tag: u32, value: &T, tm: &mut impl TagMeasurer) -> usize;
}

/// The core trait for decoding bilrost data. Data must always be copied from the buffer.
pub trait Decoder<E, T>: Encoder<E, T> {
    /// Decodes a field's value with the given wire type; the field's key should have already been
    /// consumed from the buffer.
    fn decode<B: Buf + ?Sized>(
        wire_type: WireType,
        value: &mut T,
        buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError>;
}

/// Decoding trait for canonical decoding. Distinguished decoding is available via
/// this trait, and any type that implements this trait is guaranteed to always emit canonical data
/// via `Encoder`.
pub trait DistinguishedDecoder<E, T>: Encoder<E, T> {
    /// Decodes a field for the value, returning a value indicating how canonical the encoding was.
    fn decode_distinguished<B: Buf + ?Sized>(
        wire_type: WireType,
        value: &mut T,
        buf: Capped<B>,
        ctx: RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError>;
}

/// Decoding trait that allows decoding borrowed data.
pub trait BorrowDecoder<'a, E, T>: Encoder<E, T> {
    fn borrow_decode(
        wire_type: WireType,
        value: &mut T,
        buf: Capped<&'a [u8]>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError>;
}

/// Decoding trait that allows distinguished decoding of borrowed data.
pub trait DistinguishedBorrowDecoder<'a, E, T>: Encoder<E, T> {
    /// Decodes a field for the value, returning a value indicating how canonical the encoding was.
    fn borrow_decode_distinguished(
        wire_type: WireType,
        value: &mut T,
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
pub trait Wiretyped<E, T: ?Sized> {
    const WIRE_TYPE: WireType;
}

/// The core trait for encoding implementations for raw values that always encode to a single value.
/// This is the basis for all the other plain, optional, and repeated encodings.
pub trait ValueEncoder<E, T: ?Sized>: Wiretyped<E, T> {
    /// Encodes the given value unconditionally. This is guaranteed to emit data to the buffer.
    fn encode_value<B: BufMut + ?Sized>(value: &T, buf: &mut B);

    /// Prepends the given value unconditionally. This is guaranteed to emit data to the buffer.
    fn prepend_value<B: ReverseBuf + ?Sized>(value: &T, buf: &mut B);

    /// Returns the number of bytes the given value would be encoded as.
    fn value_encoded_len(value: &T) -> usize;

    /// Returns the number of total bytes to encode all the values in the given container.
    #[inline]
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
}

/// The core trait for decoding single values in relaxed mode. Data is always copies when it is
/// read from the buffer.
pub trait ValueDecoder<E, T>: ValueEncoder<E, T> {
    /// Decodes a field assuming the encoder's wire type directly from the buffer.
    fn decode_value<B: Buf + ?Sized>(
        value: &mut T,
        buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError>;
}

/// The core trait for decoding single values in distinguished mode. Data is always copies when it
/// is read from the buffer.
pub trait DistinguishedValueDecoder<E, T>: ValueEncoder<E, T> + Eq {
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
        value: &mut T,
        buf: Capped<impl Buf + ?Sized>,
        ctx: RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError>;
}

/// Value-decoding trait for decoding borrowed values.
pub trait ValueBorrowDecoder<'a, E, T>: ValueEncoder<E, T> {
    /// Decodes a field assuming the encoder's wire type directly from the buffer.
    fn borrow_decode_value(
        value: &mut T,
        buf: Capped<&'a [u8]>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError>;
}

/// Value-decoding trait for distinguished decoding of borrowed values.
pub trait DistinguishedValueBorrowDecoder<'a, E, T>: ValueEncoder<E, T> + Eq {
    /// Indicates whether the `ALLOW_EMPTY` argument in `borrow_decode_value_distinguished` has any
    /// effect. Some decoder implementations can more cheaply determine whether they were empty
    /// during decoding, and will return `NotCanonical` if `ALLOW_EMPTY` was false; for these
    /// implementations, `CHECKS_EMPTY` should be set to `true`. When `CHECKS_EMPTY` is `false`, the
    /// caller must invoke `EmptyState::is_empty` after the call if empty states are non-canonical.
    const CHECKS_EMPTY: bool;

    /// Decodes a field assuming the encoder's wire type directly from the buffer, also performing
    /// any additional validation required to guarantee that the value would be re-encoded into the
    /// exact same bytes.
    fn borrow_decode_value_distinguished<const ALLOW_EMPTY: bool>(
        value: &mut T,
        buf: Capped<&'a [u8]>,
        ctx: RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError>;
}

/// Affiliated helper trait for ValueEncoder that provides obligate implementations for handling
/// field keys and wire types.
pub trait FieldEncoder<E, T>: ValueEncoder<E, T> {
    /// Encodes exactly one field with the given tag and value into the buffer.
    fn encode_field<B: BufMut + ?Sized>(tag: u32, value: &T, buf: &mut B, tw: &mut TagWriter);

    /// Prepends exactly one field with the given tag and value into the buffer.
    fn prepend_field<B: ReverseBuf + ?Sized>(
        tag: u32,
        value: &T,
        buf: &mut B,
        tw: &mut TagRevWriter,
    );

    /// Returns the encoded length of the field including its key.
    fn field_encoded_len(tag: u32, value: &T, tm: &mut impl TagMeasurer) -> usize;
}

/// Affiliated helper trait for ValueDecoder that provides obligate implementations for handling
/// field keys and wire types.
pub trait FieldDecoder<E, T>: ValueDecoder<E, T> {
    /// Decodes a field directly from the buffer, also checking the wire type.
    fn decode_field<B: Buf + ?Sized>(
        wire_type: WireType,
        value: &mut T,
        buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError>;
}

/// Affiliated helper trait for DistinguishedValueDecoder that provides obligate implementations for
/// handling field keys and wire types.
pub trait DistinguishedFieldDecoder<E, T>: DistinguishedValueDecoder<E, T> {
    /// Decodes a field directly from the buffer, also checking the wire type.
    fn decode_field_distinguished<const ALLOW_EMPTY: bool>(
        wire_type: WireType,
        value: &mut T,
        buf: Capped<impl Buf + ?Sized>,
        ctx: RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError>;
}

/// Affiliated helper trait for BorrowDecoder that provides obligate implementations for handling
/// field keys and wire types.
pub trait FieldBorrowDecoder<'a, E, T>: ValueBorrowDecoder<'a, E, T> {
    /// Decodes a field directly from the buffer, also checking the wire type.
    fn borrow_decode_field(
        wire_type: WireType,
        value: &mut T,
        buf: Capped<&'a [u8]>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError>;
}

/// Affiliated helper trait for DistinguishedBorrowDecoder that provides obligate implementations
/// for handling field keys and wire types.
pub trait DistinguishedFieldBorrowDecoder<'a, E, T>:
    DistinguishedValueBorrowDecoder<'a, E, T>
{
    /// Decodes a field directly from the buffer, also checking the wire type.
    fn borrow_decode_field_distinguished<const ALLOW_EMPTY: bool>(
        wire_type: WireType,
        value: &mut T,
        buf: Capped<&'a [u8]>,
        ctx: RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError>;
}

impl<T, E> FieldEncoder<E, T> for ()
where
    (): ValueEncoder<E, T>,
{
    #[inline]
    fn encode_field<B: BufMut + ?Sized>(tag: u32, value: &T, buf: &mut B, tw: &mut TagWriter) {
        tw.encode_key(tag, <() as Wiretyped<E, T>>::WIRE_TYPE, buf);
        <() as ValueEncoder<E, T>>::encode_value(value, buf);
    }

    #[inline]
    fn prepend_field<B: ReverseBuf + ?Sized>(
        tag: u32,
        value: &T,
        buf: &mut B,
        tw: &mut TagRevWriter,
    ) {
        tw.begin_field(tag, <() as Wiretyped<E, T>>::WIRE_TYPE, buf);
        <() as ValueEncoder<E, T>>::prepend_value(value, buf);
    }

    #[inline]
    fn field_encoded_len(tag: u32, value: &T, tm: &mut impl TagMeasurer) -> usize {
        tm.key_len(tag) + <() as ValueEncoder<E, T>>::value_encoded_len(value)
    }
}

impl<T, E> FieldDecoder<E, T> for ()
where
    (): ValueDecoder<E, T>,
{
    #[inline]
    fn decode_field<B: Buf + ?Sized>(
        wire_type: WireType,
        value: &mut T,
        buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        check_wire_type(<() as Wiretyped<E, T>>::WIRE_TYPE, wire_type)?;
        <() as ValueDecoder<E, T>>::decode_value(value, buf, ctx)
    }
}

impl<T, E> DistinguishedFieldDecoder<E, T> for ()
where
    (): DistinguishedValueDecoder<E, T>,
{
    #[inline(always)]
    fn decode_field_distinguished<const ALLOW_EMPTY: bool>(
        wire_type: WireType,
        value: &mut T,
        buf: Capped<impl Buf + ?Sized>,
        ctx: RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError> {
        check_wire_type(<() as Wiretyped<E, T>>::WIRE_TYPE, wire_type)?;
        <() as DistinguishedValueDecoder<E, T>>::decode_value_distinguished::<ALLOW_EMPTY>(
            value, buf, ctx,
        )
    }
}

impl<'a, T, E> FieldBorrowDecoder<'a, E, T> for ()
where
    (): ValueBorrowDecoder<'a, E, T>,
{
    #[inline]
    fn borrow_decode_field(
        wire_type: WireType,
        value: &mut T,
        buf: Capped<&'a [u8]>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        check_wire_type(<() as Wiretyped<E, T>>::WIRE_TYPE, wire_type)?;
        <() as ValueBorrowDecoder<E, T>>::borrow_decode_value(value, buf, ctx)
    }
}

impl<'a, T, E> DistinguishedFieldBorrowDecoder<'a, E, T> for ()
where
    (): DistinguishedValueBorrowDecoder<'a, E, T>,
{
    #[inline(always)]
    fn borrow_decode_field_distinguished<const ALLOW_EMPTY: bool>(
        wire_type: WireType,
        value: &mut T,
        buf: Capped<&'a [u8]>,
        ctx: RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError> {
        check_wire_type(<() as Wiretyped<E, T>>::WIRE_TYPE, wire_type)?;
        <() as DistinguishedValueBorrowDecoder<E, T>>::borrow_decode_value_distinguished::<
            ALLOW_EMPTY,
        >(value, buf, ctx)
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

    impl<T, E> Encoder<E, Option<T>> for ()
    where
        (): ValueEncoder<E, T> + ForOverwrite<E, T>,
    {
        #[inline]
        fn encode<B: BufMut + ?Sized>(
            tag: u32,
            value: &Option<T>,
            buf: &mut B,
            tw: &mut TagWriter,
        ) {
            if let Some(value) = value {
                <() as FieldEncoder<E, T>>::encode_field(tag, value, buf, tw);
            }
        }

        #[inline]
        fn prepend_encode<B: ReverseBuf + ?Sized>(
            tag: u32,
            value: &Option<T>,
            buf: &mut B,
            tw: &mut TagRevWriter,
        ) {
            if let Some(value) = value {
                <() as FieldEncoder<E, T>>::prepend_field(tag, value, buf, tw)
            }
        }

        #[inline]
        fn encoded_len(tag: u32, value: &Option<T>, tm: &mut impl TagMeasurer) -> usize {
            if let Some(value) = value {
                <() as FieldEncoder<E, T>>::field_encoded_len(tag, value, tm)
            } else {
                0
            }
        }
    }

    impl<T, E> Decoder<E, Option<T>> for ()
    where
        (): ValueDecoder<E, T> + ForOverwrite<E, T>,
    {
        #[inline]
        fn decode<B: Buf + ?Sized>(
            wire_type: WireType,
            value: &mut Option<T>,
            buf: Capped<B>,
            ctx: DecodeContext,
        ) -> Result<(), DecodeError> {
            <() as FieldDecoder<E, T>>::decode_field(
                wire_type,
                value.get_or_insert_with(<() as ForOverwrite<E, T>>::for_overwrite),
                buf,
                ctx,
            )
        }
    }

    impl<T, E> DistinguishedDecoder<E, Option<T>> for ()
    where
        Option<T>: Eq,
        (): DistinguishedValueDecoder<E, T> + ForOverwrite<E, T>,
    {
        #[inline]
        fn decode_distinguished<B: Buf + ?Sized>(
            wire_type: WireType,
            value: &mut Option<T>,
            buf: Capped<B>,
            ctx: RestrictedDecodeContext,
        ) -> Result<Canonicity, DecodeError> {
            check_wire_type(<() as Wiretyped<E, T>>::WIRE_TYPE, wire_type)?;
            <() as DistinguishedValueDecoder<E, T>>::decode_value_distinguished::<true>(
                value.get_or_insert_with(<() as ForOverwrite<E, T>>::for_overwrite),
                buf,
                ctx,
            )
        }
    }

    impl<'a, T, E> BorrowDecoder<'a, E, Option<T>> for ()
    where
        (): ValueBorrowDecoder<'a, E, T> + ForOverwrite<E, T>,
    {
        #[inline]
        fn borrow_decode(
            wire_type: WireType,
            value: &mut Option<T>,
            buf: Capped<&'a [u8]>,
            ctx: DecodeContext,
        ) -> Result<(), DecodeError> {
            <() as FieldBorrowDecoder<E, T>>::borrow_decode_field(
                wire_type,
                value.get_or_insert_with(<() as ForOverwrite<E, T>>::for_overwrite),
                buf,
                ctx,
            )
        }
    }

    impl<'a, T, E> DistinguishedBorrowDecoder<'a, E, Option<T>> for ()
    where
        Option<T>: Eq,
        (): DistinguishedValueBorrowDecoder<'a, E, T> + ForOverwrite<E, T>,
    {
        #[inline]
        fn borrow_decode_distinguished(
            wire_type: WireType,
            value: &mut Option<T>,
            buf: Capped<&'a [u8]>,
            ctx: RestrictedDecodeContext,
        ) -> Result<Canonicity, DecodeError> {
            check_wire_type(<() as Wiretyped<E, T>>::WIRE_TYPE, wire_type)?;
            <() as DistinguishedValueBorrowDecoder<E, T>>::borrow_decode_value_distinguished::<true>(
                value.get_or_insert_with(<() as ForOverwrite<E, T>>::for_overwrite),
                buf,
                ctx,
            )
        }
    }
}
