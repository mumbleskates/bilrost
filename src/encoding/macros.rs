//! Macro rules for expressly delegating from one encoder to another.

/// Expressly delegates support for encoding message fields from one encoding to another.
#[macro_export]
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
                value: &mut $value_ty,
                buf: $crate::encoding::Capped<B>,
                ctx: $crate::encoding::DecodeContext,
            ) -> Result<(), $crate::DecodeError> {
                $crate::encoding::Decoder::<$to_ty>::decode(
                    wire_type,
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
                value: &mut $value_ty,
                buf: $crate::encoding::Capped<&'__a [u8]>,
                ctx: $crate::encoding::DecodeContext,
            ) -> Result<(), $crate::DecodeError> {
                $crate::encoding::BorrowDecoder::<$to_ty>::borrow_decode(
                    wire_type,
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
                value: &mut $value_ty,
                buf: $crate::encoding::Capped<B>,
                ctx: $crate::encoding::RestrictedDecodeContext,
            ) -> Result<$crate::Canonicity, $crate::DecodeError> {
                $crate::encoding::DistinguishedDecoder::<$to_ty>::decode_distinguished(
                    wire_type,
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
                value: &mut $value_ty,
                buf: $crate::encoding::Capped<&'__a [u8]>,
                ctx: $crate::encoding::RestrictedDecodeContext,
            ) -> Result<$crate::Canonicity, $crate::DecodeError> {
                $crate::encoding::DistinguishedBorrowDecoder::<$to_ty>::borrow_decode_distinguished(
                    wire_type,
                    value,
                    buf,
                    ctx,
                )
            }
        }
    };
}
pub use delegate_encoding;

/// Expressly delegates support for encoding *values* from one encoding to another, but not message
/// fields themselves. Most built-in encodings in bilrost have the ability to encode any type they
/// can encode as a value as a message field as well; a notable exception is the special `Proxied`
/// encoding, which cannot.
///
/// Also includes support for providing borrowed value decoding implementations that delegate to the
/// owned implementation, for types which have no borrowed representation. Most types supported by
/// `bilrost` cannot be meaningfully borrowed and delegate their borrowed decoding impls back to the
/// owned impls this way.
///
/// Delegation is implemented by macro type-by-type rather than as a blanket impl (such as borrowed
/// encoding whenever owned decoding exists) right now because we want `impl RawMessage` to provide
/// value encoding and decoding in the `General` encoding. This continues to preclude any other
/// blanket trait delegation.
#[macro_export]
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

        impl<'__a $(, $($value_generics)*)?>
        $crate::encoding::ValueBorrowDecoder<'__a, $from_ty> for $value_ty
        where
            Self: $crate::encoding::ValueBorrowDecoder<'__a, $to_ty>,
            $($($where_clause)+ ,)?
        {
            #[inline(always)]
            fn borrow_decode_value(
                value: &mut $value_ty,
                buf: $crate::encoding::Capped<&'__a [u8]>,
                ctx: $crate::encoding::DecodeContext,
            ) -> Result<(), $crate::DecodeError> {
                $crate::encoding::ValueBorrowDecoder::<$to_ty>::borrow_decode_value(value, buf, ctx)
            }
        }
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

        impl<'__a $(, $($value_generics)*)?>
        $crate::encoding::DistinguishedValueBorrowDecoder<'__a, $from_ty> for $value_ty
        where
            Self: $crate::encoding::DistinguishedValueBorrowDecoder<'__a, $to_ty>,
            $($($relaxed_where)+ ,)?
            $($($distinguished_where)+ ,)?
        {
            const CHECKS_EMPTY: bool = <
                $value_ty as $crate::encoding::DistinguishedValueBorrowDecoder<'__a, $to_ty>
            >::CHECKS_EMPTY;

            #[inline(always)]
            fn borrow_decode_value_distinguished<const ALLOW_EMPTY: bool>(
                value: &mut $value_ty,
                buf: $crate::encoding::Capped<&'__a [u8]>,
                ctx: $crate::encoding::RestrictedDecodeContext,
            ) -> Result<$crate::Canonicity, $crate::DecodeError> {
                $crate::encoding::DistinguishedValueBorrowDecoder::<$to_ty>::
                    borrow_decode_value_distinguished::<ALLOW_EMPTY>
                (
                    value,
                    buf,
                    ctx,
                )
            }
        }
    };

    (
        encoding ($encoding:ty) borrows type ($ty:ty) as owned
        $(with where clause ($($where_clause:tt)+))?
        $(with generics ($($impl_generics:tt)*))?
    ) => {
        impl<'__a, $($($impl_generics)*)?>
        $crate::encoding::ValueBorrowDecoder<'__a, $encoding> for $ty
        where
            $ty: $crate::encoding::ValueDecoder<$encoding>,
        {
            #[inline]
            fn borrow_decode_value(
                value: &mut Self,
                buf: $crate::encoding::Capped<&'__a [u8]>,
                ctx: $crate::encoding::DecodeContext,
            ) -> Result<(), $crate::DecodeError> {
                $crate::encoding::ValueDecoder::<$encoding>::decode_value(value, buf, ctx)
            }
        }
    };

    (
        encoding ($encoding:ty) borrows type ($ty:ty) as owned including distinguished
        $(with where clause ($($where_clause:tt)+))?
        $(with generics ($($impl_generics:tt)*))?
    ) => {
        $crate::encoding::delegate_value_encoding!(
            encoding ($encoding) borrows type ($ty) as owned
            $(with where clause ($($where_clause)*))?
            $(with generics ($($impl_generics)*))?
        );

        impl<'__a, $($($impl_generics)*)?>
        $crate::encoding::DistinguishedValueBorrowDecoder<'__a, $encoding> for $ty
        where
            $ty: $crate::encoding::DistinguishedValueDecoder<$encoding>,
        {
            const CHECKS_EMPTY: bool =
                <$ty as $crate::encoding::DistinguishedValueDecoder<$encoding>>::CHECKS_EMPTY;

            #[inline]
            fn borrow_decode_value_distinguished<const ALLOW_EMPTY: bool>(
                value: &mut Self,
                buf: $crate::encoding::Capped<&'__a [u8]>,
                ctx: $crate::encoding::RestrictedDecodeContext,
            ) -> Result<$crate::Canonicity, $crate::DecodeError> {
                $crate::encoding::DistinguishedValueDecoder::<$encoding>::
                    decode_value_distinguished::<ALLOW_EMPTY>
                (
                    value,
                    buf,
                    ctx,
                )
            }
        }
    };
}
pub use delegate_value_encoding;

