use crate::buf::ReverseBuf;
use crate::encoding::schema::{FieldRepr, PopulateSchema, Schema, ValueRepr};
use crate::encoding::value_traits::{
    Collection, DistinguishedCollection, EmptyState, ForOverwrite,
};
use crate::encoding::{
    decoding_modes, encode_varint, encoded_len_varint, encoding_uses_base_empty_state,
    prepend_varint, unpacked, BorrowDecoder, Canonicity, Capped, DecodeContext, DecodeError,
    Decoder, DistinguishedBorrowDecoder, DistinguishedDecoder, DistinguishedValueBorrowDecoder,
    DistinguishedValueDecoder, Encoder, FieldEncoder, GeneralPacked, RestrictedDecodeContext,
    TagMeasurer, TagRevWriter, TagWriter, ValueBorrowDecoder, ValueDecoder, ValueEncoder, WireType,
    Wiretyped,
};
use crate::DecodeErrorKind::{InvalidValue, Truncated};
use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use bytes::{Buf, BufMut};
use core::fmt::Display;
use core::mem;

pub struct Packed<E = GeneralPacked>(E);

encoding_uses_base_empty_state!(Packed<E>, with generics (E));

/// Packed encodings always prefer to encode length delimited.
impl<E, T: ?Sized> Wiretyped<Packed<E>, T> for () {
    const WIRE_TYPE: WireType = WireType::LengthDelimited;
}

impl<C, E> ValueRepr<Packed<E>, C> for ()
where
    C: Collection,
    (): EmptyState<(), C> + ValueEncoder<Packed<E>, C> + ValueRepr<E, C::Item>,
{
    fn repr(schema: &Schema) -> Box<dyn Display> {
        let bounds = match C::BOUNDS.end {
            None => String::new(),
            Some(max) => format!("; at most {max} items"),
        };
        let restrictions = match C::RESTRICTIONS {
            None => String::new(),
            Some(r) => format!("; items are {r}"),
        };
        schema.make_lazy_repr(move |schema| {
            format!(
                "{packed_repr}{bounds}{restrictions}",
                packed_repr = <() as ValueRepr<Packed<E>, [C::Item]>>::repr(schema),
            )
        })
    }
}

impl<C, T, E> ValueEncoder<Packed<E>, C> for ()
where
    C: Collection<Item = T>,
    (): EmptyState<(), C> + ForOverwrite<E, T> + ValueEncoder<E, T>,
{
    #[inline]
    fn encode_value<B: BufMut + ?Sized>(value: &C, buf: &mut B) {
        encode_varint(
            <() as ValueEncoder<E, _>>::many_values_encoded_len(value.iter()) as u64,
            buf,
        );
        for val in value.iter() {
            <() as ValueEncoder<E, _>>::encode_value(val, buf);
        }
    }

    #[inline]
    fn prepend_value<B: ReverseBuf + ?Sized>(value: &C, buf: &mut B) {
        let end = buf.remaining();
        for val in value.reversed() {
            <() as ValueEncoder<E, _>>::prepend_value(val, buf);
        }
        prepend_varint((buf.remaining() - end) as u64, buf);
    }

    #[inline]
    fn value_encoded_len(value: &C) -> usize {
        let inner_len = <() as ValueEncoder<E, _>>::many_values_encoded_len(value.iter());
        encoded_len_varint(inner_len as u64)
            .checked_add(inner_len)
            .unwrap()
    }
}

impl<T, E> FieldRepr<Packed<E>, T> for ()
where
    (): Encoder<Packed<E>, T> + ValueRepr<Packed<E>, T>,
{
    fn repr(schema: &Schema) -> Box<dyn Display> {
        <() as ValueRepr<Packed<E>, T>>::repr(schema)
    }
}

/// ValueEncoder for packed repeated encodings lets this value type nest.
impl<C, T, E> Encoder<Packed<E>, C> for ()
where
    C: Collection<Item = T>,
    (): EmptyState<(), C> + ForOverwrite<E, T> + ValueEncoder<E, T> + ValueEncoder<Packed<E>, C>,
{
    #[inline]
    fn encode<B: BufMut + ?Sized>(tag: u32, value: &C, buf: &mut B, tw: &mut TagWriter) {
        if !<() as EmptyState<(), _>>::is_empty(value) {
            Self::encode_field(tag, value, buf, tw);
        }
    }

    #[inline]
    fn prepend_encode<B: ReverseBuf + ?Sized>(
        tag: u32,
        value: &C,
        buf: &mut B,
        tw: &mut TagRevWriter,
    ) {
        if !<() as EmptyState<(), _>>::is_empty(value) {
            Self::prepend_field(tag, value, buf, tw);
        }
    }

    #[inline]
    fn encoded_len(tag: u32, value: &C, tm: &mut impl TagMeasurer) -> usize {
        if !<() as EmptyState<(), _>>::is_empty(value) {
            Self::field_encoded_len(tag, value, tm)
        } else {
            0
        }
    }
}

