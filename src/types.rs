//! Protocol Buffers well-known wrapper types.
//!
//! This module provides implementations of `Message` for Rust standard library types which
//! correspond to a Protobuf well-known wrapper type. The remaining well-known types are defined in
//! the `bilrost-types` crate in order to avoid a cyclic dependency between `bilrost` and
//! `bilrost-build`.

use ::bytes::{Buf, BufMut};

use crate::{
    encoding::{skip_field, Capped, DecodeContext, WireType},
    DecodeError, Message,
};

impl Message for () {
    fn encode_raw<B: BufMut>(&self, _buf: &mut B) {}

    fn merge_field<B: Buf>(
        &mut self,
        _tag: u32,
        wire_type: WireType,
        _duplicated: bool,
        buf: &mut Capped<B>,
        _ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        skip_field(wire_type, buf)
    }

    fn encoded_len(&self) -> usize {
        0
    }

    fn clear(&mut self) {}
}
