use crate::buf::ReverseBuf;
use crate::encoding::schema::{Schema, ValueRepr};
use crate::encoding::{
    const_varint, delegate_encoding, delegate_value_encoding, encode_varint, encoded_len_varint,
    encoding_implemented_via_value_encoding, encoding_uses_base_empty_state,
    impl_cow_value_encoding, prepend_varint, Canonicity, Capped, DecodeContext, DecodeError,
    DistinguishedValueBorrowDecoder, DistinguishedValueDecoder, RestrictedDecodeContext,
    ValueBorrowDecoder, ValueDecoder, ValueEncoder, WireType, Wiretyped,
};
use crate::DecodeErrorKind::InvalidValue;
use alloc::borrow::Cow;
use alloc::boxed::Box;
use alloc::format;
use alloc::vec::Vec;
use bytes::{Buf, BufMut};
use core::fmt::Display;
use core::ops::Deref;

/// `PlainBytes` implements encoding for blob values directly into `Vec<u8>`, and provides the base
/// implementation for that functionality. `Vec<u8>` cannot generically dispatch to `General`'s
/// encoding, since `General` already generically implements encoding for other kinds of `Vec`, but
/// this encoder can be used instead if it's desirable to have a value whose type is exactly
/// `Vec<u8>`.
pub struct PlainBytes;

encoding_uses_base_empty_state!(PlainBytes);
encoding_implemented_via_value_encoding!(PlainBytes);

impl Wiretyped<PlainBytes, &[u8]> for () {
    const WIRE_TYPE: WireType = WireType::LengthDelimited;
}

impl ValueRepr<PlainBytes, &[u8]> for () {
    fn repr(_: &Schema) -> Box<dyn Display> {
        Box::new("delimited bytes")
    }
}

impl ValueEncoder<PlainBytes, &[u8]> for () {
    #[inline]
    fn encode_value<B: BufMut + ?Sized>(value: &&[u8], buf: &mut B) {
        encode_varint(value.len() as u64, buf);
        buf.put_slice(value);
    }

    #[inline]
    fn prepend_value<B: ReverseBuf + ?Sized>(value: &&[u8], buf: &mut B) {
        buf.prepend_slice(value);
        prepend_varint(value.len() as u64, buf);
    }

    #[inline]
    fn value_encoded_len(value: &&[u8]) -> usize {
        encoded_len_varint(value.len() as u64) + value.len()
    }
}

impl<'a> ValueBorrowDecoder<'a, PlainBytes, &'a [u8]> for () {
    #[inline]
    fn borrow_decode_value(
        value: &mut &'a [u8],
        mut buf: Capped<&'a [u8]>,
        _ctx: impl DecodeContext,
    ) -> Result<(), DecodeError> {
        *value = buf.take_borrowed_length_delimited()?;
        Ok(())
    }
}

impl<'a> DistinguishedValueBorrowDecoder<'a, PlainBytes, &'a [u8]> for () {
    const CHECKS_EMPTY: bool = false;

    #[inline]
    fn borrow_decode_value_distinguished<const ALLOW_EMPTY: bool>(
        value: &mut &'a [u8],
        mut buf: Capped<&'a [u8]>,
        _ctx: impl RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError> {
        *value = buf.take_borrowed_length_delimited()?;
        Ok(Canonicity::Canonical)
    }
}

#[cfg(test)]
mod ref_bytes {
    crate::encoding::test::check_borrowable!(borrowed: [u8], encoding: crate::encoding::PlainBytes);
}

impl Wiretyped<PlainBytes, Vec<u8>> for () {
    const WIRE_TYPE: WireType = WireType::LengthDelimited;
}

impl ValueRepr<PlainBytes, Vec<u8>> for () {
    fn repr(schema: &Schema) -> Box<dyn Display> {
        <() as ValueRepr<PlainBytes, &[u8]>>::repr(schema)
    }
}

impl ValueEncoder<PlainBytes, Vec<u8>> for () {
    #[inline]
    fn encode_value<B: BufMut + ?Sized>(value: &Vec<u8>, buf: &mut B) {
        <() as ValueEncoder<PlainBytes, _>>::encode_value(&value.as_slice(), buf)
    }

