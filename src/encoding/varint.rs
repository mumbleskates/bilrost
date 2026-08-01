use crate::buf::ReverseBuf;
use crate::encoding::schema::{Schema, ValueRepr};
use crate::encoding::{
    encode_varint, encoded_len_varint, encoding_implemented_via_value_encoding,
    encoding_uses_base_empty_state, prepend_varint, Buf, BufMut, Canonicity, Capped, DecodeContext,
    DistinguishedValueDecoder, RestrictedDecodeContext, ValueDecoder, ValueEncoder, WireType,
    Wiretyped,
};
use crate::DecodeError;
use crate::DecodeErrorKind::{InvalidValue, OutOfDomainValue};
use alloc::boxed::Box;
use alloc::format;
use core::fmt::Display;
use core::mem;

pub struct Varint;

encoding_uses_base_empty_state!(Varint);
encoding_implemented_via_value_encoding!(Varint);

/// Zig-zag encoding: These functions implement storing signed in unsigned integers by encoding the
/// sign bit in the least significant bit.
#[inline]
fn i8_to_unsigned(value: i8) -> u8 {
    ((value << 1) ^ (value >> 7)) as u8
}

#[inline]
fn u8_to_signed(value: u8) -> i8 {
    ((value >> 1) as i8) ^ (-((value & 1) as i8))
}

#[inline]
fn i16_to_unsigned(value: i16) -> u16 {
    ((value << 1) ^ (value >> 15)) as u16
}

#[inline]
fn u16_to_signed(value: u16) -> i16 {
    ((value >> 1) as i16) ^ (-((value & 1) as i16))
}

#[inline]
fn i32_to_unsigned(value: i32) -> u32 {
    ((value << 1) ^ (value >> 31)) as u32
}

#[inline]
fn u32_to_signed(value: u32) -> i32 {
    ((value >> 1) as i32) ^ (-((value & 1) as i32))
}

#[inline]
pub(crate) fn i64_to_unsigned(value: i64) -> u64 {
    ((value << 1) ^ (value >> 63)) as u64
}

#[inline]
pub(crate) fn u64_to_signed(value: u64) -> i64 {
    ((value >> 1) as i64) ^ (-((value & 1) as i64))
}

/// Macro which emits implementations for variable width numeric encoding.
macro_rules! varint {
    (
        $name:ident,
        $ty:ty,
        $signedness:literal,
        to_uint64($to_uint64_value:ident) $to_uint64:expr,
        from_uint64($from_uint64_value:ident) $from_uint64:expr
        $(, $($avoid_no_empty_state:tt)*)?
    ) => {
        impl Wiretyped<Varint, $ty> for () {
            const WIRE_TYPE: WireType = WireType::Varint;
        }

        impl ValueRepr<Varint, $ty> for () {
            fn repr(_schema: &Schema) -> Box<dyn Display> {
                const SIGNEDNESS: &'static str = $signedness;
                const SIZE: usize = mem::size_of::<$ty>();
                if SIGNEDNESS== "boolean" {
                    Box::new("varint; boolean 0 or 1")
                } else if SIZE == 8 {
                    Box::new(format!("varint, {SIGNEDNESS}"))
                } else {
                    Box::new(format!("varint, {SIGNEDNESS}; in {bits} bit range", bits = SIZE * 8))
                }
            }
        }

        impl ValueEncoder<Varint, $ty> for () {
            #[inline(always)]
            fn encode_value<B: BufMut + ?Sized>($to_uint64_value: &$ty, buf: &mut B) {
                encode_varint($to_uint64, buf);
            }

            #[inline(always)]
            fn prepend_value<B: ReverseBuf + ?Sized>($to_uint64_value: &$ty, buf: &mut B) {
                prepend_varint($to_uint64, buf);
            }

            #[inline(always)]
            fn value_encoded_len($to_uint64_value: &$ty) -> usize {
                encoded_len_varint($to_uint64)
            }
        }

        impl ValueDecoder<Varint, $ty> for () {
            #[inline(always)]
            fn decode_value<B: Buf + ?Sized>(
                __value: &mut $ty,
                mut buf: Capped<B>,
                _ctx: DecodeContext,
            ) -> Result<(), DecodeError> {
                let $from_uint64_value = buf.decode_varint()?;
                *__value = $from_uint64;
                Ok(())
            }
        }

        impl DistinguishedValueDecoder<Varint, $ty> for () {
            const CHECKS_EMPTY: bool = false;

            #[inline]
            fn decode_value_distinguished<const ALLOW_EMPTY: bool>(
                value: &mut $ty,
                buf: Capped<impl Buf + ?Sized>,
                ctx: RestrictedDecodeContext,
            ) -> Result<Canonicity, DecodeError> {
                <() as ValueDecoder::<Varint, _>>::decode_value(value, buf, ctx.into_inner())?;
                Ok(Canonicity::Canonical)
            }
        }

        crate::encoding::delegate_value_encoding!(
            encoding (Varint) borrows type ($ty) as owned including distinguished
        );

        #[cfg(test)]
        mod $name {
            use crate::encoding::Varint;
            crate::encoding::test::check_type_test!(
                Varint,
                relaxed,
                $ty,
                WireType::Varint
                $(, $($avoid_no_empty_state)*)?
            );
            crate::encoding::test::check_type_test!(
                Varint,
                distinguished,
                $ty,
                WireType::Varint
                $(, $($avoid_no_empty_state)*)?
            );
        }
    };
}

