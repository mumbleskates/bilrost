//! These macros provide a (fairly low-quality) shell to emulate derived message types without doing
//! absolutely everything by hand, without an impossible dependency on the derive crate, and without
//! being forced to use tuple types which might not be desirable. The implementations here are based
//! on the code in the tuple encoder macro; these are mostly useful for making proxies.

/// Fields must be listed in forward order here, and the targets should be &const.
#[allow(unused_macros)]
macro_rules! underived_encode {
    (
        $name:ident {
            $($tag:literal: $encoder:ty => $field_name:ident: $target:expr),* $(,)?
        },
        $buf:ident
    ) => {
        {
            use crate::encoding::{encode_varint, Encoder, RuntimeTagMeasurer, TagWriter};
            let buf = $buf;
            let tm = &mut RuntimeTagMeasurer::new();
            let message_len = 0usize $(+ Encoder::<$encoder>::encoded_len($tag, $target, tm))*;
            encode_varint(message_len as u64, buf);
            let tw = &mut TagWriter::new();
            $(Encoder::<$encoder>::encode($tag, $target, buf, tw);)*
        }
    }
}
#[allow(unused_imports)]
pub(crate) use underived_encode;

/// Fields must be listed in reverse order here, and the targets should be &const.
#[allow(unused_macros)]
macro_rules! underived_prepend {
    (
        $name:ident {
            $($tag:literal: $encoder:ty => $field_name:ident: $target:expr),* $(,)?
        },
        $buf:ident
    ) => {
        {
            use crate::encoding::{prepend_varint, Encoder, TagRevWriter};
            let buf = $buf;
            let end = buf.remaining();
            let tw = &mut TagRevWriter::new();
            $(Encoder::<$encoder>::prepend_encode($tag, $target, buf, tw);)*
            tw.finalize(buf);
            prepend_varint((buf.remaining() - end) as u64, buf);
        }
    }
}
#[allow(unused_imports)]
pub(crate) use underived_prepend;

/// Fields must be listed in forward order here, and the targets should be &const.
#[allow(unused_macros)]
macro_rules! underived_encoded_len {
    (
        $name:ident {
            $($tag:literal: $encoder:ty => $field_name:ident: $target:expr),* $(,)?
        }
    ) => {
        {
            use crate::encoding::{encoded_len_varint, Encoder, RuntimeTagMeasurer};
            let tm = &mut RuntimeTagMeasurer::new();
            let message_len = 0usize $(+ Encoder::<$encoder>::encoded_len($tag, $target, tm))*;
            encoded_len_varint(message_len as u64) + message_len
        }
    }
}
#[allow(unused_imports)]
pub(crate) use underived_encoded_len;

/// Fields can technically be listed in any order here; targets must be &mut.
#[allow(unused_macros)]
macro_rules! underived_decode {
    (
        $name:ident {
            $($tag:literal: $encoder:ty => $field_name:ident: $target:expr),* $(,)?
        },
        $buf:ident,
        $ctx:ident
    ) => {
        {
            use crate::encoding::{skip_field, Decoder, TagReader};
            let mut buf = $buf.take_length_delimited()?;
            let ctx = $ctx;
            ctx.limit_reached()?;
            let ctx = ctx.enter_recursion();
            let tr = &mut TagReader::new();
            let mut last_tag = None::<u32>;
            while buf.has_remaining()? {
                let (tag, wire_type) = tr.decode_key(buf.lend())?;
                let duplicated = last_tag == Some(tag);
                last_tag = Some(tag);
                match tag {
                    $($tag => {
                        Decoder::<$encoder>::decode(
                            wire_type,
                            duplicated,
                            $target,
                            buf.lend(),
                            ctx.clone(),
                        ).map_err(|mut error| {
                            error.push(stringify!($name), stringify!($field_name));
                            error
                        })?
                    })*
                    _ => skip_field(wire_type, buf.lend())?,
                }
            }
            Result::<(), crate::DecodeError>::Ok(())
        }
    };
}
#[allow(unused_imports)]
pub(crate) use underived_decode;

/// Fields can technically be listed in any order here; targets must be &mut.
#[allow(unused_macros)]
macro_rules! underived_decode_distinguished {
    (
        $name:ident {
            $($tag:literal: $encoder:ty => $field_name:ident: $target:expr),* $(,)?
        },
        $buf:ident,
        $ctx:ident
    ) => {
        {
            use crate::encoding::{skip_field, Canonicity, DistinguishedDecoder, TagReader};
            let mut buf = $buf.take_length_delimited()?;
            let ctx = $ctx;
            if !ALLOW_EMPTY && buf.remaining_before_cap() == 0 {
                Result::<_, crate::DecodeError>::Ok(Canonicity::NotCanonical)
            } else {
                ctx.limit_reached()?;
                let mut canon = Canonicity::Canonical;
                let ctx = ctx.enter_recursion();
                let tr = &mut TagReader::new();
                let mut last_tag = None::<u32>;
                while buf.has_remaining()? {
                    let (tag, wire_type) = tr.decode_key(buf.lend())?;
                    let duplicated = last_tag == Some(tag);
                    last_tag = Some(tag);
                    match tag {
                        $($tag => {
                            canon.update(
                                DistinguishedDecoder::<$encoder>::decode_distinguished(
                                    wire_type,
                                    duplicated,
                                    $target,
                                    buf.lend(),
                                    ctx.clone(),
                                ).map_err(|mut error| {
                                    error.push(stringify!($name), stringify!($field_name));
                                    error
                                })?,
                            );
                        })*
                        _ => {
                            ctx.update(&mut canon, Canonicity::HasExtensions)?;
                            skip_field(wire_type, buf.lend())?;
                        },
                    }
                }
                Result::<_, crate::DecodeError>::Ok(canon)
            }
        }
    };
}
#[allow(unused_imports)]
pub(crate) use underived_decode_distinguished;
