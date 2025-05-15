//! Every tuple type starting at arity 1 and up to arity 12 implements ValueEncoder for the encoding
//! (E1, ... EN) where its elements are encoded with the corresponding sub-encoding. The
//! representation on the wire is exactly the same as if it were a message type that had fields with
//! tags 0 through arity-minus-1.
//!
//! Note again that these tags start at zero, not at 1 as they do by default when deriving `Message`
//! for a struct. This is for similar reasons; struct fields "feel" more natural when numbered
//! ordinally, but tuple types in rust are *indexed* starting at zero so we prefer to maintain
//! parity with that.
//!
//! The arity-zero unit tuple encodes the same way, but it does so independently of this file.
//! Because it has no fields, it has a privileged lack of ambiguity regarding *how* it will encode,
//! where every other tuple type might appear differently when included in a message depending on
//! which encodings are chosen for each member. For this reason, it is the only tuple type that
//! implements `Message` itself, and it stands as the prototype for a message with no defined
//! fields.

use bytes::{Buf, BufMut};

use crate::buf::ReverseBuf;
use crate::encoding::{
    delegate_value_encoding, encode_varint, encoded_len_varint,
    encoding_implemented_via_value_encoding, implement_core_empty_state_rules, prepend_varint,
    skip_field, BorrowDecoder, Canonicity, Capped, DecodeContext, Decoder,
    DistinguishedBorrowDecoder, DistinguishedDecoder, DistinguishedValueBorrowDecoder,
    DistinguishedValueDecoder, EmptyState, Encoder, ForOverwrite, General, GeneralGeneric,
    RestrictedDecodeContext, TagReader, TagRevWriter, TagWriter, TrivialTagMeasurer,
    ValueBorrowDecoder, ValueDecoder, ValueEncoder, WireType, Wiretyped,
};
use crate::DecodeError;
use crate::DecodeErrorKind::UnexpectedlyRepeated;

