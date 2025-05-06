use crate::buf::ReverseBuf;
use crate::encoding::value_traits::{
    Collection, DistinguishedCollection, EmptyState, ForOverwrite,
};
use crate::encoding::{
    check_wire_type, decoding_modes, peek_repeated_field, BorrowDecoder, Capped, DecodeContext,
    Decoder, DistinguishedBorrowDecoder, DistinguishedDecoder, DistinguishedValueBorrowDecoder,
    DistinguishedValueDecoder, Encoder, FieldEncoder, GeneralPacked, Packed,
    RestrictedDecodeContext, TagMeasurer, TagRevWriter, TagWriter, ValueBorrowDecoder,
    ValueDecoder, ValueEncoder, WireType, Wiretyped,
};
use crate::DecodeErrorKind::InvalidValue;
use crate::{Canonicity, DecodeError};
use bytes::BufMut;

pub struct Unpacked<E = GeneralPacked>(E);

macro_rules! define_decoders {
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
        /// Decodes a collection value from the unpacked representation. This greedily consumes
        /// consecutive fields as long as they have the same tag.
        #[inline]
        pub(crate) fn decode<$($lifetime,)? T, E>(
            wire_type: WireType,
            collection: &mut T,
            mut buf: Capped<$impl_buf_ty>,
            ctx: DecodeContext,
        ) -> Result<(), DecodeError>
        where
            T: Collection,
            T::Item: ForOverwrite + $relaxed_value <$($lifetime,)? E>,
        {
            check_wire_type(<T::Item as Wiretyped<E>>::WIRE_TYPE, wire_type)?;
            loop {
                // Decode one item
                let mut new_item = T::Item::for_overwrite();
                $relaxed_value::<E>::$relaxed_value_method(&mut new_item, buf.lend(), ctx.clone())?;
                collection.insert(new_item)?;

                if let Some(next_wire_type) = peek_repeated_field(&mut buf) {
                    check_wire_type(<T::Item as Wiretyped<E>>::WIRE_TYPE, next_wire_type)?;
                } else {
                    break;
                }
            }
            Ok(())
        }

        /// Decodes an array value from either unpacked or packed representation. If there are not
        /// exactly the expected number of fields the value is considered to be invalid.
        #[inline]
        pub(super) fn decode_array_either_repr<$($lifetime,)? T, const N: usize, E>(
            wire_type: WireType,
            arr: &mut [T; N],
            buf: Capped<$impl_buf_ty>,
            ctx: DecodeContext,
        ) -> Result<(), DecodeError>
        where
            T: $relaxed_value <$($lifetime,)? E>,
        {
            if wire_type == WireType::LengthDelimited
                && <T as Wiretyped<E>>::WIRE_TYPE != WireType::LengthDelimited
            {
                // We've encountered a length-delimited field when we aren't expecting one; try
                // decoding it in packed format instead.
                $relaxed_value::<Packed<E>>::$relaxed_value_method(arr, buf, ctx)
            } else {
                // Otherwise, decode in unpacked mode.
                decode_array_unpacked_only(wire_type, arr, buf, ctx)
            }
        }

        /// Decodes an array value in only the unpacked representation. If there are not exactly the
        /// expected number of fields the value is considered to be invalid.
        #[inline]
        pub(crate) fn decode_array_unpacked_only<$($lifetime,)? T, const N: usize, E>(
            wire_type: WireType,
            arr: &mut [T; N],
            mut buf: Capped<$impl_buf_ty>,
            ctx: DecodeContext,
        ) -> Result<(), DecodeError>
        where
            T: $relaxed_value <$($lifetime,)? E>,
        {
            check_wire_type(<T as Wiretyped<E>>::WIRE_TYPE, wire_type)?;
            for (i, dest) in arr.iter_mut().enumerate() {
                // The initial field key is consumed, but we must read the repeated field key for
                // each one after that.
                if i > 0 {
                    if let Some(next_wire_type) = peek_repeated_field(&mut buf) {
                        check_wire_type(<T as Wiretyped<E>>::WIRE_TYPE, next_wire_type)?;
                    } else {
                        // Not enough value fields
                        return Err(DecodeError::new(InvalidValue));
                    }
                }
                // Decode one item
                $relaxed_value::<E>::$relaxed_value_method(dest, buf.lend(), ctx.clone())?;
            }
            if peek_repeated_field(&mut buf).is_some() {
                // Too many value fields
                Err(DecodeError::new(InvalidValue))
            } else {
                Ok(())
            }
        }

        /// Decodes a collection value from the unpacked representation in distinguished mode. This
        /// greedily consumes consecutive fields as long as they have the same tag.
        #[inline]
        pub(crate) fn decode_distinguished<$($lifetime,)? T, E>(
            wire_type: WireType,
            collection: &mut T,
            mut buf: Capped<$impl_buf_ty>,
            ctx: RestrictedDecodeContext,
        ) -> Result<Canonicity, DecodeError>
        where
            T: DistinguishedCollection,
            T::Item: ForOverwrite + Eq + $distinguished_value <$($lifetime,)? E>,
        {
            check_wire_type(<T::Item as Wiretyped<E>>::WIRE_TYPE, wire_type)?;
            let mut canon = Canonicity::Canonical;
            loop {
                // Decode one item
                let mut new_item = T::Item::for_overwrite();
                // Decoded field values are nested within the collection; empty values are OK
                canon.update(
                    $distinguished_value::<E>::$distinguished_value_method::<true>(
                        &mut new_item,
                        buf.lend(),
                        ctx.clone(),
                    )?,
                );
                canon.update(ctx.check(collection.insert_distinguished(new_item)?)?);

                if let Some(next_wire_type) = peek_repeated_field(&mut buf) {
                    check_wire_type(<T::Item as Wiretyped<E>>::WIRE_TYPE, next_wire_type)?;
                } else {
                    break;
                }
            }
            Ok(canon)
        }

        /// Decodes an array value from either packed or unpacked in distinguished mode. If there
        /// are not exactly the expected number of fields the value is considered to be invalid.
        #[inline]
        pub(super) fn decode_distinguished_array_either_repr<$($lifetime,)? T, const N: usize, E>(
            wire_type: WireType,
            arr: &mut [T; N],
            buf: Capped<$impl_buf_ty>,
            ctx: RestrictedDecodeContext,
        ) -> Result<Canonicity, DecodeError>
        where
            T: Eq
                + $relaxed_value <$($lifetime,)? E>
                + $distinguished_value <$($lifetime,)? E>,
        {
            if wire_type == WireType::LengthDelimited
                && <T as Wiretyped<E>>::WIRE_TYPE != WireType::LengthDelimited
            {
                // We've encountered a length-delimited field when we aren't expecting one; try
                // decoding it in packed format instead.
                // The data is already known to be non-canonical; use relaxed decoding
                _ = ctx.check(Canonicity::NotCanonical)?;
                $relaxed_value::<Packed<E>>::$relaxed_value_method(arr, buf, ctx.into_inner())?;
                Ok(Canonicity::NotCanonical)
            } else {
                // Otherwise, decode in unpacked mode.
                decode_distinguished_array_unpacked_only(wire_type, arr, buf, ctx)
            }
        }

        /// Decodes an array value from the unpacked representation in distinguished mode. If there
        /// are not exactly the expected number of fields the value is considered to be invalid.
        #[inline]
        fn decode_distinguished_array_unpacked_only<$($lifetime,)? T, const N: usize, E>(
            wire_type: WireType,
            arr: &mut [T; N],
            mut buf: Capped<$impl_buf_ty>,
            ctx: RestrictedDecodeContext,
        ) -> Result<Canonicity, DecodeError>
        where
            T: Eq + $distinguished_value <$($lifetime,)? E>,
        {
            check_wire_type(<T as Wiretyped<E>>::WIRE_TYPE, wire_type)?;
            let mut canon = Canonicity::Canonical;
            for (i, dest) in arr.iter_mut().enumerate() {
                // The initial field key is consumed, but we must read the repeated field key for
                // each one after that.
                if i > 0 {
                    if let Some(next_wire_type) = peek_repeated_field(&mut buf) {
                        check_wire_type(<T as Wiretyped<E>>::WIRE_TYPE, next_wire_type)?;
                    } else {
                        // Not enough value fields
                        return Err(DecodeError::new(InvalidValue));
                    }
                }
                // Decode one item. Empty values are allowed
                canon.update(
                    $distinguished_value::<E>::$distinguished_value_method::<true>(
                        dest,
                        buf.lend(),
                        ctx.clone(),
                    )?,
                );
            }
            if peek_repeated_field(&mut buf).is_some() {
                // Too many value fields
                Err(DecodeError::new(InvalidValue))
            } else {
                Ok(canon)
            }
        }
    };
}

