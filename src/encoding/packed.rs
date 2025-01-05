use bytes::{Buf, BufMut};

use crate::buf::ReverseBuf;
use crate::encoding::value_traits::{
    Collection, DistinguishedCollection, EmptyState, ForOverwrite,
};
use crate::encoding::{
    encode_varint, encoded_len_varint, prepend_varint, unpacked, Canonicity, Capped, DecodeContext,
    DecodeError, Decoder, DistinguishedDecoder, DistinguishedValueDecoder, Encoder, FieldEncoder,
    General, RestrictedDecodeContext, TagMeasurer, TagRevWriter, TagWriter, ValueDecoder,
    ValueEncoder, WireType, Wiretyped,
};
use crate::DecodeErrorKind::{InvalidValue, Truncated, UnexpectedlyRepeated};

pub struct Packed<E = General>(E);

/// Packed encodings are always length delimited.
impl<T, E> Wiretyped<Packed<E>> for T {
    const WIRE_TYPE: WireType = WireType::LengthDelimited;
}

impl<C, T, E> ValueEncoder<Packed<E>> for C
where
    C: Collection<Item = T>,
    T: ForOverwrite + ValueEncoder<E>,
{
    #[inline]
    fn encode_value<B: BufMut + ?Sized>(value: &C, buf: &mut B) {
        encode_varint(
            ValueEncoder::<E>::many_values_encoded_len(value.iter()) as u64,
            buf,
        );
        for val in value.iter() {
            ValueEncoder::<E>::encode_value(val, buf);
        }
    }

    #[inline]
    fn prepend_value<B: ReverseBuf + ?Sized>(value: &Self, buf: &mut B) {
        let end = buf.remaining();
        for val in value.reversed() {
            <T as ValueEncoder<E>>::prepend_value(val, buf);
        }
        prepend_varint((buf.remaining() - end) as u64, buf);
    }

    #[inline]
    fn value_encoded_len(value: &C) -> usize {
        let inner_len = ValueEncoder::<E>::many_values_encoded_len(value.iter());
        encoded_len_varint(inner_len as u64)
            .checked_add(inner_len)
            .unwrap()
    }
}

impl<C, T, E> ValueDecoder<Packed<E>> for C
where
    C: Collection<Item = T>,
    T: ForOverwrite + ValueDecoder<E>,
{
    #[inline]
    fn decode_value<B: Buf + ?Sized>(
        value: &mut C,
        mut buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        let mut capped = buf.take_length_delimited()?;
        // MSRV: this could be .is_some_and(..)
        if matches!(
            <T as Wiretyped<E>>::WIRE_TYPE.fixed_size(),
            Some(fixed_size) if capped.remaining_before_cap() % fixed_size != 0
        ) {
            // No number of fixed-sized values can pack evenly into this size.
            return Err(DecodeError::new(Truncated));
        }
        while capped.has_remaining()? {
            let mut new_val = T::for_overwrite();
            ValueDecoder::<E>::decode_value(&mut new_val, capped.lend(), ctx.clone())?;
            value.insert(new_val)?;
        }
        Ok(())
    }
}