varint!(varint_bool, bool, "boolean",
to_uint64(value) {
    u64::from(*value)
},
from_uint64(value) {
    match value {
        0 => false,
        1 => true,
        _ => return Err(DecodeError::new(OutOfDomainValue))
    }
});

varint!(varint_u8, u8, "unsigned",
to_uint64(value) {
    *value as u64
},
from_uint64(value) {
    u8::try_from(value).map_err(|_| DecodeError::new(OutOfDomainValue))?
});

varint!(varint_nonzerou8, core::num::NonZeroU8, "unsigned nonzero",
to_uint64(value) {
    value.get() as u64
},
from_uint64(value) {
    core::num::NonZeroU8::new(
        u8::try_from(value).map_err(|_| DecodeError::new(OutOfDomainValue))?
    ).ok_or_else(|| DecodeError::new(InvalidValue))?
}, has no empty state);

varint!(varint_u16, u16, "unsigned",
to_uint64(value) {
    *value as u64
},
from_uint64(value) {
    u16::try_from(value).map_err(|_| DecodeError::new(OutOfDomainValue))?
});

varint!(varint_nonzerou16, core::num::NonZeroU16, "unsigned nonzero",
to_uint64(value) {
    value.get() as u64
},
from_uint64(value) {
    core::num::NonZeroU16::new(
        u16::try_from(value).map_err(|_| DecodeError::new(OutOfDomainValue))?
    ).ok_or_else(|| DecodeError::new(InvalidValue))?
}, has no empty state);

varint!(varint_u32, u32, "unsigned",
to_uint64(value) {
    *value as u64
},
from_uint64(value) {
    u32::try_from(value).map_err(|_| DecodeError::new(OutOfDomainValue))?
});

varint!(varint_nonzerou32, core::num::NonZeroU32, "unsigned nonzero",
to_uint64(value) {
    value.get() as u64
},
from_uint64(value) {
    core::num::NonZeroU32::new(
        u32::try_from(value).map_err(|_| DecodeError::new(OutOfDomainValue))?
    ).ok_or_else(|| DecodeError::new(InvalidValue))?
}, has no empty state);

varint!(varint_u64, u64, "unsigned",
to_uint64(value) {
    *value
},
from_uint64(value) {
    value
});

varint!(varint_nonzerou64, core::num::NonZeroU64, "unsigned nonzero",
to_uint64(value) {
    value.get()
},
from_uint64(value) {
    core::num::NonZeroU64::new(value).ok_or_else(|| DecodeError::new(InvalidValue))?
}, has no empty state);