impl<T, const N: usize, E> ValueRepr<Packed<E>, [T; N]> for ()
where
    (): ValueRepr<E, T>,
{
    fn repr(schema: &Schema) -> Box<dyn Display> {
        if N == 0 {
            Box::new("delimited empty")
        } else {
            schema.make_lazy_repr(|schema| {
                format!(
                    "{packed_repr}; exactly {N} items",
                    packed_repr = <() as ValueRepr<Packed<E>, [T]>>::repr(schema),
                )
            })
        }
    }
}

impl<T, const N: usize, E> ValueEncoder<Packed<E>, [T; N]> for ()
where
    (): ValueEncoder<E, T>,
{
    #[inline]
    fn encode_value<B: BufMut + ?Sized>(value: &[T; N], buf: &mut B) {
        <() as ValueEncoder<Packed<E>, [T]>>::encode_value(value, buf)
    }

    #[inline]
    fn prepend_value<B: ReverseBuf + ?Sized>(value: &[T; N], buf: &mut B) {
        <() as ValueEncoder<Packed<E>, [T]>>::prepend_value(value, buf)
    }

    #[inline]
    fn value_encoded_len(value: &[T; N]) -> usize {
        <() as ValueEncoder<Packed<E>, [T]>>::value_encoded_len(value)
    }
}

impl<T, const N: usize, E> Encoder<Packed<E>, [T; N]> for ()
where
    (): ValueEncoder<E, T> + EmptyState<E, [T; N]>,
{
    #[inline]
    fn encode<B: BufMut + ?Sized>(tag: u32, value: &[T; N], buf: &mut B, tw: &mut TagWriter) {
        if !<() as EmptyState<E, _>>::is_empty(value) {
            <() as FieldEncoder<Packed<E>, [T]>>::encode_field(tag, value, buf, tw);
        }
    }

    #[inline]
    fn prepend_encode<B: ReverseBuf + ?Sized>(
        tag: u32,
        value: &[T; N],
        buf: &mut B,
        tw: &mut TagRevWriter,
    ) {
        if !<() as EmptyState<E, _>>::is_empty(value) {
            <() as FieldEncoder<Packed<E>, [T]>>::prepend_field(tag, value, buf, tw);
        }
    }

    #[inline]
    fn encoded_len(tag: u32, value: &[T; N], tm: &mut impl TagMeasurer) -> usize {
        if !<() as EmptyState<E, _>>::is_empty(value) {
            <() as FieldEncoder<Packed<E>, [T]>>::field_encoded_len(tag, value, tm)
        } else {
            0
        }
    }
}

impl<T, E> ValueRepr<Packed<E>, [T]> for ()
where
    (): ValueRepr<E, T>,
{
    fn repr(schema: &Schema) -> Box<dyn Display> {
        schema.make_lazy_repr(|schema| {
            format!(
                "delimited packed (items: {item_repr})",
                item_repr = <() as ValueRepr<E, T>>::repr(schema),
            )
        })
    }
}

/// Packed encodes slices the same way as arrays. This implementation always uses the natural
/// emptiness and item iteration of the slice, since implementing `EmptyState` for the unsized `[T]`
/// isn't practical.
impl<T, E> ValueEncoder<Packed<E>, [T]> for ()
where
    (): ValueEncoder<E, T>,
{
    #[inline]
    fn encode_value<B: BufMut + ?Sized>(value: &[T], buf: &mut B) {
        encode_varint(
            <() as ValueEncoder<E, _>>::many_values_encoded_len(value.iter()) as u64,
            buf,
        );
        for val in value.iter() {
            <() as ValueEncoder<E, _>>::encode_value(val, buf);
        }
    }

    #[inline]
    fn prepend_value<B: ReverseBuf + ?Sized>(value: &[T], buf: &mut B) {
        let end = buf.remaining();
        for val in value.iter().rev() {
            <() as ValueEncoder<E, _>>::prepend_value(val, buf);
        }
        prepend_varint((buf.remaining() - end) as u64, buf);
    }

    #[inline]
    fn value_encoded_len(value: &[T]) -> usize {
        let inner_len = <() as ValueEncoder<E, _>>::many_values_encoded_len(value.iter());
        encoded_len_varint(inner_len as u64)
            .checked_add(inner_len)
            .unwrap()
    }
}

