use crate::buf::ReverseBuf;
use crate::encoding::{
    const_varint, delegate_encoding, delegate_value_encoding, encode_varint, encoded_len_varint,
    encoding_implemented_via_value_encoding, prepend_varint, Canonicity, Capped, DecodeContext,
    DecodeError, DistinguishedValueBorrowDecoder, DistinguishedValueDecoder, ForOverwrite,
    RestrictedDecodeContext, ValueBorrowDecoder, ValueDecoder, ValueEncoder, WireType, Wiretyped,
};
use crate::DecodeErrorKind::InvalidValue;
use alloc::borrow::Cow;
use alloc::vec::Vec;
use bytes::{Buf, BufMut};
use core::ops::Deref;

/// `PlainBytes` implements encoding for blob values directly into `Vec<u8>`, and provides the base
/// implementation for that functionality. `Vec<u8>` cannot generically dispatch to `General`'s
/// encoding, since `General` already generically implements encoding for other kinds of `Vec`, but
/// this encoder can be used instead if it's desirable to have a value whose type is exactly
/// `Vec<u8>`.
pub struct PlainBytes;

encoding_implemented_via_value_encoding!(PlainBytes);

impl Wiretyped<PlainBytes> for &[u8] {
    const WIRE_TYPE: WireType = WireType::LengthDelimited;
}

impl ValueEncoder<PlainBytes> for &[u8] {
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

impl<'a> ValueBorrowDecoder<'a, PlainBytes> for &'a [u8] {
    #[inline]
    fn borrow_decode_value(
        value: &mut Self,
        mut buf: Capped<&'a [u8]>,
        _ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        *value = buf.take_borrowed_length_delimited()?;
        Ok(())
    }
}

impl<'a> DistinguishedValueBorrowDecoder<'a, PlainBytes> for &'a [u8] {
    const CHECKS_EMPTY: bool = false;

    #[inline]
    fn borrow_decode_value_distinguished<const ALLOW_EMPTY: bool>(
        value: &mut Self,
        mut buf: Capped<&'a [u8]>,
        _ctx: RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError> {
        *value = buf.take_borrowed_length_delimited()?;
        Ok(Canonicity::Canonical)
    }
}

impl Wiretyped<PlainBytes> for Vec<u8> {
    const WIRE_TYPE: WireType = WireType::LengthDelimited;
}

impl ValueEncoder<PlainBytes> for Vec<u8> {
    #[inline]
    fn encode_value<B: BufMut + ?Sized>(value: &Vec<u8>, buf: &mut B) {
        ValueEncoder::<PlainBytes>::encode_value(&value.as_slice(), buf)
    }

    #[inline]
    fn prepend_value<B: ReverseBuf + ?Sized>(value: &Vec<u8>, buf: &mut B) {
        ValueEncoder::<PlainBytes>::prepend_value(&value.as_slice(), buf)
    }

    #[inline]
    fn value_encoded_len(value: &Vec<u8>) -> usize {
        ValueEncoder::<PlainBytes>::value_encoded_len(&value.as_slice())
    }
}

impl ValueDecoder<PlainBytes> for Vec<u8> {
    fn decode_value<B: Buf + ?Sized>(
        value: &mut Vec<u8>,
        mut buf: Capped<B>,
        _ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        let buf = buf.take_length_delimited()?;
        value.clear();
        value.reserve(buf.remaining_before_cap());
        value.put(buf.take_all());
        Ok(())
    }
}

impl DistinguishedValueDecoder<PlainBytes> for Vec<u8> {
    const CHECKS_EMPTY: bool = false;