pub(crate) mod owned {
    use super::*;
    decoding_modes::invoke!(define_decoders, owned);
}

pub(crate) mod borrowed {
    use super::*;
    decoding_modes::invoke!(define_decoders, borrowed);
}

/// Unpacked encodes vecs as repeated fields and in relaxed decoding mode will accept both packed
/// and un-packed encodings.
impl<C, T, E> Encoder<Unpacked<E>> for C
where
    C: Collection<Item = T>,
    T: ForOverwrite<E> + ValueEncoder<E>,
{
    #[inline]
    fn encode<B: BufMut + ?Sized>(tag: u32, value: &C, buf: &mut B, tw: &mut TagWriter) {
        for val in value.iter() {
            FieldEncoder::<E>::encode_field(tag, val, buf, tw);
        }
    }

    #[inline]
    fn prepend_encode<B: ReverseBuf + ?Sized>(
        tag: u32,
        value: &Self,
        buf: &mut B,
        tw: &mut TagRevWriter,
    ) {
        for val in value.reversed() {
            FieldEncoder::<E>::prepend_field(tag, val, buf, tw);
        }
    }

    #[inline]
    fn encoded_len(tag: u32, value: &C, tm: &mut impl TagMeasurer) -> usize {
        if !value.is_empty() {
            // Each *additional* field encoded after the first needs only 1 byte for the field key.
            tm.key_len(tag) + ValueEncoder::<E>::many_values_encoded_len(value.iter()) + value.len()
                - 1
        } else {
            0
        }
    }
}