// TODO(widders): docs
#[macro_export]
macro_rules! delegate_proxied_encoding {
    (
        use encoding ($to:ty)
        to encode proxied type ($value_ty:ty)
        $(using proxy tag ($proxy_tag:ty))?
        with encoding ($from:ty)
        $(with where clause ($($where_clause:tt)+))?
        $(with generics ($($impl_generics:tt)*))?
    ) => {
        $crate::delegate_value_encoding!(
            delegate from ($from)
            to ($crate::encoding::Proxied<$to $(, $proxy_tag)?>)
            for type ($value_ty)
            $(with where clause ($($where_clause)*))?
            $(with generics ($($impl_generics)*))?
        );
    };
    (
        use encoding ($to:ty)
        to encode proxied type ($value_ty:ty)
        $(using proxy tag ($proxy_tag:ty))?
        with encoding ($from:ty)
        including distinguished
        $(with where clause for relaxed ($($relaxed_where:tt)*))?
        $(with where clause for distinguished ($($distinguished_where:tt)*))?
        $(with generics ($($impl_generics:tt)*))?
    ) => {
        $crate::delegate_value_encoding!(
            delegate from ($from)
            to ($crate::encoding::Proxied<$to $(, $proxy_tag)?>)
            for type ($value_ty)
            including distinguished
            $(with where clause for relaxed ($($relaxed_where)*))?
            $(with where clause for distinguished ($($distinguished_where)*))?
            $(with generics ($($impl_generics)*))?
        );
    };

    (
        use encoding ($to:ty)
        to encode proxied type ($value_ty:ty)
        $(using proxy tag ($proxy_tag:ty))?
        with general encodings
        $(with where clause ($($where_clause:tt)+))?
        $(with generics ($($impl_generics:tt)*))?
    ) => {
        $crate::delegate_value_encoding!(
            delegate from ($crate::encoding::GeneralGeneric<__G>)
            to ($crate::encoding::Proxied<$to $(, $proxy_tag)?>)
            for type ($value_ty)
            $(with where clause ($($where_clause)*))?
            with generics (const __G: u8, $($($impl_generics)*)?)
        );
    };
    (
        use encoding ($to:ty)
        to encode proxied type ($value_ty:ty)
        $(using proxy tag ($proxy_tag:ty))?
        with general encodings
        including distinguished
        $(with where clause for relaxed ($($relaxed_where:tt)*))?
        $(with where clause for distinguished ($($distinguished_where:tt)*))?
        $(with generics ($($impl_generics:tt)*))?
    ) => {
        $crate::delegate_value_encoding!(
            delegate from ($crate::encoding::GeneralGeneric<__G>)
            to ($crate::encoding::Proxied<$to $(, $proxy_tag)?>)
            for type ($value_ty)
            including distinguished
            $(with where clause for relaxed ($($relaxed_where)*))?
            $(with where clause for distinguished ($($distinguished_where)*))?
            with generics (const __G: u8, $($($impl_generics)*)?)
        );
    };
}
pub use delegate_proxied_encoding;

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
                wire_type: $crate::encoding::WireType,
                value: &mut T,
                buf: $crate::encoding::Capped<$buf_ty>,
                ctx: $crate::encoding::DecodeContext,
            ) -> ::core::result::Result<(), $crate::DecodeError> {
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
            T: ::core::cmp::Eq
                + $crate::encoding::EmptyState
                + $crate::encoding::$distinguished_value <$($lifetime,)? $encoding>,
            $($($where_clause)*)?
        {
            #[inline(always)]
            fn $distinguished_method $($($buf_generic)*)? (
                wire_type: $crate::encoding::WireType,
                value: &mut T,
                buf: $crate::encoding::Capped<$buf_ty>,
                ctx: $crate::encoding::RestrictedDecodeContext,
            ) -> ::core::result::Result<$crate::Canonicity, $crate::DecodeError> {
                // decoding a value as a whole message field, empty values are unacceptable
                let mut canon = $crate::encoding::$distinguished_field::<$encoding>
                    ::$distinguished_field_method::<false>(
                        wire_type,
                        value,
                        buf,
                        ctx.clone(),
                    )?;
                if !T::CHECKS_EMPTY && value.is_empty() {
                    canon.update(ctx.check(crate::Canonicity::NotCanonical)?);
                }
                Ok(canon)
            }
        }
    };
}
pub(crate) use __impl_decoder_where_value_decoder;