    fn decode_value_distinguished<const ALLOW_EMPTY: bool>(
        value: &mut Vec<u8>,
        buf: Capped<impl Buf + ?Sized>,
        ctx: RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError> {
        ValueDecoder::<PlainBytes>::decode_value(value, buf, ctx.into_inner())?;
        Ok(Canonicity::Canonical)
    }
}

delegate_value_encoding!(
    encoding (PlainBytes) borrows type (Vec<u8>) as owned including distinguished
);

delegate_encoding!(delegate from (PlainBytes) to (crate::encoding::Unpacked<PlainBytes>)
    for type (Vec<Vec<u8>>) including distinguished);
delegate_encoding!(delegate from (PlainBytes) to (crate::encoding::Unpacked<PlainBytes>)
    for type (Vec<Cow<'a, [u8]>>) including distinguished with generics ('a));
delegate_encoding!(delegate from (PlainBytes) to (crate::encoding::Unpacked<PlainBytes>)
    for type (Vec<&'a [u8]>) including distinguished with generics ('a));
delegate_encoding!(delegate from (PlainBytes) to (crate::encoding::Unpacked<PlainBytes>)
    for type (Vec<&'a [u8; N]>) including distinguished with generics ('a, const N: usize));

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

impl Wiretyped<PlainBytes> for Cow<'_, [u8]> {
    const WIRE_TYPE: WireType = WireType::LengthDelimited;
}

impl ValueEncoder<PlainBytes> for Cow<'_, [u8]> {
    #[inline]
    fn encode_value<B: BufMut + ?Sized>(value: &Cow<[u8]>, buf: &mut B) {
        ValueEncoder::<PlainBytes>::encode_value(&&**value, buf)
    }

    #[inline]
    fn prepend_value<B: ReverseBuf + ?Sized>(value: &Cow<[u8]>, buf: &mut B) {
        ValueEncoder::<PlainBytes>::prepend_value(&&**value, buf)
    }

    #[inline]
    fn value_encoded_len(value: &Cow<[u8]>) -> usize {
        ValueEncoder::<PlainBytes>::value_encoded_len(&&**value)
    }
}

impl ValueDecoder<PlainBytes> for Cow<'_, [u8]> {
    #[inline]
    fn decode_value<B: Buf + ?Sized>(
        value: &mut Cow<[u8]>,
        buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        ValueDecoder::<PlainBytes>::decode_value(value.to_mut(), buf, ctx)
    }
}

impl DistinguishedValueDecoder<PlainBytes> for Cow<'_, [u8]> {
    const CHECKS_EMPTY: bool = <Vec<u8> as DistinguishedValueDecoder<PlainBytes>>::CHECKS_EMPTY;

    #[inline]
    fn decode_value_distinguished<const ALLOW_EMPTY: bool>(
        value: &mut Cow<[u8]>,
        buf: Capped<impl Buf + ?Sized>,
        ctx: RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError> {
        DistinguishedValueDecoder::<PlainBytes>::decode_value_distinguished::<ALLOW_EMPTY>(
            value.to_mut(),
            buf,
            ctx,
        )
    }
}

impl<'a> ValueBorrowDecoder<'a, PlainBytes> for Cow<'a, [u8]> {
    #[inline]
    fn borrow_decode_value(
        value: &mut Cow<'a, [u8]>,
        buf: Capped<&'a [u8]>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        let mut s = <&[u8]>::for_overwrite();
        ValueBorrowDecoder::<PlainBytes>::borrow_decode_value(&mut s, buf, ctx)?;
        *value = Cow::Borrowed(s);
        Ok(())
    }
}

impl<'a> DistinguishedValueBorrowDecoder<'a, PlainBytes> for Cow<'a, [u8]> {
    const CHECKS_EMPTY: bool =
        <&[u8] as DistinguishedValueBorrowDecoder<'a, PlainBytes>>::CHECKS_EMPTY;

    #[inline]
    fn borrow_decode_value_distinguished<const ALLOW_EMPTY: bool>(
        value: &mut Cow<'a, [u8]>,
        buf: Capped<&'a [u8]>,
        ctx: RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError> {
        ValueBorrowDecoder::<PlainBytes>::borrow_decode_value(value, buf, ctx.into_inner())?;
        Ok(Canonicity::Canonical)
    }
}

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

impl<const N: usize> Wiretyped<PlainBytes> for [u8; N] {
    const WIRE_TYPE: WireType = WireType::LengthDelimited;
}

