//! Protocol Buffers well-known wrapper types.
//!
//! This module provides implementations of `Message` for Rust standard library types which
//! correspond to a Protobuf well-known wrapper type. The remaining well-known types are defined in
//! the `bilrost-types` crate in order to avoid a cyclic dependency between `bilrost` and
//! `bilrost-build`.

use core::borrow::{Borrow, BorrowMut};
use core::convert::{AsMut, AsRef, From, Into};
use core::ops::{Deref, DerefMut};

use crate::bytes::{Buf, BufMut};

use crate::{
    encoding::{skip_field, Capped, DecodeContext, WireType},
    DecodeError, Message,
};

/// Newtype wrapper for `Vec<u8>` to act as a simple "bytes data" type in Bilrost. It transparently
/// wraps a Vec<u8>.
#[derive(Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Debug, Default)]
#[repr(transparent)]
#[cfg_attr(test, derive(proptest_derive::Arbitrary))]
pub struct Blob(Vec<u8>);

impl Blob {
    fn new(vec: Vec<u8>) -> Self {
        Self(vec)
    }

    fn into_inner(self) -> Vec<u8> {
        self.0
    }
}

impl Deref for Blob {
    type Target = Vec<u8>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Blob {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl AsRef<Vec<u8>> for Blob {
    fn as_ref(&self) -> &Vec<u8> {
        &self.0
    }
}

impl AsMut<Vec<u8>> for Blob {
    fn as_mut(&mut self) -> &mut Vec<u8> {
        &mut self.0
    }
}

impl Borrow<Vec<u8>> for Blob {
    fn borrow(&self) -> &Vec<u8> {
        &self.0
    }
}

impl BorrowMut<Vec<u8>> for Blob {
    fn borrow_mut(&mut self) -> &mut Vec<u8> {
        &mut self.0
    }
}

impl From<Vec<u8>> for Blob {
    fn from(value: Vec<u8>) -> Self {
        Blob::new(value)
    }
}

impl Into<Vec<u8>> for Blob {
    fn into(self) -> Vec<u8> {
        self.0
    }
}

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
