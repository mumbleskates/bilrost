//! Macro rules for expressly delegating from one encoder to another.

/// Expressly delegates support for encoding message fields from one encoding to another.
#[macro_export]
macro_rules! delegate_encoding {
    (
        delegate schema from ($from_ty:ty) to ($to_ty:ty) for type ($value_ty:ty)
        $(with where clause ($($where_clause:tt)*))?
        $(with generics ($($value_generics:tt)*))?
    ) => {
        impl$(<$($value_generics)*>)? $crate::encoding::schema::FieldRepr<$from_ty, $value_ty>
        for ()
            where
            (): $crate::encoding::schema::FieldRepr<$to_ty, $value_ty>,
            $($($where_clause)*)?
        {
            fn repr(
                schema: &$crate::encoding::schema::Schema
            ) -> $crate::alloc::boxed::Box<dyn ::core::fmt::Display> {
                <() as $crate::encoding::schema::FieldRepr<$to_ty, $value_ty>>::repr(schema)
            }
        }
    };

    (
        delegate from ($from_ty:ty) to ($to_ty:ty) for type ($value_ty:ty)
        including schema
        $(with where clause ($($where_clause:tt)*))?
        $(with generics ($($value_generics:tt)*))?
    ) => {
        $crate::delegate_encoding!(
            delegate schema from ($from_ty) to ($to_ty) for type ($value_ty)
            $(with where clause ($($where_clause)*))?
            $(with generics ($($value_generics)*))?
        );
        $crate::delegate_encoding!(
            delegate from ($from_ty) to ($to_ty) for type ($value_ty)
            $(with where clause ($($where_clause)*))?
            $(with generics ($($value_generics)*))?
        );
    };

    (
        delegate from ($from_ty:ty) to ($to_ty:ty) for type ($value_ty:ty)
        $(with where clause ($($where_clause:tt)*))?
        $(with generics ($($value_generics:tt)*))?
    ) => {
        impl$(<$($value_generics)*>)? $crate::encoding::Encoder<$from_ty, $value_ty> for ()
        where
            (): $crate::encoding::Encoder<$to_ty, $value_ty>,
            $($($where_clause)*)?
        {
            #[inline(always)]
            fn encode<B: $crate::bytes::BufMut + ?Sized>(
                tag: u32,
                value: &$value_ty,
                buf: &mut B,
                tw: &mut $crate::encoding::TagWriter,
            ) {
                <() as $crate::encoding::Encoder::<$to_ty, _>>::encode(tag, value, buf, tw)
            }

            #[inline(always)]
            fn prepend_encode<B: $crate::buf::ReverseBuf + ?Sized>(
                tag: u32,
                value: &$value_ty,
                buf: &mut B,
                tw: &mut $crate::encoding::TagRevWriter,
            ) {
                <() as $crate::encoding::Encoder::<$to_ty, _>>::prepend_encode(tag, value, buf, tw)
            }

            #[inline(always)]
            fn encoded_len(
                tag: u32,
                value: &$value_ty,
                tm: &mut impl $crate::encoding::TagMeasurer,
            ) -> usize {
                <() as $crate::encoding::Encoder::<$to_ty, _>>::encoded_len(tag, value, tm)
            }
        }

        impl$(<$($value_generics)*>)? $crate::encoding::Decoder<$from_ty, $value_ty> for ()
        where
            (): $crate::encoding::Decoder<$to_ty, $value_ty>,
            $($($where_clause)*)?
        {
            #[inline(always)]
            fn decode<B: $crate::bytes::Buf + ?Sized>(
                wire_type: $crate::encoding::WireType,
                value: &mut $value_ty,
                buf: $crate::encoding::Capped<B>,
                ctx: $crate::encoding::DecodeContext,
            ) -> Result<(), $crate::DecodeError> {
                <() as $crate::encoding::Decoder::<$to_ty, _>>::decode(
                    wire_type,
                    value,
                    buf,
                    ctx,
                )
            }
        }

        impl<'__a$(, $($value_generics)*)?>
        $crate::encoding::BorrowDecoder<'__a, $from_ty, $value_ty> for ()
        where
            (): $crate::encoding::BorrowDecoder<'__a, $to_ty, $value_ty>,
            $($($where_clause)*)?
        {
            #[inline(always)]
            fn borrow_decode(
                wire_type: $crate::encoding::WireType,
                value: &mut $value_ty,
                buf: $crate::encoding::Capped<&'__a [u8]>,
                ctx: $crate::encoding::DecodeContext,
            ) -> Result<(), $crate::DecodeError> {
                <() as $crate::encoding::BorrowDecoder::<$to_ty, _>>::borrow_decode(
                    wire_type,
                    value,
                    buf,
                    ctx,
                )
            }
        }
    };

    (
        delegate from ($from_ty:ty) to ($to_ty:ty) for type ($value_ty:ty)
        including distinguished
        including schema
        $(with where clause ($($where_clause:tt)*))?
        $(with generics ($($value_generics:tt)*))?
    ) => {
        $crate::delegate_encoding!(
            delegate schema from ($from_ty) to ($to_ty) for type ($value_ty)
            $(with where clause ($($where_clause)*))?
            $(with generics ($($value_generics)*))?
        );
        $crate::delegate_encoding!(
            delegate from ($from_ty) to ($to_ty) for type ($value_ty)
            including distinguished
            $(with where clause ($($where_clause)*))?
            $(with generics ($($value_generics)*))?
        );
    };

    (
        delegate from ($from_ty:ty) to ($to_ty:ty) for type ($value_ty:ty)
        including distinguished
        $(with where clause ($($where_clause:tt)*))?
        $(with generics ($($value_generics:tt)*))?
    ) => {
        $crate::delegate_encoding!(
            delegate from ($from_ty) to ($to_ty) for type ($value_ty)
            $(with where clause ($($where_clause)*))?
            $(with generics ($($value_generics)*))?
        );

        impl$(<$($value_generics)*>)?
        $crate::encoding::DistinguishedDecoder<$from_ty, $value_ty> for ()
        where
            (): $crate::encoding::DistinguishedDecoder<$to_ty, $value_ty>
                + $crate::encoding::Encoder<$to_ty, $value_ty>,
            $($($where_clause)*)?
        {
            #[inline(always)]
            fn decode_distinguished<B: $crate::bytes::Buf + ?Sized>(
                wire_type: $crate::encoding::WireType,
                value: &mut $value_ty,
                buf: $crate::encoding::Capped<B>,
                ctx: $crate::encoding::RestrictedDecodeContext,
            ) -> Result<$crate::Canonicity, $crate::DecodeError> {
                <() as $crate::encoding::DistinguishedDecoder::<$to_ty, _>>::decode_distinguished(
                    wire_type,
                    value,
                    buf,
                    ctx,
                )
            }
        }

        impl<'__a$(, $($value_generics)*)?>
        $crate::encoding::DistinguishedBorrowDecoder<'__a, $from_ty, $value_ty> for ()
        where
            (): $crate::encoding::DistinguishedBorrowDecoder<'__a, $to_ty, $value_ty>
                + $crate::encoding::Encoder<$to_ty, $value_ty>,
            $($($where_clause)*)?
        {
            #[inline(always)]
            fn borrow_decode_distinguished(
                wire_type: $crate::encoding::WireType,
                value: &mut $value_ty,
                buf: $crate::encoding::Capped<&'__a [u8]>,
                ctx: $crate::encoding::RestrictedDecodeContext,
            ) -> Result<$crate::Canonicity, $crate::DecodeError> {
                <() as $crate::encoding::DistinguishedBorrowDecoder::<$to_ty, _>>::
                    borrow_decode_distinguished
                (
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
/// value encoding and decoding in the `General` encodings. This continues to preclude any other
/// blanket trait delegation.
#[macro_export]
macro_rules! delegate_value_encoding {
    (
        delegate schema from ($from_ty:ty) to ($to_ty:ty) for type ($value_ty:ty)
        $(with where clause ($($where_clause:tt)+))?
        $(with generics ($($value_generics:tt)*))?
    ) => {
        impl$(<$($value_generics)*>)? $crate::encoding::schema::ValueRepr<$from_ty, $value_ty>
        for ()
            where
            (): $crate::encoding::schema::ValueRepr<$to_ty, $value_ty>,
            $($($where_clause)*)?
            {
                fn repr(
                    schema: &$crate::encoding::schema::Schema
                ) -> $crate::alloc::boxed::Box<dyn ::core::fmt::Display> {
                    <() as $crate::encoding::schema::ValueRepr<$to_ty, $value_ty>>::repr(schema)
                }
            }
    };

    (
        delegate from ($from_ty:ty) to ($to_ty:ty) for type ($value_ty:ty)
        including schema
        $(with where clause ($($where_clause:tt)+))?
        $(with generics ($($value_generics:tt)*))?
    ) => {
        $crate::delegate_value_encoding!(
            delegate from ($from_ty) to ($to_ty) for type ($value_ty)
            $(with where clause ($($where_clause)*))?
            $(with generics ($($value_generics)*))?
        );
        $crate::delegate_value_encoding!(
            delegate schema from ($from_ty) to ($to_ty) for type ($value_ty)
            $(with where clause ($($where_clause)*))?
            $(with generics ($($value_generics)*))?
        );
    };

    (
        delegate from ($from_ty:ty) to ($to_ty:ty) for type ($value_ty:ty)
        $(with where clause ($($where_clause:tt)+))?
        $(with generics ($($value_generics:tt)*))?
    ) => {
        impl$(<$($value_generics)*>)? $crate::encoding::Wiretyped<$from_ty, $value_ty> for ()
        where
            (): $crate::encoding::Wiretyped<$to_ty, $value_ty>,
            $($($where_clause)+ ,)?
        {
            const WIRE_TYPE: $crate::encoding::WireType =
                <() as $crate::encoding::Wiretyped<$to_ty, $value_ty>>::WIRE_TYPE;
        }

        impl$(<$($value_generics)*>)? $crate::encoding::ValueEncoder<$from_ty, $value_ty> for ()
        where
            (): $crate::encoding::ValueEncoder<$to_ty, $value_ty>,
            $($($where_clause)+ ,)?
        {
            #[inline(always)]
            fn encode_value<__B: $crate::bytes::BufMut + ?Sized>(value: &$value_ty, buf: &mut __B) {
                <() as $crate::encoding::ValueEncoder::<$to_ty, _>>::encode_value(value, buf)
            }

            #[inline(always)]
            fn prepend_value<__B: $crate::buf::ReverseBuf + ?Sized>(
                value: &$value_ty,
                buf: &mut __B,
            ) {
                <() as $crate::encoding::ValueEncoder::<$to_ty, _>>::prepend_value(value, buf)
            }

            #[inline(always)]
            fn value_encoded_len(value: &$value_ty) -> usize {
                <() as $crate::encoding::ValueEncoder::<$to_ty, _>>::value_encoded_len(value)
            }

            #[inline(always)]
            fn many_values_encoded_len<__I>(values: __I) -> usize
            where
                __I: ExactSizeIterator,
                __I::Item: core::ops::Deref<Target = $value_ty>,
            {
                <() as $crate::encoding::ValueEncoder::<$to_ty, _>>::many_values_encoded_len(
                    values,
                )
            }
        }

        impl$(<$($value_generics)*>)? $crate::encoding::ValueDecoder<$from_ty, $value_ty> for ()
        where
            (): $crate::encoding::ValueDecoder<$to_ty, $value_ty>,
            $($($where_clause)+ ,)?
        {
            #[inline(always)]
            fn decode_value<__B: $crate::bytes::Buf + ?Sized>(
                value: &mut $value_ty,
                buf: $crate::encoding::Capped<__B>,
                ctx: $crate::encoding::DecodeContext,
            ) -> Result<(), $crate::DecodeError> {
                <() as $crate::encoding::ValueDecoder::<$to_ty, _>>::decode_value(value, buf, ctx)
            }
        }

        impl<'__a $(, $($value_generics)*)?>
        $crate::encoding::ValueBorrowDecoder<'__a, $from_ty, $value_ty> for ()
        where
            (): $crate::encoding::ValueBorrowDecoder<'__a, $to_ty, $value_ty>,
            $($($where_clause)+ ,)?
        {
            #[inline(always)]
            fn borrow_decode_value(
                value: &mut $value_ty,
                buf: $crate::encoding::Capped<&'__a [u8]>,
                ctx: $crate::encoding::DecodeContext,
            ) -> Result<(), $crate::DecodeError> {
                <() as $crate::encoding::ValueBorrowDecoder::<$to_ty, _>>::borrow_decode_value(
                    value,
                    buf,
                    ctx,
                )
            }
        }
    };

    (
        delegate from ($from_ty:ty) to ($to_ty:ty) for type ($value_ty:ty)
        including distinguished
        including schema
        $(with where clause for relaxed ($($relaxed_where:tt)+))?
        $(with where clause for distinguished ($($distinguished_where:tt)+))?
        $(with generics ($($value_generics:tt)*))?
    ) => {
        $crate::delegate_value_encoding!(
            delegate schema from ($from_ty) to ($to_ty) for type ($value_ty)
            $(with where clause ($($relaxed_where)*))?
            $(with generics ($($value_generics)*))?
        );
        $crate::delegate_value_encoding!(
            delegate from ($from_ty) to ($to_ty) for type ($value_ty)
            including distinguished
            $(with where clause for relaxed ($($relaxed_where)+))?
            $(with where clause for distinguished ($($distinguished_where)+))?
            $(with generics ($($value_generics)*))?
        );
    };

    (
        delegate from ($from_ty:ty) to ($to_ty:ty) for type ($value_ty:ty) including distinguished
        $(with where clause for relaxed ($($relaxed_where:tt)+))?
        $(with where clause for distinguished ($($distinguished_where:tt)+))?
        $(with generics ($($value_generics:tt)*))?
    ) => {
        $crate::delegate_value_encoding!(
            delegate from ($from_ty) to ($to_ty) for type ($value_ty)
            $(with where clause ($($relaxed_where)+))?
            $(with generics ($($value_generics)*))?
        );

        impl$(<$($value_generics)*>)?
        $crate::encoding::DistinguishedValueDecoder<$from_ty, $value_ty> for ()
        where
            (): $crate::encoding::DistinguishedValueDecoder<$to_ty, $value_ty>,
            $($($relaxed_where)+ ,)?
            $($($distinguished_where)+ ,)?
        {
            const CHECKS_EMPTY: bool = <
                () as $crate::encoding::DistinguishedValueDecoder<$to_ty, $value_ty>
            >::CHECKS_EMPTY;

            #[inline(always)]
            fn decode_value_distinguished<const ALLOW_EMPTY: bool>(
                value: &mut $value_ty,
                buf: $crate::encoding::Capped<impl $crate::bytes::Buf + ?Sized>,
                ctx: $crate::encoding::RestrictedDecodeContext,
            ) -> Result<$crate::Canonicity, $crate::DecodeError> {
                <() as $crate::encoding::DistinguishedValueDecoder::<$to_ty, _>>::
                    decode_value_distinguished::<ALLOW_EMPTY>
                (
                    value,
                    buf,
                    ctx,
                )
            }
        }

        impl<'__a $(, $($value_generics)*)?>
        $crate::encoding::DistinguishedValueBorrowDecoder<'__a, $from_ty, $value_ty> for ()
        where
            (): $crate::encoding::DistinguishedValueBorrowDecoder<'__a, $to_ty, $value_ty>,
            $($($relaxed_where)+ ,)?
            $($($distinguished_where)+ ,)?
        {
            const CHECKS_EMPTY: bool = <
                () as $crate::encoding::DistinguishedValueBorrowDecoder<'__a, $to_ty, $value_ty>
            >::CHECKS_EMPTY;

            #[inline(always)]
            fn borrow_decode_value_distinguished<const ALLOW_EMPTY: bool>(
                value: &mut $value_ty,
                buf: $crate::encoding::Capped<&'__a [u8]>,
                ctx: $crate::encoding::RestrictedDecodeContext,
            ) -> Result<$crate::Canonicity, $crate::DecodeError> {
                <() as $crate::encoding::DistinguishedValueBorrowDecoder::<$to_ty, _>>::
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
        $crate::encoding::ValueBorrowDecoder<'__a, $encoding, $ty> for ()
        where
            (): $crate::encoding::ValueDecoder<$encoding, $ty>,
        {
            #[inline]
            fn borrow_decode_value(
                value: &mut $ty,
                buf: $crate::encoding::Capped<&'__a [u8]>,
                ctx: $crate::encoding::DecodeContext,
            ) -> Result<(), $crate::DecodeError> {
                <() as $crate::encoding::ValueDecoder::<$encoding, _>>::decode_value(
                    value,
                    buf,
                    ctx,
                )
            }
        }
    };

    (
        encoding ($encoding:ty) borrows type ($ty:ty) as owned including distinguished
        $(with where clause ($($where_clause:tt)+))?
        $(with generics ($($impl_generics:tt)*))?
    ) => {
        $crate::delegate_value_encoding!(
            encoding ($encoding) borrows type ($ty) as owned
            $(with where clause ($($where_clause)*))?
            $(with generics ($($impl_generics)*))?
        );

        impl<'__a, $($($impl_generics)*)?>
        $crate::encoding::DistinguishedValueBorrowDecoder<'__a, $encoding, $ty> for ()
        where
            (): $crate::encoding::DistinguishedValueDecoder<$encoding, $ty>,
        {
            const CHECKS_EMPTY: bool =
                <() as $crate::encoding::DistinguishedValueDecoder<$encoding, $ty>>::CHECKS_EMPTY;

            #[inline]
            fn borrow_decode_value_distinguished<const ALLOW_EMPTY: bool>(
                value: &mut $ty,
                buf: $crate::encoding::Capped<&'__a [u8]>,
                ctx: $crate::encoding::RestrictedDecodeContext,
            ) -> Result<$crate::Canonicity, $crate::DecodeError> {
                <() as $crate::encoding::DistinguishedValueDecoder::<$encoding, _>>::
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

/// Shorthand call for delegating encoding using the `Proxiable` traits and the `Proxied` encoding
/// that uses it.
#[macro_export]
macro_rules! delegate_proxied_encoding {
    (
        use encoding ($to:ty)
        to encode proxied type ($value_ty:ty)
        $(using proxy tag ($proxy_tag:ty))?
        with encoding ($from:ty)
        including schema
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
        including schema
        $(with where clause for relaxed ($($relaxed_where:tt)*))?
        $(with where clause for distinguished ($($distinguished_where:tt)*))?
        $(with generics ($($impl_generics:tt)*))?
    ) => {
        $crate::delegate_value_encoding!(
            delegate from ($from)
            to ($crate::encoding::Proxied<$to $(, $proxy_tag)?>)
            for type ($value_ty)
            including distinguished
            including schema
            $(with where clause for relaxed ($($relaxed_where)*))?
            $(with where clause for distinguished ($($distinguished_where)*))?
            $(with generics ($($impl_generics)*))?
        );
    };

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
        including schema
        $(with where clause ($($where_clause:tt)+))?
        $(with generics ($($impl_generics:tt)*))?
    ) => {
        $crate::delegate_value_encoding!(
            delegate from ($crate::encoding::GeneralGeneric<__G>)
            to ($crate::encoding::Proxied<$to $(, $proxy_tag)?>)
            for type ($value_ty)
            including schema
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
        including schema
        $(with where clause for relaxed ($($relaxed_where:tt)*))?
        $(with where clause for distinguished ($($distinguished_where:tt)*))?
        $(with generics ($($impl_generics:tt)*))?
    ) => {
        $crate::delegate_value_encoding!(
            delegate from ($crate::encoding::GeneralGeneric<__G>)
            to ($crate::encoding::Proxied<$to $(, $proxy_tag)?>)
            for type ($value_ty)
            including distinguished
            including schema
            $(with where clause for relaxed ($($relaxed_where)*))?
            $(with where clause for distinguished ($($distinguished_where)*))?
            with generics (const __G: u8, $($($impl_generics)*)?)
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

/// Generates implementations of `ForOverwrite` and `EmptyState` for the given encoding that always
/// defer to the base implementation of the trait.
///
/// This is suitable for any encoding that won't need to implement those traits for any un-owned
/// types. To implement encodings for types that are not owned for your crate (such as types in
/// `std` or a third-party crate that `bilrost` doesn't already cover) your implementation will be
/// required to "own" the trait that is being implemented. This means that for your custom encoding
/// type `MyEncoding`, the traits `ForOverwrite<MyEncoding>` and `EmptyState<MyEncoding>` can still
/// be implemented inside your crate as long as you don't also use this macro.
#[macro_export]
macro_rules! encoding_uses_base_empty_state {
    (
        $encoding:ty
        $(, with generics ($($impl_generics:tt)*))?
        $(, with where clause ($($where_clause:tt)*))?
    ) => {
        impl<$($($impl_generics)*,)? __T> $crate::encoding::ForOverwrite<$encoding, __T> for ()
        where
            (): $crate::encoding::ForOverwrite<(), __T>,
            $($($where_clause)*)?
        {
            #[inline(always)]
            fn for_overwrite() -> __T {
                <() as $crate::encoding::ForOverwrite::<(), _>>::for_overwrite()
            }
        }

        impl<$($($impl_generics)*,)? __T> $crate::encoding::EmptyState<$encoding, __T> for ()
        where
            (): $crate::encoding::EmptyState<(), __T>,
            $($($where_clause)*)?
        {
            #[inline(always)]
            fn empty() -> __T {
                <() as $crate::encoding::EmptyState::<(), _>>::empty()
            }

            #[inline(always)]
            fn is_empty(__val: &__T) -> bool {
                <() as $crate::encoding::EmptyState::<(), _>>::is_empty(__val)
            }

            #[inline(always)]
            fn clear(__val: &mut __T) {
                <() as $crate::encoding::EmptyState::<(), _>>::clear(__val);
            }
        }
    }
}
pub(crate) use encoding_uses_base_empty_state;

/// Adds basic empty trait implementations that are typically universal. Currently this includes
/// `Option<T>` (which is always empty when `None`, regardless of the type of `T`), and arrays
/// `[T; N]` which are implemented whenever `T` is implemented, and use the implementation for `T`
/// for each value in the array. For example, an array of values of type `T` is only empty when each
/// value in the array is empty, and clearing the array value will clear each of the values inside.
#[macro_export]
macro_rules! implement_core_empty_state_rules {
    (
        $encoding:ty
        $(, with generics ($($impl_generics:tt)*))?
        $(, with where clause ($($where_clause:tt)*))?
    ) => {
        impl<$($($impl_generics)*,)? __T>
        $crate::encoding::ForOverwrite<$encoding, ::core::option::Option<__T>> for ()
        $(where $($where_clause)*)?
        {
            #[inline(always)]
            fn for_overwrite() -> ::core::option::Option<__T> {
                ::core::option::Option::None
            }
        }

        impl<$($($impl_generics)*,)? __T>
        $crate::encoding::EmptyState<$encoding, ::core::option::Option<__T>> for ()
        $(where $($where_clause)*)?
        {
            #[inline(always)]
            fn is_empty(__val: &::core::option::Option<__T>) -> bool {
                ::core::option::Option::is_none(__val)
            }

            #[inline(always)]
            fn clear(__val: &mut ::core::option::Option<__T>) {
                *__val = ::core::option::Option::None;
            }
        }

        impl<$($($impl_generics)*,)? __T, const __N: usize>
        $crate::encoding::ForOverwrite<$encoding, [__T; __N]> for ()
        where
            (): $crate::encoding::ForOverwrite<$encoding, __T>,
            $($($where_clause)*)?
        {
            #[inline]
            fn for_overwrite() -> [__T; __N] {
                ::core::array::from_fn(|_| {
                    <() as $crate::encoding::ForOverwrite<$encoding, __T>>::for_overwrite()
                })
            }
        }

        impl<$($($impl_generics)*,)? __T, const __N: usize>
        $crate::encoding::EmptyState<$encoding, [__T; __N]> for ()
        where
            (): $crate::encoding::EmptyState<$encoding, __T>,
            $($($where_clause)*)?
        {
            #[inline]
            fn empty() -> [__T; __N]
            where
                [__T; __N]: Sized,
            {
                ::core::array::from_fn(|_| <() as $crate::encoding::EmptyState<$encoding, __T>>::empty())
            }

            #[inline]
            fn is_empty(val: &[__T; __N]) -> bool {
                val.iter().all(<() as $crate::encoding::EmptyState<$encoding, __T>>::is_empty)
            }

            #[inline]
            fn clear(val: &mut [__T; __N]) {
                for v in val {
                    <() as $crate::encoding::EmptyState<$encoding, __T>>::clear(v);
                }
            }
        }
    }
}
pub(crate) use implement_core_empty_state_rules;

/// Most kinds of encodings want to act as field decoders for bare values in any situation where
/// they also implement value decoding. Only a couple encodings want to do anything fancy, like
/// accepting alternate wire-types in relaxed mode; the rest want to use this to blanket those
/// definitions.
#[doc(hidden)]
#[macro_export]
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
        $crate::encoding::$relaxed<$($lifetime,)? $encoding, T> for ()
        where
            (): $crate::encoding::EmptyState<$encoding, T>
                + $crate::encoding::$relaxed_value<$($lifetime,)? $encoding, T>,
            $($($where_clause)*)?
        {
            #[inline(always)]
            fn $relaxed_method $($($buf_generic)*)? (
                wire_type: $crate::encoding::WireType,
                value: &mut T,
                buf: $crate::encoding::Capped<$buf_ty>,
                ctx: $crate::encoding::DecodeContext,
            ) -> ::core::result::Result<(), $crate::DecodeError> {
                <() as $crate::encoding::$relaxed_field<$encoding, _>>::$relaxed_field_method(
                    wire_type, value, buf, ctx)
            }
        }

        /// Canonical encoding for plain values forbids encoding empty values. This includes
        /// directly-nested message types, which are not emitted when all their fields are default.
        /// If an empty value is decoded it is considered fully non-canonical.
        impl<$($lifetime,)? T $(, $($generics)*)?>
        $crate::encoding::$distinguished <$($lifetime,)? $encoding, T> for ()
        where
            T: ::core::cmp::Eq,
            (): $crate::encoding::EmptyState<$encoding, T>
                + $crate::encoding::$distinguished_value<$($lifetime,)? $encoding, T>,
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
                let mut canon = <() as $crate::encoding::$distinguished_field<$encoding, _>>::
                    $distinguished_field_method::<false>
                (
                    wire_type,
                    value,
                    buf,
                    ctx.clone(),
                )?;
                if !<() as $crate::encoding::$distinguished_value<$encoding, T>>::CHECKS_EMPTY
                    && <() as $crate::encoding::EmptyState<$encoding, _>>::is_empty(value)
                {
                    canon.update(ctx.check($crate::Canonicity::NotCanonical)?);
                }
                Ok(canon)
            }
        }
    };
}

#[macro_export]
macro_rules! encoding_implemented_via_value_encoding {
    (
        $encoding:ty
        $(, with where clause ($($where_clause:tt)*))?
        $(, with generics ($($generics:tt)*) $(,)?)?
    ) => {
        impl<T $(, $($generics)*)?> $crate::encoding::schema::FieldRepr<$encoding, T> for ()
        where
            (): $crate::encoding::Encoder<$encoding, T>,
            (): $crate::encoding::schema::ValueRepr<$encoding, T>,
            $($($where_clause)*)?
        {
            fn repr(
                schema: &$crate::encoding::schema::Schema
            ) -> $crate::alloc::boxed::Box<dyn ::core::fmt::Display> {
                <() as $crate::encoding::schema::ValueRepr<$encoding, T>>::repr(schema)
            }
        }

        /// Encodes plain values only when they are non-empty.
        impl<T $(, $($generics)*)?> $crate::encoding::Encoder<$encoding, T> for ()
        where
            (): $crate::encoding::EmptyState<$encoding, T>
                + $crate::encoding::ValueEncoder<$encoding, T>,
            $($($where_clause)*)?
        {
            #[inline(always)]
            fn encode<B: $crate::bytes::BufMut + ?Sized>(
                tag: u32,
                value: &T,
                buf: &mut B,
                tw: &mut $crate::encoding::TagWriter,
            ) {
                if !<() as $crate::encoding::EmptyState<$encoding, T>>::is_empty(value) {
                    <() as $crate::encoding::FieldEncoder<$encoding, T>>::encode_field(
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
                if !<() as $crate::encoding::EmptyState<$encoding, T>>::is_empty(value) {
                    <() as $crate::encoding::FieldEncoder<$encoding, T>>::prepend_field(
                        tag, value, buf, tw);
                }
            }

            #[inline(always)]
            fn encoded_len(
                tag: u32,
                value: &T,
                tm: &mut impl $crate::encoding::TagMeasurer,
            ) -> usize {
                if !<() as $crate::encoding::EmptyState<$encoding, T>>::is_empty(value) {
                    <() as $crate::encoding::FieldEncoder<$encoding, T>>::field_encoded_len(
                        tag, value, tm)
                } else {
                    0
                }
            }
        }

        $crate::__invoke!(
            $crate::__impl_decoder_where_value_decoder,
            owned,
            encoding: $encoding,
            $(with where clause ($($where_clause)*),)?
            $(with generics ($($generics)*),)?
        );
        $crate::__invoke!(
            $crate::__impl_decoder_where_value_decoder,
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
            use alloc::boxed::Box;
            use bytes::{Buf, BufMut};
            use core::fmt::Display;
            use $crate::buf::ReverseBuf;
            use $crate::encoding::{
                Capped, DecodeContext, DistinguishedValueBorrowDecoder, DistinguishedValueDecoder,
                ForOverwrite, RestrictedDecodeContext, ValueBorrowDecoder, ValueEncoder, WireType,
                Wiretyped,
            };
            use $crate::encoding::schema::{Schema, ValueRepr};
            use $crate::{Canonicity, DecodeError};

            impl$(<$($generic)*>)? Wiretyped<$E, Cow<'_, $T>> for () {
                const WIRE_TYPE: WireType = {
                    let b = <() as Wiretyped<$E, &$T>>::WIRE_TYPE;
                    let o = <() as Wiretyped<$E, $Owned>>::WIRE_TYPE;
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

            impl$(<$($generic)*>)? ValueRepr<$E, Cow<'_, $T>> for ()
            where
                (): ValueRepr<$E, $Owned>,
            {
                fn repr(schema: &Schema) -> Box<dyn Display> {
                    <() as ValueRepr<$E, $Owned>>::repr(schema)
                }
            }

            impl$(<$($generic)*>)? ValueEncoder<$E, Cow<'_, $T>> for () {
                #[inline]
                fn encode_value<B: BufMut + ?Sized>(value: &Cow<$T>, buf: &mut B) {
                    <() as ValueEncoder<$E, _>>::encode_value(&&**value, buf)
                }

                #[inline]
                fn prepend_value<B: ReverseBuf + ?Sized>(value: &Cow<$T>, buf: &mut B) {
                    <() as ValueEncoder<$E, _>>::prepend_value(&&**value, buf)
                }

                #[inline]
                fn value_encoded_len(value: &Cow<$T>) -> usize {
                    <() as ValueEncoder<$E, _>>::value_encoded_len(&&**value)
                }
            }

            impl$(<$($generic)*>)? ValueDecoder<$E, Cow<'_, $T>> for () {
                #[inline]
                fn decode_value<B: Buf + ?Sized>(
                    value: &mut Cow<$T>,
                    buf: Capped<B>,
                    ctx: DecodeContext,
                ) -> Result<(), DecodeError> {
                    <() as ValueDecoder<$E, _>>::decode_value(value.to_mut(), buf, ctx)
                }
            }

            impl$(<$($generic)*>)? DistinguishedValueDecoder<$E, Cow<'_, $T>> for () {
                const CHECKS_EMPTY: bool =
                    <() as DistinguishedValueDecoder<$E, $Owned>>::CHECKS_EMPTY;

                #[inline]
                fn decode_value_distinguished<const ALLOW_EMPTY: bool>(
                    value: &mut Cow<$T>,
                    buf: Capped<impl Buf + ?Sized>,
                    ctx: RestrictedDecodeContext,
                ) -> Result<Canonicity, DecodeError> {
                    <() as DistinguishedValueDecoder<$E, _>>::
                        decode_value_distinguished::<ALLOW_EMPTY>
                    (
                        value.to_mut(),
                        buf,
                        ctx,
                    )
                }
            }

            impl<'a $(, $($generic)*)?> ValueBorrowDecoder<'a, $E, Cow<'a, $T>> for () {
                #[inline]
                fn borrow_decode_value(
                    value: &mut Cow<'a, $T>,
                    buf: Capped<&'a [u8]>,
                    ctx: DecodeContext,
                ) -> Result<(), DecodeError> {
                    let mut s = <() as ForOverwrite<$E, &$T>>::for_overwrite();
                    <() as ValueBorrowDecoder<$E, _>>::borrow_decode_value(&mut s, buf, ctx)?;
                    *value = Cow::Borrowed(s);
                    Ok(())
                }
            }

            impl<'a $(, $($generic)*)?>
            DistinguishedValueBorrowDecoder<'a, $E, Cow<'a, $T>> for () {
                const CHECKS_EMPTY: bool =
                    <() as DistinguishedValueBorrowDecoder<'a, $E, &$T>>::CHECKS_EMPTY;

                #[inline]
                fn borrow_decode_value_distinguished<const ALLOW_EMPTY: bool>(
                    value: &mut Cow<'a, $T>,
                    buf: Capped<&'a [u8]>,
                    ctx: RestrictedDecodeContext,
                ) -> Result<Canonicity, DecodeError> {
                    let mut s = <() as ForOverwrite<$E, &$T>>::for_overwrite();
                    let canon = <() as DistinguishedValueBorrowDecoder<$E, _>>::
                        borrow_decode_value_distinguished::<ALLOW_EMPTY>
                    (
                        &mut s,
                        buf,
                        ctx,
                    )?;
                    *value = Cow::Borrowed(s);
                    Ok(canon)
                }
            }
        };
    };
}
pub(crate) use impl_cow_value_encoding;