impl<const N: usize> ValueEncoder<PlainBytes> for [u8; N] {
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

impl<const N: usize> ValueDecoder<PlainBytes> for [u8; N] {
    fn decode_value<B: Buf + ?Sized>(
        value: &mut [u8; N],
        mut buf: Capped<B>,
        _ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        let mut delimited = buf.take_length_delimited()?;
        if delimited.remaining_before_cap() != N {
            return Err(DecodeError::new(InvalidValue));
        }
        delimited.copy_to_slice(value.as_mut_slice());
        Ok(())
    }
}

impl<const N: usize> DistinguishedValueDecoder<PlainBytes> for [u8; N] {
    const CHECKS_EMPTY: bool = false;

    fn decode_value_distinguished<const ALLOW_EMPTY: bool>(
        value: &mut [u8; N],
        buf: Capped<impl Buf + ?Sized>,
        ctx: RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError> {
        ValueDecoder::<PlainBytes>::decode_value(value, buf, ctx.into_inner())?;
        Ok(Canonicity::Canonical)
    }
}

delegate_value_encoding!(
    encoding (PlainBytes) borrows type ([u8; N]) as owned including distinguished
    with generics (const N: usize)
);

// TODO(widders): implement &[u8; N]

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

#[allow(unused_macros)]
macro_rules! plain_bytes_vec_impl {
    (
        $ty:ty,
        $value:ident, $buf:ident, $chunk:ident,
        $do_reserve:expr,
        $do_extend:expr
        $(, with generics ($($generics:tt)*))?
    ) => {
        $crate::encoding::delegate_value_encoding!(
            encoding ($crate::encoding::PlainBytes)
            borrows type ($ty) as owned including distinguished
            $(with generics ($($generics)*))?
        );

        impl$(<$($generics)*>)? $crate::encoding::Wiretyped<$crate::encoding::PlainBytes> for $ty {
            const WIRE_TYPE: $crate::encoding::WireType =
                $crate::encoding::WireType::LengthDelimited;
        }

        impl$(<$($generics)*>)? $crate::encoding::ValueEncoder<$crate::encoding::PlainBytes>
        for $ty {
            fn encode_value<B: $crate::bytes::BufMut + ?Sized>(value: &$ty, buf: &mut B) {
                $crate::encoding::ValueEncoder::<$crate::encoding::PlainBytes>::encode_value
                    (&&**value, buf)
            }

            fn prepend_value<B: $crate::buf::ReverseBuf + ?Sized>(value: &$ty, buf: &mut B) {
                $crate::encoding::ValueEncoder::<$crate::encoding::PlainBytes>::prepend_value
                    (&&**value, buf)
            }

            fn value_encoded_len(value: &$ty) -> usize {
                $crate::encoding::ValueEncoder::<$crate::encoding::PlainBytes>::value_encoded_len
                    (&&**value)
            }
        }

        impl$(<$($generics)*>)? $crate::encoding::ValueDecoder<$crate::encoding::PlainBytes>
        for $ty {
            fn decode_value<B: $crate::bytes::Buf + ?Sized>(
                $value: &mut $ty,
                mut buf: $crate::encoding::Capped<B>,
                _ctx: $crate::encoding::DecodeContext,
            ) -> Result<(), $crate::DecodeError> {
                let mut $buf = buf.take_length_delimited()?.take_all();
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
        $crate::encoding::DistinguishedValueDecoder<$crate::encoding::PlainBytes> for $ty {
            const CHECKS_EMPTY: bool = false;

            fn decode_value_distinguished<const ALLOW_EMPTY: bool>(
                value: &mut $ty,
                buf: $crate::encoding::Capped<impl $crate::bytes::Buf + ?Sized>,
                ctx: $crate::encoding::RestrictedDecodeContext,
            ) -> Result<$crate::Canonicity, $crate::DecodeError> {
                $crate::encoding::ValueDecoder::<$crate::encoding::PlainBytes>::decode_value(
                    value, buf, ctx.into_inner())?;
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