varint!(varint_usize, usize, "unsigned",
to_uint64(value) {
    *value as u64
},
from_uint64(value) {
    usize::try_from(value).map_err(|_| DecodeError::new(OutOfDomainValue))?
});

varint!(varint_nonzerousize, core::num::NonZeroUsize, "unsigned nonzero",
to_uint64(value) {
    value.get() as u64
},
from_uint64(value) {
    core::num::NonZeroUsize::new(
        usize::try_from(value).map_err(|_| DecodeError::new(OutOfDomainValue))?
    ).ok_or_else(|| DecodeError::new(InvalidValue))?
}, has no empty state);

varint!(varint_i8, i8, "signed",
to_uint64(value) {
    i8_to_unsigned(*value) as u64
},
from_uint64(value) {
    let value = u8::try_from(value)
        .map_err(|_| DecodeError::new(OutOfDomainValue))?;
    u8_to_signed(value)
});

varint!(varint_nonzeroi8, core::num::NonZeroI8, "signed nonzero",
to_uint64(value) {
    i8_to_unsigned(value.get()) as u64
},
from_uint64(value) {
    let value = u8::try_from(value)
        .map_err(|_| DecodeError::new(OutOfDomainValue))?;
    core::num::NonZeroI8::new(u8_to_signed(value))
        .ok_or_else(|| DecodeError::new(InvalidValue))?
}, has no empty state);

varint!(varint_i16, i16, "signed",
to_uint64(value) {
    i16_to_unsigned(*value) as u64
},
from_uint64(value) {
    let value = u16::try_from(value)
        .map_err(|_| DecodeError::new(OutOfDomainValue))?;
    u16_to_signed(value)
});

varint!(varint_nonzeroi16, core::num::NonZeroI16, "signed nonzero",
to_uint64(value) {
    i16_to_unsigned(value.get()) as u64
},
from_uint64(value) {
    let value = u16::try_from(value)
        .map_err(|_| DecodeError::new(OutOfDomainValue))?;
    core::num::NonZeroI16::new(u16_to_signed(value))
        .ok_or_else(|| DecodeError::new(InvalidValue))?
}, has no empty state);

varint!(varint_i32, i32, "signed",
to_uint64(value) {
    i32_to_unsigned(*value) as u64
},
from_uint64(value) {
    let value = u32::try_from(value)
        .map_err(|_| DecodeError::new(OutOfDomainValue))?;
    u32_to_signed(value)
});

varint!(varint_nonzeroi32, core::num::NonZeroI32, "signed nonzero",
to_uint64(value) {
    i32_to_unsigned(value.get()) as u64
},
from_uint64(value) {
    let value = u32::try_from(value)
        .map_err(|_| DecodeError::new(OutOfDomainValue))?;
    core::num::NonZeroI32::new(u32_to_signed(value))
        .ok_or_else(|| DecodeError::new(InvalidValue))?
}, has no empty state);

varint!(varint_i64, i64, "signed",
to_uint64(value) {
    i64_to_unsigned(*value)
},
from_uint64(value) {
    u64_to_signed(value)
});

varint!(varint_nonzero648, core::num::NonZeroI64, "signed nonzero",
to_uint64(value) {
    i64_to_unsigned(value.get())
},
from_uint64(value) {
    core::num::NonZeroI64::new(u64_to_signed(value))
        .ok_or_else(|| DecodeError::new(InvalidValue))?
}, has no empty state);

varint!(varint_isize, isize, "signed",
to_uint64(value) {
    i64_to_unsigned(*value as i64)
},
from_uint64(value) {
    isize::try_from(u64_to_signed(value))
        .map_err(|_| DecodeError::new(OutOfDomainValue))?
});

varint!(varint_nonzeroisize, core::num::NonZeroIsize, "signed nonzero",
to_uint64(value) {
    i64_to_unsigned(value.get() as i64)
},
from_uint64(value) {
    let value = isize::try_from(u64_to_signed(value))
        .map_err(|_| DecodeError::new(OutOfDomainValue))?;
    core::num::NonZeroIsize::new(value)
        .ok_or_else(|| DecodeError::new(InvalidValue))?
}, has no empty state);