    #[inline]
    fn prepend_value<B: ReverseBuf + ?Sized>(value: &Vec<u8>, buf: &mut B) {
        <() as ValueEncoder<PlainBytes, _>>::prepend_value(&value.as_slice(), buf)
    }

    #[inline]
    fn value_encoded_len(value: &Vec<u8>) -> usize {
        <() as ValueEncoder<PlainBytes, _>>::value_encoded_len(&value.as_slice())
    }
}

impl ValueDecoder<PlainBytes, Vec<u8>> for () {
    fn decode_value<B: Buf + ?Sized>(
        value: &mut Vec<u8>,
        mut buf: Capped<B>,
        ctx: impl DecodeContext,
    ) -> Result<(), DecodeError> {
        let buf = buf.take_length_delimited()?;
        ctx.heap_used(buf.remaining_before_cap())?;
        value.clear();
        value.reserve(buf.remaining_before_cap());
        value.put(buf.take_all());
        Ok(())
    }
}

impl DistinguishedValueDecoder<PlainBytes, Vec<u8>> for () {
    const CHECKS_EMPTY: bool = false;

    fn decode_value_distinguished<const ALLOW_EMPTY: bool>(
        value: &mut Vec<u8>,
        buf: Capped<impl Buf + ?Sized>,
        ctx: impl RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError> {
        <() as ValueDecoder<PlainBytes, _>>::decode_value(value, buf, ctx.into_inner())?;
        Ok(Canonicity::Canonical)
    }
}

delegate_value_encoding!(
    encoding (PlainBytes) borrows type (Vec<u8>) as owned including distinguished
);

delegate_encoding!(
    delegate from (PlainBytes) to (crate::encoding::Unpacked<PlainBytes>)
    for type (Vec<Vec<u8>>)
    including distinguished
    including schema
);
delegate_encoding!(
    delegate from (PlainBytes) to (crate::encoding::Unpacked<PlainBytes>)
    for type (Vec<Cow<'a, [u8]>>)
    including distinguished
    including schema
    with generics ('a)
);
delegate_encoding!(
    delegate from (PlainBytes) to (crate::encoding::Unpacked<PlainBytes>)
    for type (Vec<&'a [u8]>)
    including distinguished
    including schema
    with generics ('a)
);
delegate_encoding!(
    delegate from (PlainBytes) to (crate::encoding::Unpacked<PlainBytes>)
    for type (Vec<&'a [u8; N]>)
    including distinguished
    including schema
    with generics ('a, const N: usize)
);

#[cfg(test)]
mod vec_u8 {
    use super::{PlainBytes, Vec};
    use crate::encoding::test::check_type_test;
    check_type_test!(PlainBytes, relaxed, Vec<u8>, WireType::LengthDelimited);
    check_type_test!(
        PlainBytes,
        distinguished,
        Vec<u8>,
        WireType::LengthDelimited
    );
}

impl_cow_value_encoding!(borrowed [u8], owned Vec<u8>, encoding PlainBytes);

#[cfg(test)]
mod cow_bytes {
    use super::{Cow, PlainBytes};
    use crate::encoding::test::check_type_test;
    check_type_test!(PlainBytes, relaxed, Cow<[u8]>, WireType::LengthDelimited);
    check_type_test!(
        PlainBytes,
        distinguished,
        Cow<[u8]>,
        WireType::LengthDelimited
    );
}

impl<const N: usize> Wiretyped<PlainBytes, [u8; N]> for () {
    const WIRE_TYPE: WireType = WireType::LengthDelimited;
}

impl<const N: usize> ValueRepr<PlainBytes, [u8; N]> for () {
    fn repr(_: &Schema) -> Box<dyn Display> {
        Box::new(format!("delimited bytes, exactly {N}"))
    }
}

impl<const N: usize> ValueEncoder<PlainBytes, [u8; N]> for () {
    #[inline]
    fn encode_value<B: BufMut + ?Sized>(value: &[u8; N], mut buf: &mut B) {
        buf.put_slice(&const_varint(N as u64));
        (&mut buf).put(value.as_slice())
    }

    #[inline]
    fn prepend_value<B: ReverseBuf + ?Sized>(value: &[u8; N], buf: &mut B) {
        buf.prepend_slice(value);
        buf.prepend_slice(&const_varint(N as u64))
    }

    #[inline]
    fn value_encoded_len(_value: &[u8; N]) -> usize {
        const_varint(N as u64).len() + N
    }

    #[inline]
    fn many_values_encoded_len<I>(values: I) -> usize
    where
        I: ExactSizeIterator,
        I::Item: Deref<Target = [u8; N]>,
    {
        values.len() * (const_varint(N as u64).len() + N)
    }
}

impl<const N: usize> ValueDecoder<PlainBytes, [u8; N]> for () {
    fn decode_value<B: Buf + ?Sized>(
        value: &mut [u8; N],
        mut buf: Capped<B>,
        _ctx: impl DecodeContext,
    ) -> Result<(), DecodeError> {
        let mut delimited = buf.take_length_delimited()?;
        if delimited.remaining_before_cap() != N {
            return Err(DecodeError::new(InvalidValue));
        }
        delimited.copy_to_slice(value.as_mut_slice());
        Ok(())
    }
}

impl<const N: usize> DistinguishedValueDecoder<PlainBytes, [u8; N]> for () {
    const CHECKS_EMPTY: bool = false;

    fn decode_value_distinguished<const ALLOW_EMPTY: bool>(
        value: &mut [u8; N],
        buf: Capped<impl Buf + ?Sized>,
        ctx: impl RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError> {
        <() as ValueDecoder<PlainBytes, _>>::decode_value(value, buf, ctx.into_inner())?;
        Ok(Canonicity::Canonical)
    }
}

delegate_value_encoding!(
    encoding (PlainBytes) borrows type ([u8; N]) as owned including distinguished
    with generics (const N: usize)
);

impl<const N: usize> Wiretyped<PlainBytes, &[u8; N]> for () {
    const WIRE_TYPE: WireType = WireType::LengthDelimited;
}

impl<const N: usize> ValueRepr<PlainBytes, &[u8; N]> for () {
    fn repr(schema: &Schema) -> Box<dyn Display> {
        <() as ValueRepr<PlainBytes, [u8; N]>>::repr(schema)
    }
}

impl<'a, const N: usize> ValueEncoder<PlainBytes, &'a [u8; N]> for () {
    #[inline]
    fn encode_value<B: BufMut + ?Sized>(value: &&'a [u8; N], buf: &mut B) {
        <() as ValueEncoder<PlainBytes, _>>::encode_value(&value.as_slice(), buf)
    }

    #[inline]
    fn prepend_value<B: ReverseBuf + ?Sized>(value: &&'a [u8; N], buf: &mut B) {
        <() as ValueEncoder<PlainBytes, _>>::prepend_value(&value.as_slice(), buf)
    }

    #[inline]
    fn value_encoded_len(value: &&'a [u8; N]) -> usize {
        <() as ValueEncoder<PlainBytes, _>>::value_encoded_len(&value.as_slice())
    }

    #[inline]
    fn many_values_encoded_len<I>(values: I) -> usize
    where
        I: ExactSizeIterator,
        I::Item: Deref<Target = &'a [u8; N]>,
    {
        values.len() * (const_varint(N as u64).len() + N)
    }
}

impl<'a, const N: usize> ValueBorrowDecoder<'a, PlainBytes, &'a [u8; N]> for () {
    #[inline]
    fn borrow_decode_value(
        value: &mut &'a [u8; N],
        mut buf: Capped<&'a [u8]>,
        _ctx: impl DecodeContext,
    ) -> Result<(), DecodeError> {
        *value = buf
            .take_borrowed_length_delimited()?
            .try_into()
            .map_err(|_| InvalidValue)?;
        Ok(())
    }
}

impl<'a, const N: usize> DistinguishedValueBorrowDecoder<'a, PlainBytes, &'a [u8; N]> for () {
    const CHECKS_EMPTY: bool = false;

    #[inline]
    fn borrow_decode_value_distinguished<const ALLOW_EMPTY: bool>(
        value: &mut &'a [u8; N],
        buf: Capped<&'a [u8]>,
        ctx: impl RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError> {
        <() as ValueBorrowDecoder<PlainBytes, _>>::borrow_decode_value(
            value,
            buf,
            ctx.into_inner(),
        )?;
        Ok(Canonicity::Canonical)
    }
}

#[cfg(test)]
mod ref_u8_array {
    crate::encoding::test::check_borrowable!(
        borrowed: [u8; 1],
        encoding: crate::encoding::PlainBytes,
        mod one_byte,
    );
    crate::encoding::test::check_borrowable!(
        borrowed: [u8; 10],
        encoding: crate::encoding::PlainBytes,
        mod ten_bytes,
    );
}

#[cfg(test)]
mod u8_array {
    mod length_0 {
        use crate::encoding::test::check_type_test;
        use crate::encoding::PlainBytes;
        check_type_test!(PlainBytes, relaxed, [u8; 0], WireType::LengthDelimited);
        check_type_test!(
            PlainBytes,
            distinguished,
            [u8; 0],
            WireType::LengthDelimited
        );
    }

    mod length_1 {
        use crate::encoding::test::check_type_test;
        use crate::encoding::PlainBytes;
        check_type_test!(PlainBytes, relaxed, [u8; 1], WireType::LengthDelimited);
        check_type_test!(
            PlainBytes,
            distinguished,
            [u8; 1],
            WireType::LengthDelimited
        );
    }

    mod length_8 {
        use crate::encoding::test::check_type_test;
        use crate::encoding::PlainBytes;
        check_type_test!(PlainBytes, relaxed, [u8; 8], WireType::LengthDelimited);
        check_type_test!(
            PlainBytes,
            distinguished,
            [u8; 8],
            WireType::LengthDelimited
        );
    }

    mod length_13 {
        use crate::encoding::test::check_type_test;
        use crate::encoding::PlainBytes;
        check_type_test!(PlainBytes, relaxed, [u8; 13], WireType::LengthDelimited);
        check_type_test!(
            PlainBytes,
            distinguished,
            [u8; 13],
            WireType::LengthDelimited
        );
    }
}

impl_cow_value_encoding!(
    borrowed [u8; N], owned [u8; N], encoding PlainBytes, with generic (const N: usize)
);

#[allow(unused_macros)]
macro_rules! plain_bytes_vec_impl {
    (
        $ty:ty,
        $value:ident, $buf:ident, $chunk:ident,
        $do_reserve:expr,
        $do_extend:expr
        $(, limit $limit:expr)?
        $(, with generics ($($generics:tt)*))?
    ) => {
        $crate::encoding::delegate_value_encoding!(
            encoding ($crate::encoding::PlainBytes)
            borrows type ($ty) as owned including distinguished
            $(with generics ($($generics)*))?
        );

        impl$(<$($generics)*>)?
        $crate::encoding::Wiretyped<$crate::encoding::PlainBytes, $ty> for () {
            const WIRE_TYPE: $crate::encoding::WireType =
                $crate::encoding::WireType::LengthDelimited;
        }

        impl$(<$($generics)*>)?
        $crate::encoding::schema::ValueRepr<$crate::encoding::PlainBytes, $ty> for () {
            fn repr(
                schema: &$crate::encoding::schema::Schema,
            ) -> $crate::alloc::boxed::Box<dyn ::core::fmt::Display> {
                let res = <() as $crate::encoding::schema::ValueRepr<
                    $crate::encoding::PlainBytes,
                    &[u8]
                >>::repr(schema);
                $(
                    let res = $crate::alloc::boxed::Box::new(
                        ::alloc::format!("{res}; at most {limit} bytes", limit = $limit)
                    );
                )?
                res
            }
        }

        impl$(<$($generics)*>)?
        $crate::encoding::ValueEncoder<$crate::encoding::PlainBytes, $ty> for () {
            fn encode_value<B: $crate::bytes::BufMut + ?Sized>(value: &$ty, buf: &mut B) {
                <() as $crate::encoding::ValueEncoder<
                    $crate::encoding::PlainBytes,
                    _
                >>::encode_value(&&**value, buf)
            }

            fn prepend_value<B: $crate::buf::ReverseBuf + ?Sized>(value: &$ty, buf: &mut B) {
                <() as $crate::encoding::ValueEncoder<
                    $crate::encoding::PlainBytes,
                    _
                >>::prepend_value(&&**value, buf)
            }

            fn value_encoded_len(value: &$ty) -> usize {
                <() as $crate::encoding::ValueEncoder<
                    $crate::encoding::PlainBytes,
                    _
                >>::value_encoded_len(&&**value)
            }
        }

        impl$(<$($generics)*>)?
        $crate::encoding::ValueDecoder<$crate::encoding::PlainBytes, $ty> for () {
            fn decode_value<B: $crate::bytes::Buf + ?Sized>(
                $value: &mut $ty,
                mut buf: $crate::encoding::Capped<B>,
                ctx: impl $crate::encoding::DecodeContext,
            ) -> Result<(), $crate::DecodeError> {
                let mut $buf = buf.take_length_delimited()?.take_all();
                // heap used: bytes data goes on the heap
                ctx.heap_used($buf.remaining())?;
                $value.clear();
                $do_reserve;
                while $buf.has_remaining() {
                    let $chunk = $buf.chunk();
                    $do_extend;
                    $buf.advance($chunk.len());
                }
                Ok(())
            }
        }

        impl$(<$($generics)*>)?
        $crate::encoding::DistinguishedValueDecoder<$crate::encoding::PlainBytes, $ty> for () {
            const CHECKS_EMPTY: bool = false;

            fn decode_value_distinguished<const ALLOW_EMPTY: bool>(
                value: &mut $ty,
                buf: $crate::encoding::Capped<impl $crate::bytes::Buf + ?Sized>,
                ctx: impl $crate::encoding::RestrictedDecodeContext,
            ) -> Result<$crate::Canonicity, $crate::DecodeError> {
                <() as $crate::encoding::ValueDecoder<$crate::encoding::PlainBytes, _>>::
                    decode_value
                (
                    value, buf, ctx.into_inner(),
                )?;
                Ok($crate::Canonicity::Canonical)
            }
        }
    }
}
#[allow(unused_imports)]
pub(crate) use plain_bytes_vec_impl;

#[cfg(test)]
pub(crate) mod test {
    #[allow(unused_macros)]
    macro_rules! check_unbounded {
        ($ty:ty) => {
            $crate::encoding::test::check_type_test!(
                $crate::encoding::PlainBytes,
                relaxed,
                from ::alloc::vec::Vec<u8>,
                into $ty,
                converter(val) val.into_iter().collect(),
                $crate::encoding::WireType::LengthDelimited
            );
            $crate::encoding::test::check_type_test!(
                $crate::encoding::PlainBytes,
                distinguished,
                from ::alloc::vec::Vec<u8>,
                into $ty,
                converter(val) val.into_iter().collect(),
                $crate::encoding::WireType::LengthDelimited
            );
        };
    }
    #[allow(unused_macros)]
    macro_rules! check_bounded {
        ($ty:ty, $N:expr) => {
            use proptest::prelude::*;
            proptest! {
                #[test]
                fn check(from in prop::collection::vec(any::<u8>(), 0..=$N), tag: u32) {
                    let into: $ty = from.into_iter().collect();
                    $crate::encoding::test::relaxed::
                        check_type::<$ty, $crate::encoding::PlainBytes>
                    (
                        into.clone(),
                        tag,
                        $crate::encoding::WireType::LengthDelimited,
                    )?;
                    $crate::encoding::test::distinguished::
                        check_type::<$ty, $crate::encoding::PlainBytes>
                    (
                        into,
                        tag,
                        $crate::encoding::WireType::LengthDelimited,
                    )?;
                }
                #[test]
                fn check_optional(
                    from in prop::option::of(prop::collection::vec(any::<u8>(), 0..=$N)),
                    tag: u32,
                ) {
                    let into: Option<$ty> = from.map(|val| val.into_iter().collect());
                    $crate::encoding::test::relaxed::
                        check_type::<Option<$ty>, $crate::encoding::PlainBytes>
                    (
                        into.clone(),
                        tag,
                        $crate::encoding::WireType::LengthDelimited,
                    )?;
                    $crate::encoding::test::distinguished::
                        check_type::<Option<$ty>, $crate::encoding::PlainBytes>
                    (
                        into,
                        tag,
                        $crate::encoding::WireType::LengthDelimited,
                    )?;
                }
            }
        };
    }
    #[allow(unused_imports)]
    pub(crate) use {check_bounded, check_unbounded};
}
