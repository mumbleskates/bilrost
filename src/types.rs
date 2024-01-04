use core::borrow::{Borrow, BorrowMut};
use core::convert::{AsMut, AsRef, From};
use core::ops::{Deref, DerefMut};

use bytes::{Buf, BufMut};

use crate::{
    encoding::{skip_field, Capped, DecodeContext, WireType},
    DecodeError, Message,
};

/// Newtype wrapper to act as a simple "bytes data" type in Bilrost. It transparently wraps a
/// `Vec<u8>` and is fully supported by the `General` encoder.
///
/// To use `Vec<u8>` directly, use the `VecBlob` encoder.
#[derive(Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Debug, Default)]
#[repr(transparent)]
pub struct Blob(Vec<u8>);

impl Blob {
    pub fn new(vec: Vec<u8>) -> Self {
        Self(vec)
    }

    pub fn into_inner(self) -> Vec<u8> {
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

impl From<Blob> for Vec<u8> {
    fn from(value: Blob) -> Self {
        value.0
    }
}

#[cfg(test)]
impl proptest::arbitrary::Arbitrary for Blob {
    type Parameters = <Vec<u8> as proptest::arbitrary::Arbitrary>::Parameters;
    fn arbitrary_with(top: Self::Parameters) -> Self::Strategy {
        {
            proptest::strategy::Strategy::prop_map(
                proptest::arbitrary::any_with::<Vec<u8>>(top),
                Blob::new,
            )
        }
    }
    type Strategy = proptest::strategy::Map<
        <Vec<u8> as proptest::arbitrary::Arbitrary>::Strategy,
        fn(Vec<u8>) -> Self,
    >;
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
