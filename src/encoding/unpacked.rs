use bytes::{Buf, BufMut};

use crate::buf::ReverseBuf;
use crate::encoding::value_traits::{
    Collection, DistinguishedCollection, EmptyState, ForOverwrite,
};
use crate::encoding::{
    check_wire_type, peek_repeated_field, BorrowDecoder, Capped, DecodeContext, Decoder,
    DistinguishedBorrowDecoder, DistinguishedDecoder, DistinguishedValueBorrowDecoder,
    DistinguishedValueDecoder, Encoder, FieldEncoder, General, Packed, RestrictedDecodeContext,
    TagMeasurer, TagRevWriter, TagWriter, ValueBorrowDecoder, ValueDecoder, ValueEncoder, WireType,
    Wiretyped,
};
use crate::DecodeErrorKind::{InvalidValue, UnexpectedlyRepeated};
use crate::{Canonicity, DecodeError};

pub struct Unpacked<E = General>(E);

macro_rules! define_decoders {
    (
        decoder: $decoder:ident,
        value_decoder: $value_decoder:ident::$value_decoder_method:ident,
        distinguished_value_decoder:
            $distinguished_value_decoder:ident::$distinguished_value_decoder_method:ident,
        buf: ($($buf:tt)*),
        $(lifetime: $lifetime:lifetime,)?
    ) => {
        /// Decodes a collection value from the unpacked representation. This greedily consumes
        /// consecutive fields as long as they have the same tag.
        #[inline]
        pub(crate) fn decode<$($lifetime,)? T, E>(
            wire_type: WireType,
            collection: &mut T,
            mut buf: Capped<$($buf)*>,
            ctx: DecodeContext,
        ) -> Result<(), DecodeError>
        where
            T: Collection,
            T::Item: ForOverwrite + $value_decoder <$($lifetime,)? E>,
        {
            check_wire_type(<T::Item as Wiretyped<E>>::WIRE_TYPE, wire_type)?;
            loop {
                // Decode one item
                let mut new_item = T::Item::for_overwrite();
                $value_decoder::<E>::$value_decoder_method(&mut new_item, buf.lend(), ctx.clone())?;
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
            buf: Capped<$($buf)*>,
            ctx: DecodeContext,
        ) -> Result<(), DecodeError>
        where
            T: $value_decoder <$($lifetime,)? E>,
        {
            if wire_type == WireType::LengthDelimited
                && <T as Wiretyped<E>>::WIRE_TYPE != WireType::LengthDelimited
            {
                // We've encountered a length-delimited field when we aren't expecting one; try
                // decoding it in packed format instead.
                $value_decoder::<Packed<E>>::$value_decoder_method(arr, buf, ctx)
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
            mut buf: Capped<$($buf)*>,
            ctx: DecodeContext,
        ) -> Result<(), DecodeError>
        where
            T: $value_decoder <$($lifetime,)? E>,
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
                $value_decoder::<E>::$value_decoder_method(dest, buf.lend(), ctx.clone())?;
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
            mut buf: Capped<$($buf)*>,
            ctx: RestrictedDecodeContext,
        ) -> Result<Canonicity, DecodeError>
        where
            T: DistinguishedCollection,
            T::Item: ForOverwrite + Eq + $distinguished_value_decoder <$($lifetime,)? E>,
        {
            check_wire_type(<T::Item as Wiretyped<E>>::WIRE_TYPE, wire_type)?;
            let mut canon = Canonicity::Canonical;
            loop {
                // Decode one item
                let mut new_item = T::Item::for_overwrite();
                // Decoded field values are nested within the collection; empty values are OK
                canon.update(
                    $distinguished_value_decoder::<E>::$distinguished_value_decoder_method::<true>(
                        &mut new_item,
                        buf.lend(),
                        ctx.clone(),
                    )?,
                );
                ctx.update(&mut canon, collection.insert_distinguished(new_item)?)?;

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
            buf: Capped<$($buf)*>,
            ctx: RestrictedDecodeContext,
        ) -> Result<Canonicity, DecodeError>
        where
            T: Eq
                + $value_decoder <$($lifetime,)? E>
                + $distinguished_value_decoder <$($lifetime,)? E>,
        {
            if wire_type == WireType::LengthDelimited
                && <T as Wiretyped<E>>::WIRE_TYPE != WireType::LengthDelimited
            {
                // We've encountered a length-delimited field when we aren't expecting one; try
                // decoding it in packed format instead.
                // The data is already known to be non-canonical; use relaxed decoding
                _ = ctx.check(Canonicity::NotCanonical)?;
                $value_decoder::<Packed<E>>::$value_decoder_method(arr, buf, ctx.into_inner())?;
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
            mut buf: Capped<$($buf)*>,
            ctx: RestrictedDecodeContext,
        ) -> Result<Canonicity, DecodeError>
        where
            T: Eq + $distinguished_value_decoder <$($lifetime,)? E>,
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
                    $distinguished_value_decoder::<E>::$distinguished_value_decoder_method::<true>(
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

    define_decoders!(
        decoder: Decoder,
        value_decoder: ValueDecoder::decode_value,
        distinguished_value_decoder: DistinguishedValueDecoder::decode_value_distinguished,
        buf: (impl Buf + ?Sized),
    );
}

pub(crate) mod borrowed {
    use super::*;

    define_decoders!(
        decoder: BorrowDecoder,
        value_decoder: ValueBorrowDecoder::borrow_decode_value,
        distinguished_value_decoder:
            DistinguishedValueBorrowDecoder::borrow_decode_value_distinguished,
        buf: (&'a [u8]),
        lifetime: 'a,
    );
}

/// Unpacked encodes vecs as repeated fields and in relaxed decoding mode will accept both packed
/// and un-packed encodings.
impl<C, T, E> Encoder<Unpacked<E>> for C
where
    C: Collection<Item = T>,
    T: ForOverwrite + ValueEncoder<E>,
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
    T: EmptyState + ValueEncoder<E>,
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
    T: ForOverwrite + ValueEncoder<E>,
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
        decoder: $decoder:ident::$decoder_method:ident,
        distinguished_decoder: $distinguished_decoder:ident::$distinguished_decoder_method:ident,
        value_decoder: $value_decoder:ident::$value_decoder_method:ident,
        distinguished_value_decoder:
            $distinguished_value_decoder:ident::$distinguished_value_decoder_method:ident,
        mode: $mode:ident,
        $(buf_bound: $buf:ident => ($($buf_bound:tt)*),)?
        $(lifetime: $lifetime:lifetime,)?
    ) => {
        impl<$($lifetime,)? C, T, E> $decoder <$($lifetime,)? Unpacked<E>> for C
        where
            C: Collection<Item = T>,
            T: ForOverwrite + $value_decoder <$($lifetime,)? E>,
        {
            #[inline]
            fn $decoder_method $(<$buf: $($buf_bound)*>)? (
                wire_type: WireType,
                duplicated: bool,
                value: &mut C,
                buf: Capped<$($buf)? $(&$lifetime [u8])?>,
                ctx: DecodeContext,
            ) -> Result<(), DecodeError> {
                if duplicated {
                    return Err(DecodeError::new(UnexpectedlyRepeated));
                }
                if wire_type == WireType::LengthDelimited
                    && <C::Item as Wiretyped<E>>::WIRE_TYPE != WireType::LengthDelimited
                {
                    // We've encountered a length-delimited field when we aren't expecting one; try decoding
                    // it in packed format instead.
                    $value_decoder::<Packed<E>>::$value_decoder_method(value, buf, ctx)
                } else {
                    // Otherwise, decode in unpacked mode.
                    $mode::decode::<C, E>(wire_type, value, buf, ctx)
                }
            }
        }

        /// Distinguished encoding enforces only the repeated field representation is allowed.
        impl<$($lifetime,)? C, T, E> $distinguished_decoder <$($lifetime,)? Unpacked<E>> for C
        where
            Self: DistinguishedCollection<Item = T>
                + $value_decoder <$($lifetime,)? Packed<E>>
                + $decoder <$($lifetime,)? Unpacked<E>>,
            T: ForOverwrite + Eq + $distinguished_value_decoder <$($lifetime,)? E>,
        {
            #[inline]
            fn $distinguished_decoder_method $(<$buf: $($buf_bound)*>)? (
                wire_type: WireType,
                duplicated: bool,
                value: &mut C,
                buf: Capped<$($buf)? $(&$lifetime [u8])?>,
                ctx: RestrictedDecodeContext,
            ) -> Result<Canonicity, DecodeError> {
                if duplicated {
                    return Err(DecodeError::new(UnexpectedlyRepeated));
                }
                if wire_type == WireType::LengthDelimited
                    && <T as Wiretyped<E>>::WIRE_TYPE != WireType::LengthDelimited
                {
                    // We've encountered a length-delimited field when we aren't expecting one; try decoding
                    // it in packed format instead.
                    // The data is already known to be non-canonical; use relaxed decoding
                    _ = ctx.check(Canonicity::NotCanonical)?;
                    $value_decoder::<Packed<E>>::$value_decoder_method(
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

        impl<$($lifetime,)? T, const N: usize, E> $decoder <$($lifetime,)? Unpacked<E>> for [T; N]
        where
            T: EmptyState + $value_decoder <$($lifetime,)? E>,
        {
            #[inline]
            fn $decoder_method $(<$buf: $($buf_bound)*>)? (
                wire_type: WireType,
                duplicated: bool,
                value: &mut [T; N],
                buf: Capped<$($buf)? $(&$lifetime [u8])?>,
                ctx: DecodeContext,
            ) -> Result<(), DecodeError> {
                if duplicated {
                    return Err(DecodeError::new(UnexpectedlyRepeated));
                }
                $mode::decode_array_either_repr(wire_type, value, buf, ctx)
            }
        }

        /// Distinguished encoding considers only the repeated field representation to be canonical.
        impl<$($lifetime,)? T, const N: usize, E>
        $distinguished_decoder <$($lifetime,)? Unpacked<E>> for [T; N]
        where
            T: Eq
                + EmptyState
                + $distinguished_value_decoder <$($lifetime,)? E>
                + $value_decoder <$($lifetime,)? E>,
        {
            #[inline]
            fn $distinguished_decoder_method $(<$buf: $($buf_bound)*>)? (
                wire_type: WireType,
                duplicated: bool,
                value: &mut [T; N],
                buf: Capped<$($buf)? $(&$lifetime [u8])?>,
                ctx: RestrictedDecodeContext,
            ) -> Result<Canonicity, DecodeError> {
                if duplicated {
                    return Err(DecodeError::new(UnexpectedlyRepeated));
                }
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
        $decoder <$($lifetime,)? Unpacked<E>> for Option<[T; N]>
        where
            T: ForOverwrite + $value_decoder <$($lifetime,)? E>,
        {
            #[inline]
            fn $decoder_method $(<$buf: $($buf_bound)*>)? (
                wire_type: WireType,
                duplicated: bool,
                value: &mut Option<[T; N]>,
                buf: Capped<$($buf)? $(&$lifetime [u8])?>,
                ctx: DecodeContext,
            ) -> Result<(), DecodeError> {
                if duplicated {
                    return Err(DecodeError::new(UnexpectedlyRepeated));
                }
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
        $distinguished_decoder <$($lifetime,)? Unpacked<E>> for Option<[T; N]>
        where
            T: Eq
                + ForOverwrite
                + $distinguished_value_decoder<$($lifetime,)? E>
                + $value_decoder<$($lifetime,)? E>,
        {
            #[inline]
            fn $distinguished_decoder_method $(<$buf: $($buf_bound)*>)? (
                wire_type: WireType,
                duplicated: bool,
                value: &mut Option<[T; N]>,
                buf: Capped<$($buf)? $(&$lifetime [u8])?>,
                ctx: RestrictedDecodeContext,
            ) -> Result<Canonicity, DecodeError> {
                if duplicated {
                    return Err(DecodeError::new(UnexpectedlyRepeated));
                }
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

impl_decoders!(
    decoder: Decoder::decode,
    distinguished_decoder: DistinguishedDecoder::decode_distinguished,
    value_decoder: ValueDecoder::decode_value,
    distinguished_value_decoder: DistinguishedValueDecoder::decode_value_distinguished,
    mode: owned,
    buf_bound: B => (Buf + ?Sized),
);

impl_decoders!(
    decoder: BorrowDecoder::borrow_decode,
    distinguished_decoder: DistinguishedBorrowDecoder::borrow_decode_distinguished,
    value_decoder: ValueBorrowDecoder::borrow_decode_value,
    distinguished_value_decoder: DistinguishedValueBorrowDecoder::borrow_decode_value_distinguished,
    mode: borrowed,
    lifetime: 'a,
);

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