impl<C, T, E> DistinguishedValueDecoder<Packed<E>> for C
where
    C: DistinguishedCollection<Item = T> + Eq,
    T: ForOverwrite + Eq + DistinguishedValueDecoder<E>,
{
    const CHECKS_EMPTY: bool = false;

    #[inline]
    fn decode_value_distinguished<const ALLOW_EMPTY: bool>(
        value: &mut C,
        mut buf: Capped<impl Buf + ?Sized>,
        ctx: RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError> {
        let mut capped = buf.take_length_delimited()?;
        // MSRV: this could be .is_some_and(..)
        if matches!(
            <T as Wiretyped<E>>::WIRE_TYPE.fixed_size(),
            Some(fixed_size) if capped.remaining_before_cap() % fixed_size != 0
        ) {
            // No number of fixed-sized values can pack evenly into this size.
            return Err(DecodeError::new(Truncated));
        }
        let canon = &mut Canonicity::Canonical;
        while capped.has_remaining()? {
            let mut new_val = T::for_overwrite();
            ctx.update(
                canon,
                DistinguishedValueDecoder::<E>::decode_value_distinguished::<true>(
                    &mut new_val,
                    capped.lend(),
                    ctx.clone(),
                )?,
            )?;
            ctx.update(canon, value.insert_distinguished(new_val)?)?;
        }
        Ok(*canon)
    }
}

/// ValueEncoder for packed repeated encodings lets this value type nest.
impl<C, T, E> Encoder<Packed<E>> for C
where
    C: Collection<Item = T> + ValueEncoder<Packed<E>>,
    T: ForOverwrite + ValueEncoder<E>,
{
    #[inline]
    fn encode<B: BufMut + ?Sized>(tag: u32, value: &C, buf: &mut B, tw: &mut TagWriter) {
        if !value.is_empty() {
            Self::encode_field(tag, value, buf, tw);
        }
    }

    #[inline]
    fn prepend_encode<B: ReverseBuf + ?Sized>(
        tag: u32,
        value: &Self,
        buf: &mut B,
        tw: &mut TagRevWriter,
    ) {
        if !value.is_empty() {
            Self::prepend_field(tag, value, buf, tw);
        }
    }

    #[inline]
    fn encoded_len(tag: u32, value: &C, tm: &mut impl TagMeasurer) -> usize {
        if !value.is_empty() {
            Self::field_encoded_len(tag, value, tm)
        } else {
            0
        }
    }
}

impl<C, T, E> Decoder<Packed<E>> for C
where
    C: Collection<Item = T> + ValueDecoder<Packed<E>>,
    T: ForOverwrite + ValueDecoder<E>,
{
    #[inline]
    fn decode<B: Buf + ?Sized>(
        wire_type: WireType,
        duplicated: bool,
        value: &mut C,
        buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        if duplicated {
            return Err(DecodeError::new(UnexpectedlyRepeated));
        }
        if wire_type == WireType::LengthDelimited {
            // We've encountered the expected length-delimited type: decode it in packed format.
            Self::decode_value(value, buf, ctx)
        } else {
            // Otherwise, try decoding it in the unpacked representation
            unpacked::decode::<C, E>(wire_type, value, buf, ctx)
        }
    }
}

impl<C, T, E> DistinguishedDecoder<Packed<E>> for C
where
    C: DistinguishedCollection<Item = T> + DistinguishedValueDecoder<Packed<E>>,
    T: ForOverwrite + Eq + ValueDecoder<E>,
{
    #[inline]
    fn decode_distinguished<B: Buf + ?Sized>(
        wire_type: WireType,
        duplicated: bool,
        value: &mut C,
        buf: Capped<B>,
        ctx: RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError> {
        if duplicated {
            return Err(DecodeError::new(UnexpectedlyRepeated));
        }
        if wire_type == WireType::LengthDelimited {
            // We've encountered the expected length-delimited type: decode it in packed format.
            // Set ALLOW_EMPTY to false: empty collections are not canonical
            let canon = DistinguishedValueDecoder::<Packed<E>>::decode_value_distinguished::<false>(
                value,
                buf,
                ctx.clone(),
            )?;
            ctx.check(if !C::CHECKS_EMPTY && value.is_empty() {
                Canonicity::NotCanonical
            } else {
                canon
            })
        } else {
            // Otherwise, try decoding it in the unpacked representation
            _ = ctx.check(Canonicity::NotCanonical)?;
            unpacked::decode::<C, E>(wire_type, value, buf, ctx.into_expedient())?;
            Ok(Canonicity::NotCanonical)
        }
    }
}

impl<T, const N: usize, E> ValueEncoder<Packed<E>> for [T; N]
where
    T: ValueEncoder<E>,
{
    #[inline]
    fn encode_value<B: BufMut + ?Sized>(value: &[T; N], buf: &mut B) {
        encode_varint(
            ValueEncoder::<E>::many_values_encoded_len(value.iter()) as u64,
            buf,
        );
        for val in value.iter() {
            ValueEncoder::<E>::encode_value(val, buf);
        }
    }

    #[inline]
    fn prepend_value<B: ReverseBuf + ?Sized>(value: &[T; N], buf: &mut B) {
        let end = buf.remaining();
        for val in value.iter().rev() {
            <T as ValueEncoder<E>>::prepend_value(val, buf);
        }
        prepend_varint((buf.remaining() - end) as u64, buf);
    }

    #[inline]
    fn value_encoded_len(value: &[T; N]) -> usize {
        let inner_len = ValueEncoder::<E>::many_values_encoded_len(value.iter());
        encoded_len_varint(inner_len as u64)
            .checked_add(inner_len)
            .unwrap()
    }
}

impl<T, const N: usize, E> ValueDecoder<Packed<E>> for [T; N]
where
    T: ValueDecoder<E>,
{
    #[inline]
    fn decode_value<B: Buf + ?Sized>(
        value: &mut [T; N],
        mut buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        let mut capped = buf.take_length_delimited()?;
        // MSRV: this could be .is_some_and(..)
        if matches!(
            <T as Wiretyped<E>>::WIRE_TYPE.fixed_size(),
            Some(fixed_size) if capped.remaining_before_cap() != fixed_size * N
        ) {
            // We know the exact size of a valid value and this isn't it.
            return Err(DecodeError::new(InvalidValue));
        }

        for dest in value {
            // If the value's size was already checked, we don't need to check again
            if <T as Wiretyped<E>>::WIRE_TYPE.fixed_size().is_none() && !capped.has_remaining()? {
                // Not enough values
                return Err(DecodeError::new(InvalidValue));
            }
            ValueDecoder::<E>::decode_value(dest, capped.lend(), ctx.clone())?;
        }

        // If the value's size was already checked, we don't need to check again
        if <T as Wiretyped<E>>::WIRE_TYPE.fixed_size().is_none() && capped.has_remaining()? {
            // Too many values or trailing data
            Err(DecodeError::new(InvalidValue))
        } else {
            Ok(())
        }
    }
}

impl<T, const N: usize, E> DistinguishedValueDecoder<Packed<E>> for [T; N]
where
    T: DistinguishedValueDecoder<E>,
{
    const CHECKS_EMPTY: bool = false;

    #[inline]
    fn decode_value_distinguished<const ALLOW_EMPTY: bool>(
        value: &mut [T; N],
        mut buf: Capped<impl Buf + ?Sized>,
        ctx: RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError> {
        let mut capped = buf.take_length_delimited()?;
        // MSRV: this could be .is_some_and(..)
        if matches!(
            <T as Wiretyped<E>>::WIRE_TYPE.fixed_size(),
            Some(fixed_size) if capped.remaining_before_cap() != fixed_size * N
        ) {
            // We know the exact size of a valid value and this isn't it.
            return Err(DecodeError::new(InvalidValue));
        }

        let canon = &mut Canonicity::Canonical;
        for dest in value.iter_mut() {
            // If the value's size was already checked, we don't need to check again
            if <T as Wiretyped<E>>::WIRE_TYPE.fixed_size().is_none() && !capped.has_remaining()? {
                // Not enough values
                return Err(DecodeError::new(InvalidValue));
            }
            ctx.update(
                canon,
                // Empty values are allowed because they are nested
                DistinguishedValueDecoder::<E>::decode_value_distinguished::<true>(
                    dest,
                    capped.lend(),
                    ctx.clone(),
                )?,
            )?;
        }

        // If the value's size was already checked, we don't need to check again
        if <T as Wiretyped<E>>::WIRE_TYPE.fixed_size().is_none() && capped.has_remaining()? {
            // Too many values or trailing data
            Err(DecodeError::new(InvalidValue))
        } else {
            Ok(*canon)
        }
    }
}

impl<T, const N: usize, E> Encoder<Packed<E>> for [T; N]
where
    T: EmptyState + ValueEncoder<E>,
{
    #[inline]
    fn encode<B: BufMut + ?Sized>(tag: u32, value: &[T; N], buf: &mut B, tw: &mut TagWriter) {
        if !EmptyState::is_empty(value) {
            <[T; N]>::encode_field(tag, value, buf, tw);
        }
    }

    #[inline]
    fn prepend_encode<B: ReverseBuf + ?Sized>(
        tag: u32,
        value: &[T; N],
        buf: &mut B,
        tw: &mut TagRevWriter,
    ) {
        if !EmptyState::is_empty(value) {
            <[T; N]>::prepend_field(tag, value, buf, tw);
        }
    }

    #[inline]
    fn encoded_len(tag: u32, value: &[T; N], tm: &mut impl TagMeasurer) -> usize {
        if !EmptyState::is_empty(value) {
            <[T; N]>::field_encoded_len(tag, value, tm)
        } else {
            0
        }
    }
}

impl<T, const N: usize, E> Decoder<Packed<E>> for [T; N]
where
    T: EmptyState + ValueDecoder<E>,
{
    #[inline]
    fn decode<B: Buf + ?Sized>(
        wire_type: WireType,
        duplicated: bool,
        value: &mut [T; N],
        buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        if duplicated {
            return Err(DecodeError::new(UnexpectedlyRepeated));
        }
        if wire_type == WireType::LengthDelimited {
            // We've encountered the expected length-delimited type: decode it in packed format.
            Self::decode_value(value, buf, ctx)
        } else {
            // Otherwise, try decoding it in the unpacked representation
            unpacked::decode_array_unpacked_only::<T, N, E>(wire_type, value, buf, ctx)
        }
    }
}

impl<T, const N: usize, E> DistinguishedDecoder<Packed<E>> for [T; N]
where
    T: Eq + EmptyState + DistinguishedValueDecoder<E> + ValueDecoder<E>,
{
    #[inline]
    fn decode_distinguished<B: Buf + ?Sized>(
        wire_type: WireType,
        duplicated: bool,
        value: &mut [T; N],
        buf: Capped<B>,
        ctx: RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError> {
        if duplicated {
            return Err(DecodeError::new(UnexpectedlyRepeated));
        }
        if wire_type == WireType::LengthDelimited {
            // We've encountered the expected length-delimited type: decode it in packed format.
            // Set ALLOW_EMPTY to false: empty collections are not canonical
            let canon = DistinguishedValueDecoder::<Packed<E>>::decode_value_distinguished::<false>(
                value,
                buf,
                ctx.clone(),
            )?;
            ctx.check(
                if
                /* !<[T; N]>::CHECKS_EMPTY && /* it never checks */ */
                value.is_empty() {
                    Canonicity::NotCanonical
                } else {
                    canon
                },
            )
        } else {
            // Otherwise, try decoding it in the unpacked representation
            _ = ctx.check(Canonicity::NotCanonical)?;
            unpacked::decode_array_unpacked_only::<T, N, E>(
                wire_type,
                value,
                buf,
                ctx.into_expedient(),
            )?;
            Ok(Canonicity::NotCanonical)
        }
    }
}