macro_rules! encoding_implemented_via_value_encoding {
    (
        $encoding:ty
        $(, with where clause ($($where_clause:tt)*))?
        $(, with generics ($($generics:tt)*) $(,)?)?
    ) => {
        /// Encodes plain values only when they are non-default.
        impl<T $(, $($generics)*)?> $crate::encoding::Encoder<$encoding> for T
        where
            T: $crate::encoding::EmptyState + $crate::encoding::ValueEncoder<$encoding>,
            $($($where_clause)*)?
        {
            #[inline(always)]
            fn encode<B: $crate::bytes::BufMut + ?Sized>(
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
pub(crate) use encoding_implemented_via_value_encoding;

/// Cow<'a, T> implements owned decoding for owned traits and borrowed decoding for borrowed traits.
/// This isn't writable as one or even several blanket impls for multiple reasons:
///
/// 1. blanket impls for Cow collide with things like the Collection for `Cow<[T]>` trait, which has
///    value decoding via packed
/// 2. if we try to define an impl that generalizes for just one encoder, we cannot spell the type
///    of the value we are encoding because it's a reference and we need to be able to encode it by
///    two different lifetimes
/// 3. we cannot make encoding work through references, because implementing
///    `ValueEncoder<E> for &T where T: ValueEncoder` collides with public and externally
///    implementable traits
/// 4. we cannot make encoding work on the referenced data instead of the owned data, because most
///    types don't have a referent at all and are just a value
///
/// ...but we can deduplicate the impls via macro, which can know by caller fiat that the needed
/// traits are going to be available for all the reference lifetimes we need.
macro_rules! impl_cow_value_encoding {
    (borrowed $T:ty, owned $Owned:ty, encoding $E:ty $(, with generic ($($generic:tt)*))?) => {
        const _: () = {
            use alloc::borrow::Cow;
            use bytes::{Buf, BufMut};
            use $crate::buf::ReverseBuf;
            use $crate::encoding::{
                Capped, DecodeContext, DistinguishedValueBorrowDecoder, DistinguishedValueDecoder,
                ForOverwrite, RestrictedDecodeContext, ValueBorrowDecoder, ValueEncoder, WireType,
                Wiretyped,
            };
            use $crate::{Canonicity, DecodeError};

            impl$(<$($generic)*>)? Wiretyped<$E> for Cow<'_, $T> {
                const WIRE_TYPE: WireType = {
                    let b = <&$T as Wiretyped<$E>>::WIRE_TYPE;
                    let o = <$Owned as Wiretyped<$E>>::WIRE_TYPE;
                    match (b, o) {
                        (WireType::Varint, WireType::Varint) => {}
                        (WireType::LengthDelimited, WireType::LengthDelimited) => {}
                        (WireType::ThirtyTwoBit, WireType::ThirtyTwoBit) => {}
                        (WireType::SixtyFourBit, WireType::SixtyFourBit) => {}
                        _ => {
                            panic!("wiretypes mismatched for Cow impl");
                        }
                    }
                    b
                };
            }

            impl$(<$($generic)*>)? ValueEncoder<$E> for Cow<'_, $T> {
                #[inline]
                fn encode_value<B: BufMut + ?Sized>(value: &Cow<$T>, buf: &mut B) {
                    ValueEncoder::<$E>::encode_value(&&**value, buf)
                }

                #[inline]
                fn prepend_value<B: ReverseBuf + ?Sized>(value: &Cow<$T>, buf: &mut B) {
                    ValueEncoder::<$E>::prepend_value(&&**value, buf)
                }

                #[inline]
                fn value_encoded_len(value: &Cow<$T>) -> usize {
                    ValueEncoder::<$E>::value_encoded_len(&&**value)
                }
            }

            impl$(<$($generic)*>)? ValueDecoder<$E> for Cow<'_, $T> {
                #[inline]
                fn decode_value<B: Buf + ?Sized>(
                    value: &mut Cow<$T>,
                    buf: Capped<B>,
                    ctx: DecodeContext,
                ) -> Result<(), DecodeError> {
                    ValueDecoder::<$E>::decode_value(value.to_mut(), buf, ctx)
                }
            }

            impl$(<$($generic)*>)? DistinguishedValueDecoder<$E> for Cow<'_, $T> {
                const CHECKS_EMPTY: bool = <$Owned as DistinguishedValueDecoder<$E>>::CHECKS_EMPTY;

                #[inline]
                fn decode_value_distinguished<const ALLOW_EMPTY: bool>(
                    value: &mut Cow<$T>,
                    buf: Capped<impl Buf + ?Sized>,
                    ctx: RestrictedDecodeContext,
                ) -> Result<Canonicity, DecodeError> {
                    DistinguishedValueDecoder::<$E>::decode_value_distinguished::<ALLOW_EMPTY>(
                        value.to_mut(),
                        buf,
                        ctx,
                    )
                }
            }

            impl<'a $(, $($generic)*)?> ValueBorrowDecoder<'a, $E> for Cow<'a, $T> {
                #[inline]
                fn borrow_decode_value(
                    value: &mut Cow<'a, $T>,
                    buf: Capped<&'a [u8]>,
                    ctx: DecodeContext,
                ) -> Result<(), DecodeError> {
                    let mut s = <&$T>::for_overwrite();
                    ValueBorrowDecoder::<$E>::borrow_decode_value(&mut s, buf, ctx)?;
                    *value = Cow::Borrowed(s);
                    Ok(())
                }
            }

            impl<'a $(, $($generic)*)?> DistinguishedValueBorrowDecoder<'a, $E> for Cow<'a, $T> {
                const CHECKS_EMPTY: bool =
                    <&$T as DistinguishedValueBorrowDecoder<'a, $E>>::CHECKS_EMPTY;

                #[inline]
                fn borrow_decode_value_distinguished<const ALLOW_EMPTY: bool>(
                    value: &mut Cow<'a, $T>,
                    buf: Capped<&'a [u8]>,
                    ctx: RestrictedDecodeContext,
                ) -> Result<Canonicity, DecodeError> {
                    let mut s = <&$T>::for_overwrite();
                    let canon =
                        DistinguishedValueBorrowDecoder::<$E>::borrow_decode_value_distinguished::<
                            ALLOW_EMPTY,
                        >(&mut s, buf, ctx)?;
                    *value = Cow::Borrowed(s);
                    Ok(canon)
                }
            }
        };
    };
}
pub(crate) use impl_cow_value_encoding;