impl<T, E> Encoder<Packed<E>, [T]> for ()
where
    (): ValueEncoder<E, T>,
{
    #[inline]
    fn encode<B: BufMut + ?Sized>(tag: u32, value: &[T], buf: &mut B, tw: &mut TagWriter) {
        if !value.is_empty() {
            <() as FieldEncoder<Packed<E>, [T]>>::encode_field(tag, value, buf, tw);
        }
    }

    #[inline]
    fn prepend_encode<B: ReverseBuf + ?Sized>(
        tag: u32,
        value: &[T],
        buf: &mut B,
        tw: &mut TagRevWriter,
    ) {
        if !value.is_empty() {
            <() as FieldEncoder<Packed<E>, [T]>>::prepend_field(tag, value, buf, tw);
        }
    }

    #[inline]
    fn encoded_len(tag: u32, value: &[T], tm: &mut impl TagMeasurer) -> usize {
        if !value.is_empty() {
            <() as FieldEncoder<Packed<E>, [T]>>::field_encoded_len(tag, value, tm)
        } else {
            0
        }
    }
}

macro_rules! impl_decoders {
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
    ) => {
        impl<$($lifetime,)? C, T, E> $relaxed_value <$($lifetime,)? Packed<E>, C> for ()
        where
            C: Collection<Item = T>,
            (): EmptyState<(), C>
                + ForOverwrite<E, T>
                + $relaxed_value<$($lifetime,)? E, T>,
        {
            #[inline]
            fn $relaxed_value_method $($($buf_generic)*)? (
                value: &mut C,
                mut buf: Capped<$buf_ty>,
                ctx: impl DecodeContext,
            ) -> Result<(), DecodeError> {
                let mut capped = buf.take_length_delimited()?;
                // MSRV: this could be .is_some_and(..) (1.70)
                if matches!(
                    <() as Wiretyped<E, T>>::WIRE_TYPE.fixed_size(),
                    Some(fixed_size) if capped.remaining_before_cap() % fixed_size != 0
                ) {
                    // No number of fixed-sized values can pack evenly into this size.
                    return Err(DecodeError::new(Truncated));
                }
                while capped.has_remaining()? {
                    ctx.heap_used(mem::size_of::<T>() + <() as ForOverwrite<E, T>>::INIT_HEAP)?;
                    let mut new_val = <() as ForOverwrite<E, T>>::for_overwrite();
                    <() as $relaxed_value<E, _>>::$relaxed_value_method(
                        &mut new_val,
                        capped.lend(),
                        ctx.clone(),
                    )?;
                    value.insert(new_val)?;
                }
                Ok(())
            }
        }

        impl<$($lifetime,)? C, T, E> $distinguished_value<$($lifetime,)? Packed<E>, C> for ()
        where
            C: DistinguishedCollection<Item = T> + Eq,
            T: Eq,
            (): EmptyState<(), C>
                + ForOverwrite<E, T>
                + $distinguished_value<$($lifetime,)? E, T>,
        {
            const CHECKS_EMPTY: bool = false;

            #[inline]
            fn $distinguished_value_method <const ALLOW_EMPTY: bool>(
                value: &mut C,
                mut buf: Capped<$impl_buf_ty>,
                ctx: impl RestrictedDecodeContext,
            ) -> Result<Canonicity, DecodeError> {
                let mut capped = buf.take_length_delimited()?;
                // MSRV: this could be .is_some_and(..) (1.70)
                if matches!(
                    <() as Wiretyped<E, T>>::WIRE_TYPE.fixed_size(),
                    Some(fixed_size) if capped.remaining_before_cap() % fixed_size != 0
                ) {
                    // No number of fixed-sized values can pack evenly into this size.
                    return Err(DecodeError::new(Truncated));
                }
                let mut canon = Canonicity::Canonical;
                while capped.has_remaining()? {
                    ctx.heap_used(mem::size_of::<T>() + <() as ForOverwrite<E, T>>::INIT_HEAP)?;
                    let mut new_val = <() as ForOverwrite<E, T>>::for_overwrite();
                    canon.update(
                        <() as $distinguished_value<E, _>>::$distinguished_value_method::<true>(
                            &mut new_val,
                            capped.lend(),
                            ctx.clone(),
                        )?,
                    );
                    canon.update(ctx.check(value.insert_distinguished(new_val)?)?);
                }
                Ok(canon)
            }
        }

        impl<$($lifetime,)? C, T, E> $relaxed <$($lifetime,)? Packed<E>, C> for ()
        where
            C: Collection<Item = T>,
            (): EmptyState<(), C>
                + ForOverwrite<E, T>
                + $relaxed_value <$($lifetime,)? E, T>
                + $relaxed_value <$($lifetime,)? Packed<E>, C>,
        {
            #[inline]
            fn $relaxed_method $($($buf_generic)*)? (
                wire_type: WireType,
                value: &mut C,
                buf: Capped<$buf_ty>,
                ctx: impl DecodeContext,
            ) -> Result<(), DecodeError> {
                if wire_type == WireType::LengthDelimited {
                    // We've encountered the expected length-delimited type: decode it in packed
                    // format.
                    <() as $relaxed_value<Packed<E>, C>>::$relaxed_value_method(value, buf, ctx)
                } else {
                    // Otherwise, try decoding it in the unpacked representation
                    unpacked::$mode::decode::<C, E>(wire_type, value, buf, ctx)
                }
            }
        }

        impl<$($lifetime,)? C, T, E> $distinguished <$($lifetime,)? Packed<E>, C> for ()
        where
            C: DistinguishedCollection<Item = T>,
            T: Eq,
            (): EmptyState<(), C>
                + ForOverwrite<E, T>
                + $relaxed_value <$($lifetime,)? E, T>
                + $distinguished_value <$($lifetime,)? Packed<E>, C>,
        {
            #[inline]
            fn $distinguished_method $($($buf_generic)*)? (
                wire_type: WireType,
                value: &mut C,
                buf: Capped<$buf_ty>,
                ctx: impl RestrictedDecodeContext,
            ) -> Result<Canonicity, DecodeError> {
                if wire_type == WireType::LengthDelimited {
                    // We've encountered the expected length-delimited type: decode it in packed
                    // format. Set ALLOW_EMPTY to false: empty collections are not canonical
                    let canon = <() as $distinguished_value<Packed<E>, _>>::
                        $distinguished_value_method::<false>
                    (
                        value,
                        buf,
                        ctx.clone(),
                    )?;
                    if !<() as $distinguished_value<Packed<E>, C>>::CHECKS_EMPTY
                        && <() as EmptyState<(), C>>::is_empty(value)
                    {
                        ctx.check(Canonicity::NotCanonical)
                    } else {
                        Ok(canon)
                    }
                } else {
                    // Otherwise, try decoding it in the unpacked representation
                    _ = ctx.check(Canonicity::NotCanonical)?;
                    unpacked::$mode::decode::<C, E>(
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
        $relaxed_value <$($lifetime,)? Packed<E>, [T; N]> for ()
        where
            (): $relaxed_value <$($lifetime,)? E, T>,
        {
            #[inline]
            fn $relaxed_value_method $($($buf_generic)*)? (
                value: &mut [T; N],
                mut buf: Capped<$buf_ty>,
                ctx: impl DecodeContext,
            ) -> Result<(), DecodeError> {
                let mut capped = buf.take_length_delimited()?;
                // MSRV: this could be .is_some_and(..) (1.70)
                if matches!(
                    <() as Wiretyped<E, T>>::WIRE_TYPE.fixed_size(),
                    Some(fixed_size) if capped.remaining_before_cap() != fixed_size * N
                ) {
                    // We know the exact size of a valid value and this isn't it.
                    return Err(DecodeError::new(InvalidValue));
                }

                for dest in value {
                    // If the value's size was already checked, we don't need to check again
                    if <() as Wiretyped<E, T>>::WIRE_TYPE.fixed_size().is_none()
                        && !capped.has_remaining()?
                    {
                        // Not enough values
                        return Err(DecodeError::new(InvalidValue));
                    }
                    <() as $relaxed_value<E, _>>::$relaxed_value_method(
                        dest, capped.lend(), ctx.clone())?;
                }

                // If the value's size was already checked, we don't need to check again
                if <() as Wiretyped<E, T>>::WIRE_TYPE.fixed_size().is_none()
                    && capped.has_remaining()?
                {
                    // Too many values or trailing data
                    Err(DecodeError::new(InvalidValue))
                } else {
                    Ok(())
                }
            }
        }

        impl<$($lifetime,)? T, const N: usize, E>
        $distinguished_value <$($lifetime,)? Packed<E>, [T; N]> for ()
        where
            (): $distinguished_value <$($lifetime,)? E, T>,
        {
            const CHECKS_EMPTY: bool = false;

            #[inline]
            fn $distinguished_value_method <const ALLOW_EMPTY: bool>(
                value: &mut [T; N],
                mut buf: Capped<$impl_buf_ty>,
                ctx: impl RestrictedDecodeContext,
            ) -> Result<Canonicity, DecodeError> {
                let mut capped = buf.take_length_delimited()?;
                // MSRV: this could be .is_some_and(..) (1.70)
                if matches!(
                    <() as Wiretyped<E, T>>::WIRE_TYPE.fixed_size(),
                    Some(fixed_size) if capped.remaining_before_cap() != fixed_size * N
                ) {
                    // We know the exact size of a valid value and this isn't it.
                    return Err(DecodeError::new(InvalidValue));
                }

                let mut canon = Canonicity::Canonical;
                for dest in value.iter_mut() {
                    // If the value's size was already checked, we don't need to check again
                    if <() as Wiretyped<E, T>>::WIRE_TYPE.fixed_size().is_none()
                        && !capped.has_remaining()?
                    {
                        // Not enough values
                        return Err(DecodeError::new(InvalidValue));
                    }
                    canon.update(
                        // Empty values are allowed because they are nested
                        <() as $distinguished_value<E, _>>::$distinguished_value_method::<true>(
                            dest,
                            capped.lend(),
                            ctx.clone(),
                        )?,
                    );
                }

                // If the value's size was already checked, we don't need to check again
                if <() as Wiretyped<E, T>>::WIRE_TYPE.fixed_size().is_none()
                    && capped.has_remaining()?
                {
                    // Too many values or trailing data
                    Err(DecodeError::new(InvalidValue))
                } else {
                    Ok(canon)
                }
            }
        }

        impl<$($lifetime,)? T, const N: usize, E>
        $relaxed <$($lifetime,)? Packed<E>, [T; N]> for ()
        where
            (): $relaxed_value <$($lifetime,)? E, T> + EmptyState<E, [T; N]>,
        {
            #[inline]
            fn $relaxed_method $($($buf_generic)*)? (
                wire_type: WireType,
                value: &mut [T; N],
                buf: Capped<$buf_ty>,
                ctx: impl DecodeContext,
            ) -> Result<(), DecodeError> {
                if wire_type == WireType::LengthDelimited {
                    // We've encountered the expected length-delimited type: decode it in packed
                    // format.
                    <() as $relaxed_value<Packed<E>, [T; N]>>::
                        $relaxed_value_method(value, buf, ctx)
                } else {
                    // Otherwise, try decoding it in the unpacked representation
                    unpacked::$mode::decode_array_unpacked_only::<T, N, E>(
                        wire_type,
                        value,
                        buf,
                        ctx,
                    )
                }
            }
        }

        impl<$($lifetime,)? T, const N: usize, E>
        $distinguished <$($lifetime,)? Packed<E>, [T; N]> for ()
        where
            T: Eq,
            (): $distinguished_value <$($lifetime,)? E, T>
                + $relaxed_value <$($lifetime,)? E, T>
                + EmptyState<E, [T; N]>,
        {
            #[inline]
            fn $distinguished_method $($($buf_generic)*)? (
                wire_type: WireType,
                value: &mut [T; N],
                buf: Capped<$buf_ty>,
                ctx: impl RestrictedDecodeContext,
            ) -> Result<Canonicity, DecodeError> {
                if wire_type == WireType::LengthDelimited {
                    // We've encountered the expected length-delimited type: decode it in packed
                    // format.
                    // Set ALLOW_EMPTY to false: empty collections are not canonical
                    let canon = <() as $distinguished_value<Packed<E>, _>>::
                        $distinguished_value_method::<false>
                    (
                        value,
                        buf,
                        ctx.clone(),
                    )?;

                    if
                    /* !<[T; N]>::CHECKS_EMPTY && /* it never checks */ */
                    <() as EmptyState<E, [T; N]>>::is_empty(value) {
                        ctx.check(Canonicity::NotCanonical)
                    } else {
                        Ok(canon)
                    }
                } else {
                    // Otherwise, try decoding it in the unpacked representation
                    _ = ctx.check(Canonicity::NotCanonical)?;
                    unpacked::$mode::decode_array_unpacked_only::<T, N, E>(
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

decoding_modes::__invoke!(impl_decoders, owned);
decoding_modes::__invoke!(impl_decoders, borrowed);