macro_rules! impl_tuple {
    (
        $arity:expr,
        $name:expr,
        $test_mod_name:ident,
        ($($numbers:tt),*),
        ($($numbers_desc:tt),*),
        ($($letters:ident),*),
        ($($letters_desc:ident),*),
        ($($encodings:ident),*),
        ($($encodings_desc:ident),*),
        ($($tees:ident),*),
    ) => {
        implement_core_empty_state_rules!(($($encodings,)*), with generics ($($encodings),*));

        // All tuple types encode as nested messages, so all of them implement ValueEncoder and
        // should therefore implement Encoder in terms of that.
        encoding_implemented_via_value_encoding!(
            ($($encodings,)*),
            with generics ($($encodings),*)
        );

        impl<$($letters,)* $($encodings,)*> ForOverwrite<($($encodings,)*), ($($letters,)*)> for ()
        where
            $((): ForOverwrite<$encodings, $letters>,)*
        {
            #[inline]
            fn for_overwrite() -> ($($letters,)*) {
                ($(<() as ForOverwrite<$encodings, $letters>>::for_overwrite(),)*)
            }
        }

        impl<$($letters,)* $($encodings,)*> EmptyState<($($encodings,)*), ($($letters,)*)> for ()
        where
            $((): EmptyState<$encodings, $letters>,)*
        {
            #[inline]
            fn empty() -> ($($letters,)*) {
                ($(<() as EmptyState<$encodings, $letters>>::empty(),)*)
            }

            #[inline]
            fn is_empty(val: &($($letters,)*)) -> bool {
                true $(&& <() as EmptyState<$encodings, $letters>>::is_empty(&val.$numbers))*
            }

            #[inline]
            fn clear(val: &mut ($($letters,)*)) {
                $(<() as EmptyState<$encodings, $letters>>::clear(&mut val.$numbers);)*
            }
        }

        impl<$($letters,)* $($encodings,)*> Wiretyped<($($encodings,)*), ($($letters,)*)> for () {
            const WIRE_TYPE: WireType = WireType::LengthDelimited;
        }

        impl<$($letters,)* $($encodings,)*> ValueEncoder<($($encodings,)*), ($($letters,)*)> for ()
        where
            $((): EmptyState<$encodings, $letters> + Encoder<$encodings, $letters>,)*
        {
            #[inline]
            fn encode_value<__B: BufMut + ?Sized>(value: &($($letters,)*), buf: &mut __B) {
                // Because we do not implement tuples with more than arity 32, we can always use
                // the trivial tag measurer implementation.
                let tm = &mut TrivialTagMeasurer::new();
                let message_len = 0usize $(+ <() as Encoder<$encodings, _>>::encoded_len(
                    $numbers,
                    &value.$numbers,
                    tm,
                ))*;
                encode_varint(message_len as u64, buf);
                let tw = &mut TagWriter::new();
                $(<() as Encoder<$encodings, _>>::encode($numbers, &value.$numbers, buf, tw);)*
            }

            #[inline]
            fn prepend_value<__B: ReverseBuf + ?Sized>(
                value: &($($letters,)*),
                buf: &mut __B,
            ) {
                let end = buf.remaining();
                let tw = &mut TagRevWriter::new();
                $(<() as Encoder<$encodings_desc, _>>::prepend_encode(
                    $numbers_desc,
                    &value.$numbers_desc,
                    buf,
                    tw,
                );)*
                tw.finalize(buf);
                prepend_varint((buf.remaining() - end) as u64, buf);
            }

            #[inline]
            fn value_encoded_len(value: &($($letters,)*)) -> usize {
                // Because we do not implement tuples with more than arity 32, we can always use
                // the trivial tag measurer implementation.
                let tm = &mut TrivialTagMeasurer::new();
                let message_len = 0usize $(+ <() as Encoder<$encodings, _>>::encoded_len(
                    $numbers,
                    &value.$numbers,
                    tm,
                ))*;
                encoded_len_varint(message_len as u64) + message_len
            }
        }

        impl<$($letters,)* $($encodings,)*> ValueDecoder<($($encodings,)*), ($($letters,)*)> for ()
        where
            $((): EmptyState<$encodings, $letters> + Decoder<$encodings, $letters>,)*
        {
            #[inline]
            fn decode_value<__B: Buf + ?Sized>(
                value: &mut ($($letters,)*),
                mut buf: Capped<__B>,
                ctx: DecodeContext,
            ) -> Result<(), DecodeError> {
                let mut buf = buf.take_length_delimited()?;
                ctx.limit_reached()?;
                let ctx = ctx.enter_recursion();
                let tr = &mut TagReader::new();
                let mut last_tag = None::<u32>;
                while buf.has_remaining()? {
                    let (tag, wire_type) = tr.decode_key(buf.lend())?;
                    let duplicated = last_tag == Some(tag);
                    last_tag = Some(tag);
                    // Decode the field. Each tuple field has a tag corresponding to its index.
                    match tag {
                        $($numbers => {
                            if duplicated {
                                Err(DecodeError::new(UnexpectedlyRepeated))
                            } else {
                                <() as Decoder<$encodings, _>>::decode(
                                    wire_type,
                                    &mut value.$numbers,
                                    buf.lend(),
                                    ctx.clone(),
                                )
                            }
                            .map_err(|mut error| {
                                error.push($name, stringify!($numbers));
                                error
                            })?
                        })*
                        _ => skip_field(wire_type, buf.lend())?,
                    }
                }
                Ok(())
            }
        }

        impl<$($letters,)* $($encodings,)*>
        DistinguishedValueDecoder<($($encodings,)*), ($($letters,)*)> for ()
        where
            ($($letters,)*): Eq,
            $(
                $letters: Eq,
                (): EmptyState<$encodings, $letters>
                    + DistinguishedDecoder<$encodings, $letters>,
            )*
        {
            const CHECKS_EMPTY: bool = true; // Message types are always zero-length when empty

            #[inline]
            fn decode_value_distinguished<const ALLOW_EMPTY: bool>(
                value: &mut ($($letters,)*),
                mut buf: Capped<impl Buf + ?Sized>,
                ctx: RestrictedDecodeContext,
            ) -> Result<Canonicity, DecodeError>
            where
                ($($letters,)*): Sized,
            {
                let mut buf = buf.take_length_delimited()?;
                // Since tuples emulate messages, empty values always encode and decode from zero
                // bytes. It is far cheaper to check here than to check after the value has been
                // decoded and checking the value's `is_empty()`.
                if !ALLOW_EMPTY && buf.remaining_before_cap() == 0 {
                    return ctx.check(Canonicity::NotCanonical);
                }
                ctx.limit_reached()?;
                let mut canon = Canonicity::Canonical;
                let ctx = ctx.enter_recursion();
                let tr = &mut TagReader::new();
                let mut last_tag = None::<u32>;
                while buf.has_remaining()? {
                    let (tag, wire_type) = tr.decode_key(buf.lend())?;
                    let duplicated = last_tag == Some(tag);
                    last_tag = Some(tag);
                    // Decode the field. Each tuple field has a tag corresponding to its index.
                    match tag {
                        $($numbers => {
                            canon.update(
                                if duplicated {
                                    Err(DecodeError::new(UnexpectedlyRepeated))
                                } else {
                                    <() as DistinguishedDecoder<$encodings, _>>
                                        ::decode_distinguished(
                                            wire_type,
                                            &mut value.$numbers,
                                            buf.lend(),
                                            ctx.clone(),
                                        )
                                }
                                .map_err(|mut error| {
                                    error.push($name, stringify!($numbers));
                                    error
                                })?
                            );
                        })*
                        _ => {
                            canon.update(ctx.check(Canonicity::HasExtensions)?);
                            skip_field(wire_type, buf.lend())?;
                        },
                    }
                }
                Ok(canon)
            }
        }

        // We'd like to implement the borrowed decoders here with decoding_modes::__invoke!, but it
        // seems the optional lifetime and the repeating elements in the tuple won't nest in
        // macro_rules!.

        impl<'a, $($letters,)* $($encodings,)*>
        ValueBorrowDecoder<'a, ($($encodings,)*), ($($letters,)*)> for ()
        where
            $((): EmptyState<$encodings, $letters> + BorrowDecoder<'a, $encodings, $letters>,)*
        {
            #[inline]
            fn borrow_decode_value(
                value: &mut ($($letters,)*),
                mut buf: Capped<&'a [u8]>,
                ctx: DecodeContext,
            ) -> Result<(), DecodeError> {
                let mut buf = buf.take_length_delimited()?;
                ctx.limit_reached()?;
                let ctx = ctx.enter_recursion();
                let tr = &mut TagReader::new();
                let mut last_tag = None::<u32>;
                while buf.has_remaining()? {
                    let (tag, wire_type) = tr.decode_key(buf.lend())?;
                    let duplicated = last_tag == Some(tag);
                    last_tag = Some(tag);
                    // Decode the field. Each tuple field has a tag corresponding to its index.
                    match tag {
                        $($numbers => {
                            if duplicated {
                                Err(DecodeError::new(UnexpectedlyRepeated))
                            } else {
                                <() as BorrowDecoder<$encodings, _>>::borrow_decode(
                                    wire_type,
                                    &mut value.$numbers,
                                    buf.lend(),
                                    ctx.clone(),
                                )
                            }
                            .map_err(|mut error| {
                                error.push($name, stringify!($numbers));
                                error
                            })?
                        })*
                        _ => skip_field(wire_type, buf.lend())?,
                    }
                }
                Ok(())
            }
        }

        impl<'a, $($letters,)* $($encodings,)*>
        DistinguishedValueBorrowDecoder<'a, ($($encodings,)*), ($($letters,)*)> for ()
        where
            ($($letters,)*): Eq,
            $(
                $letters: Eq,
                (): EmptyState<$encodings, $letters>
                    + DistinguishedBorrowDecoder<'a, $encodings, $letters>,
            )*
        {
            const CHECKS_EMPTY: bool = true; // Message types are always zero-length when empty

            #[inline]
            fn borrow_decode_value_distinguished<const ALLOW_EMPTY: bool>(
                value: &mut ($($letters,)*),
                mut buf: Capped<&'a [u8]>,
                ctx: RestrictedDecodeContext,
            ) -> Result<Canonicity, DecodeError>
            where
                ($($letters,)*): Sized,
            {
                let mut buf = buf.take_length_delimited()?;
                // Since tuples emulate messages, empty values always encode and decode from zero
                // bytes. It is far cheaper to check here than to check after the value has been
                // decoded and checking the value's `is_empty()`.
                if !ALLOW_EMPTY && buf.remaining_before_cap() == 0 {
                    return ctx.check(Canonicity::NotCanonical);
                }
                ctx.limit_reached()?;
                let mut canon = Canonicity::Canonical;
                let ctx = ctx.enter_recursion();
                let tr = &mut TagReader::new();
                let mut last_tag = None::<u32>;
                while buf.has_remaining()? {
                    let (tag, wire_type) = tr.decode_key(buf.lend())?;
                    let duplicated = last_tag == Some(tag);
                    last_tag = Some(tag);
                    // Decode the field. Each tuple field has a tag corresponding to its index.
                    match tag {
                        $($numbers => {
                            canon.update(
                                if duplicated {
                                    Err(DecodeError::new(UnexpectedlyRepeated))
                                } else {
                                    <() as DistinguishedBorrowDecoder<$encodings, _>>
                                        ::borrow_decode_distinguished(
                                            wire_type,
                                            &mut value.$numbers,
                                            buf.lend(),
                                            ctx.clone(),
                                        )
                                }
                                .map_err(|mut error| {
                                    error.push($name, stringify!($numbers));
                                    error
                                })?
                            );
                        })*
                        _ => {
                            canon.update(ctx.check(Canonicity::HasExtensions)?);
                            skip_field(wire_type, buf.lend())?;
                        },
                    }
                }
                Ok(canon)
            }
        }

        #[cfg(test)]
        mod $test_mod_name {
            // MSRV: impl From<[T; N]> for (T, ...{N}) is new in rust 1.71
            fn array_to_tuple<T: Clone>(arr: [T; $arity]) -> ($($tees,)*) {
                ($(arr[$numbers].clone(),)*)
            }

            mod delegated_bools {
                use crate::encoding::General;
                use crate::encoding::test::check_type_test;
                type T = bool;

                check_type_test!(
                    General,
                    relaxed,
                    from [T; $arity],
                    into ($($tees,)*),
                    converter(value) { super::super::array_to_tuple(value) },
                    WireType::LengthDelimited
                );
                check_type_test!(
                    General,
                    distinguished,
                    from [T; $arity],
                    into ($($tees,)*),
                    converter(value) { super::super::array_to_tuple(value) },
                    WireType::LengthDelimited
                );
            }
            mod varint_bools {
                use crate::encoding::test::check_type_test;
                type T = bool;
                $(type $encodings = crate::encoding::Varint;)*

                check_type_test!(
                    ($($encodings,)*),
                    relaxed,
                    from [T; $arity],
                    into ($($tees,)*),
                    converter(value) { super::super::array_to_tuple(value) },
                    WireType::LengthDelimited
                );
                check_type_test!(
                    ($($encodings,)*),
                    distinguished,
                    from [T; $arity],
                    into ($($tees,)*),
                    converter(value) { super::super::array_to_tuple(value) },
                    WireType::LengthDelimited
                );
            }
            mod fixed_floats {
                use crate::encoding::test::check_type_test;
                type T = f32;
                $(type $encodings = crate::encoding::Fixed;)*

                check_type_test!(
                    ($($encodings,)*),
                    relaxed,
                    from [T; $arity],
                    into ($($tees,)*),
                    converter(value) { super::super::array_to_tuple(value) },
                    WireType::LengthDelimited
                );
            }
            mod small_arrays {
                use crate::encoding::test::check_type_test;
                type T = [u8; 1];
                $(type $encodings = crate::encoding::PlainBytes;)*

                check_type_test!(
                    ($($encodings,)*),
                    relaxed,
                    from [T; $arity],
                    into ($($tees,)*),
                    converter(value) { super::super::array_to_tuple(value) },
                    WireType::LengthDelimited
                );
                check_type_test!(
                    ($($encodings,)*),
                    distinguished,
                    from [T; $arity],
                    into ($($tees,)*),
                    converter(value) { super::super::array_to_tuple(value) },
                    WireType::LengthDelimited
                );
            }
        }
    }
}

impl_tuple!(
    1,             //
    "(1-tuple)",   //
    tuple_arity_1, //
    (0),           //
    (0),           //
    (A),           //
    (A),           //
    (Ae),          //
    (Ae),          //
    (T),           //
);
impl_tuple!(
    2,             //
    "(2-tuple)",   //
    tuple_arity_2, //
    (0, 1),        //
    (1, 0),        //
    (A, B),        //
    (B, A),        //
    (Ae, Be),      //
    (Be, Ae),      //
    (T, T),        //
);
impl_tuple!(
    3,             //
    "(3-tuple)",   //
    tuple_arity_3, //
    (0, 1, 2),     //
    (2, 1, 0),     //
    (A, B, C),     //
    (C, B, A),     //
    (Ae, Be, Ce),  //
    (Ce, Be, Ae),  //
    (T, T, T),     //
);
impl_tuple!(
    4,                //
    "(4-tuple)",      //
    tuple_arity_4,    //
    (0, 1, 2, 3),     //
    (3, 2, 1, 0),     //
    (A, B, C, D),     //
    (D, C, B, A),     //
    (Ae, Be, Ce, De), //
    (De, Ce, Be, Ae), //
    (T, T, T, T),     //
);
impl_tuple!(
    5,                    //
    "(5-tuple)",          //
    tuple_arity_5,        //
    (0, 1, 2, 3, 4),      //
    (4, 3, 2, 1, 0),      //
    (A, B, C, D, E),      //
    (E, D, C, B, A),      //
    (Ae, Be, Ce, De, Ee), //
    (Ee, De, Ce, Be, Ae), //
    (T, T, T, T, T),      //
);
impl_tuple!(
    6,                        //
    "(6-tuple)",              //
    tuple_arity_6,            //
    (0, 1, 2, 3, 4, 5),       //
    (5, 4, 3, 2, 1, 0),       //
    (A, B, C, D, E, F),       //
    (F, E, D, C, B, A),       //
    (Ae, Be, Ce, De, Ee, Fe), //
    (Fe, Ee, De, Ce, Be, Ae), //
    (T, T, T, T, T, T),       //
);
impl_tuple!(
    7,                            //
    "(7-tuple)",                  //
    tuple_arity_7,                //
    (0, 1, 2, 3, 4, 5, 6),        //
    (6, 5, 4, 3, 2, 1, 0),        //
    (A, B, C, D, E, F, G),        //
    (G, F, E, D, C, B, A),        //
    (Ae, Be, Ce, De, Ee, Fe, Ge), //
    (Ge, Fe, Ee, De, Ce, Be, Ae), //
    (T, T, T, T, T, T, T),        //
);
impl_tuple!(
    8,                                //
    "(8-tuple)",                      //
    tuple_arity_8,                    //
    (0, 1, 2, 3, 4, 5, 6, 7),         //
    (7, 6, 5, 4, 3, 2, 1, 0),         //
    (A, B, C, D, E, F, G, H),         //
    (H, G, F, E, D, C, B, A),         //
    (Ae, Be, Ce, De, Ee, Fe, Ge, He), //
    (He, Ge, Fe, Ee, De, Ce, Be, Ae), //
    (T, T, T, T, T, T, T, T),         //
);
impl_tuple!(
    9,                                    //
    "(9-tuple)",                          //
    tuple_arity_9,                        //
    (0, 1, 2, 3, 4, 5, 6, 7, 8),          //
    (8, 7, 6, 5, 4, 3, 2, 1, 0),          //
    (A, B, C, D, E, F, G, H, I),          //
    (I, H, G, F, E, D, C, B, A),          //
    (Ae, Be, Ce, De, Ee, Fe, Ge, He, Ie), //
    (Ie, He, Ge, Fe, Ee, De, Ce, Be, Ae), //
    (T, T, T, T, T, T, T, T, T),          //
);
impl_tuple!(
    10,                                       //
    "(10-tuple)",                             //
    tuple_arity_10,                           //
    (0, 1, 2, 3, 4, 5, 6, 7, 8, 9),           //
    (9, 8, 7, 6, 5, 4, 3, 2, 1, 0),           //
    (A, B, C, D, E, F, G, H, I, J),           //
    (J, I, H, G, F, E, D, C, B, A),           //
    (Ae, Be, Ce, De, Ee, Fe, Ge, He, Ie, Je), //
    (Je, Ie, He, Ge, Fe, Ee, De, Ce, Be, Ae), //
    (T, T, T, T, T, T, T, T, T, T),           //
);
impl_tuple!(
    11,                                           //
    "(11-tuple)",                                 //
    tuple_arity_11,                               //
    (0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10),           //
    (10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0),           //
    (A, B, C, D, E, F, G, H, I, J, K),            //
    (K, J, I, H, G, F, E, D, C, B, A),            //
    (Ae, Be, Ce, De, Ee, Fe, Ge, He, Ie, Je, Ke), //
    (Ke, Je, Ie, He, Ge, Fe, Ee, De, Ce, Be, Ae), //
    (T, T, T, T, T, T, T, T, T, T, T),            //
);
impl_tuple!(
    12,                                               //
    "(12-tuple)",                                     //
    tuple_arity_12,                                   //
    (0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11),           //
    (11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0),           //
    (A, B, C, D, E, F, G, H, I, J, K, L),             //
    (L, K, J, I, H, G, F, E, D, C, B, A),             //
    (Ae, Be, Ce, De, Ee, Fe, Ge, He, Ie, Je, Ke, Le), //
    (Le, Ke, Je, Ie, He, Ge, Fe, Ee, De, Ce, Be, Ae), //
    (T, T, T, T, T, T, T, T, T, T, T, T),             //
);

delegate_value_encoding!(
    delegate from (GeneralGeneric<P>) to ((General,))
    for type ((A,)) including distinguished
    with generics (const P: u8, A)
);
delegate_value_encoding!(
    delegate from (GeneralGeneric<P>) to ((General, General))
    for type ((A, B)) including distinguished
    with generics (const P: u8, A, B)
);
delegate_value_encoding!(
    delegate from (GeneralGeneric<P>) to ((General, General, General))
    for type ((A, B, C)) including distinguished
    with generics (const P: u8, A, B, C)
);
delegate_value_encoding!(
    delegate from (GeneralGeneric<P>) to ((General, General, General, General))
    for type ((A, B, C, D)) including distinguished
    with generics (const P: u8, A, B, C, D)
);
delegate_value_encoding!(
    delegate from (GeneralGeneric<P>) to ((General, General, General, General, General))
    for type ((A, B, C, D, E)) including distinguished
    with generics (const P: u8, A, B, C, D, E)
);
delegate_value_encoding!(
    delegate from (GeneralGeneric<P>) to ((General, General, General, General, General, General))
    for type ((A, B, C, D, E, F)) including distinguished
    with generics (const P: u8, A, B, C, D, E, F)
);
delegate_value_encoding!(
    delegate from (GeneralGeneric<P>) to ((General, General, General, General, General, General,
                                           General))
    for type ((A, B, C, D, E, F, G)) including distinguished
    with generics (const P: u8, A, B, C, D, E, F, G)
);
delegate_value_encoding!(
    delegate from (GeneralGeneric<P>) to ((General, General, General, General, General, General,
                                           General, General))
    for type ((A, B, C, D, E, F, G, H)) including distinguished
    with generics (const P: u8, A, B, C, D, E, F, G, H)
);
delegate_value_encoding!(
    delegate from (GeneralGeneric<P>) to ((General, General, General, General, General, General,
                                           General, General, General))
    for type ((A, B, C, D, E, F, G, H, I)) including distinguished
    with generics (const P: u8, A, B, C, D, E, F, G, H, I)
);
delegate_value_encoding!(
    delegate from (GeneralGeneric<P>) to ((General, General, General, General, General, General,
                                           General, General, General, General))
    for type ((A, B, C, D, E, F, G, H, I, J)) including distinguished
    with generics (const P: u8, A, B, C, D, E, F, G, H, I, J)
);
delegate_value_encoding!(
    delegate from (GeneralGeneric<P>) to ((General, General, General, General, General, General,
                                           General, General, General, General, General))
    for type ((A, B, C, D, E, F, G, H, I, J, K)) including distinguished
    with generics (const P: u8, A, B, C, D, E, F, G, H, I, J, K)
);
delegate_value_encoding!(
    delegate from (GeneralGeneric<P>) to ((General, General, General, General, General, General,
                                           General, General, General, General, General, General))
    for type ((A, B, C, D, E, F, G, H, I, J, K, L)) including distinguished
    with generics (const P: u8, A, B, C, D, E, F, G, H, I, J, K, L)
);
