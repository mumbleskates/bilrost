use bytes::{Buf, BufMut};

use crate::buf::ReverseBuf;
use crate::encoding::value_traits::{
    Collection, DistinguishedCollection, EmptyState, ForOverwrite,
};
use crate::encoding::{
    encode_varint, encoded_len_varint, prepend_varint, unpacked, BorrowDecoder, Canonicity, Capped,
    DecodeContext, DecodeError, Decoder, DistinguishedBorrowDecoder, DistinguishedDecoder,
    DistinguishedValueBorrowDecoder, DistinguishedValueDecoder, Encoder, FieldEncoder, General,
    RestrictedDecodeContext, TagMeasurer, TagRevWriter, TagWriter, ValueBorrowDecoder,
    ValueDecoder, ValueEncoder, WireType, Wiretyped,
};
use crate::DecodeErrorKind::{InvalidValue, Truncated, UnexpectedlyRepeated};

pub struct Packed<E = General>(E);

/// Packed encodings always prefer to encode length delimited.
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

macro_rules! impl_decoders {
    (
        relaxed: $relaxed_trait:ident::$relaxed_method:ident,
        relaxed_value: $relaxed_value_trait:ident::$relaxed_value_method:ident,
        distinguished: $distinguished_trait:ident::$distinguished_method:ident,
        distinguished_value: $distinguished_value_trait:ident::$distinguished_value_method:ident,
        unpacked_mode: $unpacked_mode:ident,
        $(buf_bound: $buf_ty:ident => ($($buf_bound:tt)*),)?
        $(lifetime: $lifetime:lifetime,)?
    ) => {
        impl<$($lifetime,)? C, T, E> $relaxed_value_trait<$($lifetime,)? Packed<E>> for C
        where
            C: Collection<Item = T>,
            T: ForOverwrite + $relaxed_value_trait<$($lifetime,)? E>,
        {
            #[inline]
            fn $relaxed_value_method $(<$buf_ty: $($buf_bound)*>)? (
                value: &mut C,
                mut buf: Capped<$($buf_ty)? $(&$lifetime [u8])?>,
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
                    $relaxed_value_trait::<E>::$relaxed_value_method(&mut new_val, capped.lend(), ctx.clone())?;
                    value.insert(new_val)?;
                }
                Ok(())
            }
        }

        impl<$($lifetime,)? C, T, E> $distinguished_value_trait<$($lifetime,)? Packed<E>> for C
        where
            C: DistinguishedCollection<Item = T> + Eq,
            T: ForOverwrite + Eq + $distinguished_value_trait<$($lifetime,)? E>,
        {
            const CHECKS_EMPTY: bool = false;

            #[inline]
            fn $distinguished_value_method <const ALLOW_EMPTY: bool>(
                value: &mut C,
                mut buf: Capped<$(impl $($buf_bound)*)? $(&$lifetime [u8])?>,
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
                let mut canon = Canonicity::Canonical;
                while capped.has_remaining()? {
                    let mut new_val = T::for_overwrite();
                    canon.update(
                        $distinguished_value_trait::<E>::$distinguished_value_method::<true>(
                            &mut new_val,
                            capped.lend(),
                            ctx.clone(),
                        )?,
                    );
                    ctx.update(&mut canon, value.insert_distinguished(new_val)?)?;
                }
                Ok(canon)
            }
        }

        impl<$($lifetime,)? C, T, E> $relaxed_trait <$($lifetime,)? Packed<E>> for C
        where
            C: Collection<Item = T> + $relaxed_value_trait <$($lifetime,)? Packed<E>>,
            T: ForOverwrite + $relaxed_value_trait <$($lifetime,)? E>,
        {
            #[inline]
            fn $relaxed_method $(<$buf_ty: $($buf_bound)*>)? (
                wire_type: WireType,
                duplicated: bool,
                value: &mut C,
                buf: Capped<$($buf_ty)? $(&$lifetime [u8])?>,
                ctx: DecodeContext,
            ) -> Result<(), DecodeError> {
                if duplicated {
                    return Err(DecodeError::new(UnexpectedlyRepeated));
                }
                if wire_type == WireType::LengthDelimited {
                    // We've encountered the expected length-delimited type: decode it in packed format.
                    Self::$relaxed_value_method(value, buf, ctx)
                } else {
                    // Otherwise, try decoding it in the unpacked representation
                    unpacked::$unpacked_mode::decode::<C, E>(wire_type, value, buf, ctx)
                }
            }
        }

        impl<$($lifetime,)? C, T, E> $distinguished_trait <$($lifetime,)? Packed<E>> for C
        where
            C: DistinguishedCollection<Item = T>
                + $distinguished_value_trait <$($lifetime,)? Packed<E>>,
            T: ForOverwrite + Eq + $relaxed_value_trait <$($lifetime,)? E>,
        {
            #[inline]
            fn $distinguished_method $(<$buf_ty: $($buf_bound)*>)? (
                wire_type: WireType,
                duplicated: bool,
                value: &mut C,
                buf: Capped<$($buf_ty)? $(&$lifetime [u8])?>,
                ctx: RestrictedDecodeContext,
            ) -> Result<Canonicity, DecodeError> {
                if duplicated {
                    return Err(DecodeError::new(UnexpectedlyRepeated));
                }
                if wire_type == WireType::LengthDelimited {
                    // We've encountered the expected length-delimited type: decode it in packed
                    // format. Set ALLOW_EMPTY to false: empty collections are not canonical
                    let canon = $distinguished_value_trait::<Packed<E>>::
                        $distinguished_value_method::<false>
                    (
                        value,
                        buf,
                        ctx.clone(),
                    )?;
                    if !C::CHECKS_EMPTY && value.is_empty() {
                        ctx.check(Canonicity::NotCanonical)
                    } else {
                        Ok(canon)
                    }
                } else {
                    // Otherwise, try decoding it in the unpacked representation
                    _ = ctx.check(Canonicity::NotCanonical)?;
                    unpacked::$unpacked_mode::decode::<C, E>(
                        wire_type,
                        value,
                        buf,
                        ctx.into_inner(),
                    )?;
                    Ok(Canonicity::NotCanonical)
                }
            }
        }

        impl<$($lifetime,)? T, const N: usize, E>
        $relaxed_value_trait <$($lifetime,)? Packed<E>> for [T; N]
        where
            T: $relaxed_value_trait <$($lifetime,)? E>,
        {
            #[inline]
            fn $relaxed_value_method $(<$buf_ty: $($buf_bound)*>)? (
                value: &mut [T; N],
                mut buf: Capped<$($buf_ty)? $(&$lifetime [u8])?>,
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
                    $relaxed_value_trait::<E>::$relaxed_value_method(dest, capped.lend(), ctx.clone())?;
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

        impl<$($lifetime,)? T, const N: usize, E>
        $distinguished_value_trait <$($lifetime,)? Packed<E>> for [T; N]
        where
            T: $distinguished_value_trait <$($lifetime,)? E>,
        {
            const CHECKS_EMPTY: bool = false;

            #[inline]
            fn $distinguished_value_method <const ALLOW_EMPTY: bool>(
                value: &mut [T; N],
                mut buf: Capped<$(impl $($buf_bound)*)? $(&$lifetime [u8])?>,
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

                let mut canon = Canonicity::Canonical;
                for dest in value.iter_mut() {
                    // If the value's size was already checked, we don't need to check again
                    if <T as Wiretyped<E>>::WIRE_TYPE.fixed_size().is_none() && !capped.has_remaining()? {
                        // Not enough values
                        return Err(DecodeError::new(InvalidValue));
                    }
                    canon.update(
                        // Empty values are allowed because they are nested
                        $distinguished_value_trait::<E>::$distinguished_value_method::<true>(
                            dest,
                            capped.lend(),
                            ctx.clone(),
                        )?,
                    );
                }

                // If the value's size was already checked, we don't need to check again
                if <T as Wiretyped<E>>::WIRE_TYPE.fixed_size().is_none() && capped.has_remaining()? {
                    // Too many values or trailing data
                    Err(DecodeError::new(InvalidValue))
                } else {
                    Ok(canon)
                }
            }
        }

        impl<$($lifetime,)? T, const N: usize, E>
        $relaxed_trait <$($lifetime,)? Packed<E>> for [T; N]
        where
            T: EmptyState + $relaxed_value_trait <$($lifetime,)? E>,
        {
            #[inline]
            fn $relaxed_method $(<$buf_ty: $($buf_bound)*>)? (
                wire_type: WireType,
                duplicated: bool,
                value: &mut [T; N],
                buf: Capped<$($buf_ty)? $(&$lifetime [u8])?>,
                ctx: DecodeContext,
            ) -> Result<(), DecodeError> {
                if duplicated {
                    return Err(DecodeError::new(UnexpectedlyRepeated));
                }
                if wire_type == WireType::LengthDelimited {
                    // We've encountered the expected length-delimited type: decode it in packed format.
                    Self::$relaxed_value_method(value, buf, ctx)
                } else {
                    // Otherwise, try decoding it in the unpacked representation
                    unpacked::$unpacked_mode::decode_array_unpacked_only::<T, N, E>(
                        wire_type,
                        value,
                        buf,
                        ctx,
                    )
                }
            }
        }

        impl<$($lifetime,)? T, const N: usize, E>
        $distinguished_trait <$($lifetime,)? Packed<E>> for [T; N]
        where
            T: Eq
                + EmptyState
                + $distinguished_value_trait <$($lifetime,)? E>
                + $relaxed_value_trait <$($lifetime,)? E>,
        {
            #[inline]
            fn $distinguished_method $(<$buf_ty: $($buf_bound)*>)? (
                wire_type: WireType,
                duplicated: bool,
                value: &mut [T; N],
                buf: Capped<$($buf_ty)? $(&$lifetime [u8])?>,
                ctx: RestrictedDecodeContext,
            ) -> Result<Canonicity, DecodeError> {
                if duplicated {
                    return Err(DecodeError::new(UnexpectedlyRepeated));
                }
                if wire_type == WireType::LengthDelimited {
                    // We've encountered the expected length-delimited type: decode it in packed format.
                    // Set ALLOW_EMPTY to false: empty collections are not canonical
                    let canon = $distinguished_value_trait::<Packed<E>>::
                        $distinguished_value_method::<false>
                    (
                        value,
                        buf,
                        ctx.clone(),
                    )?;

                    if
                    /* !<[T; N]>::CHECKS_EMPTY && /* it never checks */ */
                    value.is_empty() {
                        ctx.check(Canonicity::NotCanonical)
                    } else {
                        Ok(canon)
                    }
                } else {
                    // Otherwise, try decoding it in the unpacked representation
                    _ = ctx.check(Canonicity::NotCanonical)?;
                    unpacked::$unpacked_mode::decode_array_unpacked_only::<T, N, E>(
                        wire_type,
                        value,
                        buf,
                        ctx.into_inner(),
                    )?;
                    Ok(Canonicity::NotCanonical)
                }
            }
        }
    };
}

impl_decoders!(
    relaxed: Decoder::decode,
    relaxed_value: ValueDecoder::decode_value,
    distinguished: DistinguishedDecoder::decode_distinguished,
    distinguished_value: DistinguishedValueDecoder::decode_value_distinguished,
    unpacked_mode: owned,
    buf_bound: B => (Buf + ?Sized),
);

impl_decoders!(
    relaxed: BorrowDecoder::borrow_decode,
    relaxed_value: ValueBorrowDecoder::borrow_decode_value,
    distinguished: DistinguishedBorrowDecoder::borrow_decode_distinguished,
    distinguished_value: DistinguishedValueBorrowDecoder::borrow_decode_value_distinguished,
    unpacked_mode: borrowed,
    lifetime: 'a,
);