/// Unpacked encodes arrays as repeated fields if any of the values are non-empty, and in relaxed
/// decoding mode will accept both packed and un-packed encodings.
impl<T, const N: usize, E> Encoder<Unpacked<E>> for [T; N]
where
    T: EmptyState<E> + ValueEncoder<E>,
{
    #[inline]
    fn encode<B: BufMut + ?Sized>(tag: u32, value: &[T; N], buf: &mut B, tw: &mut TagWriter) {
        if !EmptyState::is_empty(value) {
            for val in value.iter() {
                FieldEncoder::<E>::encode_field(tag, val, buf, tw);
            }
        }
    }

    #[inline]
    fn prepend_encode<B: ReverseBuf + ?Sized>(
        tag: u32,
        value: &Self,
        buf: &mut B,
        tw: &mut TagRevWriter,
    ) {
        if !EmptyState::is_empty(value) {
            for val in value.iter().rev() {
                FieldEncoder::<E>::prepend_field(tag, val, buf, tw);
            }
        }
    }

    #[inline]
    fn encoded_len(tag: u32, value: &[T; N], tm: &mut impl TagMeasurer) -> usize {
        if !EmptyState::is_empty(value) {
            // Each *additional* field encoded after the first needs only 1 byte for the field key.
            tm.key_len(tag) + ValueEncoder::<E>::many_values_encoded_len(value.iter()) + N - 1
        } else {
            0
        }
    }
}

/// Unpacked encodes arrays as repeated fields if any of the values are non-empty.
impl<T, const N: usize, E> Encoder<Unpacked<E>> for Option<[T; N]>
where
    T: ForOverwrite<E> + ValueEncoder<E>,
{
    #[inline]
    fn encode<B: BufMut + ?Sized>(
        tag: u32,
        value: &Option<[T; N]>,
        buf: &mut B,
        tw: &mut TagWriter,
    ) {
        if let Some(values) = value.as_ref() {
            for val in values {
                FieldEncoder::<E>::encode_field(tag, val, buf, tw);
            }
        }
    }

    #[inline]
    fn prepend_encode<B: ReverseBuf + ?Sized>(
        tag: u32,
        value: &Self,
        buf: &mut B,
        tw: &mut TagRevWriter,
    ) {
        if let Some(values) = value.as_ref() {
            for val in values.iter().rev() {
                FieldEncoder::<E>::prepend_field(tag, val, buf, tw);
            }
        }
    }

    #[inline]
    fn encoded_len(tag: u32, value: &Option<[T; N]>, tm: &mut impl TagMeasurer) -> usize {
        if let Some(values) = value.as_ref() {
            // Each *additional* field encoded after the first needs only 1 byte for the field key.
            tm.key_len(tag) + ValueEncoder::<E>::many_values_encoded_len(values.iter()) + N - 1
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
        impl<$($lifetime,)? C, T, E> $relaxed <$($lifetime,)? Unpacked<E>> for C
        where
            C: Collection<Item = T>,
            T: ForOverwrite<E> + $relaxed_value <$($lifetime,)? E>,
        {
            #[inline]
            fn $relaxed_method $($($buf_generic)*)? (
                wire_type: WireType,
                value: &mut C,
                buf: Capped<$buf_ty>,
                ctx: DecodeContext,
            ) -> Result<(), DecodeError> {
                if wire_type == WireType::LengthDelimited
                    && <C::Item as Wiretyped<E>>::WIRE_TYPE != WireType::LengthDelimited
                {
                    // We've encountered a length-delimited field when we aren't expecting one; try decoding
                    // it in packed format instead.
                    $relaxed_value::<Packed<E>>::$relaxed_value_method(value, buf, ctx)
                } else {
                    // Otherwise, decode in unpacked mode.
                    $mode::decode::<C, E>(wire_type, value, buf, ctx)
                }
            }
        }

        /// Distinguished encoding enforces only the repeated field representation is allowed.
        impl<$($lifetime,)? C, T, E> $distinguished <$($lifetime,)? Unpacked<E>> for C
        where
            Self: DistinguishedCollection<Item = T>
                + $relaxed_value <$($lifetime,)? Packed<E>>
                + $relaxed <$($lifetime,)? Unpacked<E>>,
            T: ForOverwrite<E> + Eq + $distinguished_value <$($lifetime,)? E>,
        {
            #[inline]
            fn $distinguished_method $($($buf_generic)*)? (
                wire_type: WireType,
                value: &mut C,
                buf: Capped<$buf_ty>,
                ctx: RestrictedDecodeContext,
            ) -> Result<Canonicity, DecodeError> {
                if wire_type == WireType::LengthDelimited
                    && <T as Wiretyped<E>>::WIRE_TYPE != WireType::LengthDelimited
                {
                    // We've encountered a length-delimited field when we aren't expecting one; try decoding
                    // it in packed format instead.
                    // The data is already known to be non-canonical; use relaxed decoding
                    _ = ctx.check(Canonicity::NotCanonical)?;
                    $relaxed_value::<Packed<E>>::$relaxed_value_method(
                        value,
                        buf,
                        ctx.into_inner(),
                    )?;
                    Ok(Canonicity::NotCanonical)
                } else {
                    // Otherwise, decode in unpacked mode.
                    $mode::decode_distinguished::<C, E>(wire_type, value, buf, ctx)
                }
            }
        }

        impl<$($lifetime,)? T, const N: usize, E> $relaxed <$($lifetime,)? Unpacked<E>> for [T; N]
        where
            T: EmptyState<E> + $relaxed_value <$($lifetime,)? E>,
        {
            #[inline]
            fn $relaxed_method $($($buf_generic)*)? (
                wire_type: WireType,
                value: &mut [T; N],
                buf: Capped<$buf_ty>,
                ctx: DecodeContext,
            ) -> Result<(), DecodeError> {
                $mode::decode_array_either_repr(wire_type, value, buf, ctx)
            }
        }

        /// Distinguished encoding considers only the repeated field representation to be canonical.
        impl<$($lifetime,)? T, const N: usize, E>
        $distinguished <$($lifetime,)? Unpacked<E>> for [T; N]
        where
            T: Eq
                + EmptyState<E>
                + $distinguished_value <$($lifetime,)? E>
                + $relaxed_value <$($lifetime,)? E>,
        {
            #[inline]
            fn $distinguished_method $($($buf_generic)*)? (
                wire_type: WireType,
                value: &mut [T; N],
                buf: Capped<$buf_ty>,
                ctx: RestrictedDecodeContext,
            ) -> Result<Canonicity, DecodeError> {
                let canon = $mode::decode_distinguished_array_either_repr(
                    wire_type,
                    value,
                    buf,
                    ctx.clone(),
                )?;
                if EmptyState::is_empty(value) {
                    ctx.check(Canonicity::NotCanonical)
                } else {
                    Ok(canon)
                }
            }
        }

        impl<$($lifetime,)? T, const N: usize, E>
        $relaxed <$($lifetime,)? Unpacked<E>> for Option<[T; N]>
        where
            T: ForOverwrite<E> + $relaxed_value <$($lifetime,)? E>,
        {
            #[inline]
            fn $relaxed_method $($($buf_generic)*)? (
                wire_type: WireType,
                value: &mut Option<[T; N]>,
                buf: Capped<$buf_ty>,
                ctx: DecodeContext,
            ) -> Result<(), DecodeError> {
                $mode::decode_array_either_repr(
                    wire_type,
                    value.get_or_insert_with(ForOverwrite::for_overwrite),
                    buf,
                    ctx,
                )
            }
        }

        /// Distinguished encoding enforces only the repeated field representation is considered to be
        /// canonical.
        impl<$($lifetime,)? T, const N: usize, E>
        $distinguished <$($lifetime,)? Unpacked<E>> for Option<[T; N]>
        where
            T: Eq
                + ForOverwrite<E>
                + $distinguished_value<$($lifetime,)? E>
                + $relaxed_value<$($lifetime,)? E>,
        {
            #[inline]
            fn $distinguished_method $($($buf_generic)*)? (
                wire_type: WireType,
                value: &mut Option<[T; N]>,
                buf: Capped<$buf_ty>,
                ctx: RestrictedDecodeContext,
            ) -> Result<Canonicity, DecodeError> {
                $mode::decode_distinguished_array_either_repr(
                    wire_type,
                    value.get_or_insert_with(ForOverwrite::for_overwrite),
                    buf,
                    ctx,
                )
            }
        }
    };
}

decoding_modes::invoke!(impl_decoders, owned);
decoding_modes::invoke!(impl_decoders, borrowed);

#[cfg(test)]
mod test {
    use alloc::string::String;
    use alloc::vec::Vec;

    use proptest::proptest;

    use crate::encoding::test::{distinguished, relaxed};
    use crate::encoding::{Fixed, Unpacked, WireType};

    proptest! {
        #[test]
        fn varint(value: Vec<u64>, tag: u32) {
            relaxed::check_type_unpacked::<Vec<u64>, Unpacked>(
                value.clone(),
                tag,
                WireType::Varint,
            )?;
            distinguished::check_type_unpacked::<Vec<u64>, Unpacked>(value, tag, WireType::Varint)?;
        }

        #[test]
        fn length_delimited(value: Vec<String>, tag: u32) {
            relaxed::check_type_unpacked::<Vec<String>, Unpacked>(
                value.clone(),
                tag,
                WireType::LengthDelimited,
            )?;
            distinguished::check_type_unpacked::<Vec<String>, Unpacked>(
                value,
                tag,
                WireType::LengthDelimited,
            )?;
        }

        #[test]
        fn fixed32(value: Vec<u32>, tag: u32) {
            relaxed::check_type_unpacked::<Vec<u32>, Unpacked<Fixed>>(
                value.clone(),
                tag,
                WireType::ThirtyTwoBit,
            )?;
            distinguished::check_type_unpacked::<Vec<u32>, Unpacked<Fixed>>(
                value,
                tag,
                WireType::ThirtyTwoBit,
            )?;
        }

        #[test]
        fn fixed64(value: Vec<u64>, tag: u32) {
            relaxed::check_type_unpacked::<Vec<u64>, Unpacked<Fixed>>(
                value.clone(),
                tag,
                WireType::SixtyFourBit,
            )?;
            distinguished::check_type_unpacked::<Vec<u64>, Unpacked<Fixed>>(
                value,
                tag,
                WireType::SixtyFourBit,
            )?;
        }

        #[test]
        fn varint_array(value: [u64; 2], tag: u32) {
            relaxed::check_type_unpacked::<[u64; 2], Unpacked>(
                value,
                tag,
                WireType::Varint,
            )?;
            distinguished::check_type_unpacked::<[u64; 2], Unpacked>(value, tag, WireType::Varint)?;
        }

        #[test]
        fn length_delimited_array(value: [String; 2], tag: u32) {
            relaxed::check_type_unpacked::<[String; 2], Unpacked>(
                value.clone(),
                tag,
                WireType::LengthDelimited,
            )?;
            distinguished::check_type_unpacked::<[String; 2], Unpacked>(
                value,
                tag,
                WireType::LengthDelimited,
            )?;
        }

        #[test]
        fn fixed32_array(value: [u32; 2], tag: u32) {
            relaxed::check_type_unpacked::<[u32; 2], Unpacked<Fixed>>(
                value,
                tag,
                WireType::ThirtyTwoBit,
            )?;
            distinguished::check_type_unpacked::<[u32; 2], Unpacked<Fixed>>(
                value,
                tag,
                WireType::ThirtyTwoBit,
            )?;
        }

        #[test]
        fn fixed64_array(value: [u64; 2], tag: u32) {
            relaxed::check_type_unpacked::<[u64; 2], Unpacked<Fixed>>(
                value,
                tag,
                WireType::SixtyFourBit,
            )?;
            distinguished::check_type_unpacked::<[u64; 2], Unpacked<Fixed>>(
                value,
                tag,
                WireType::SixtyFourBit,
            )?;
        }
    }
}
