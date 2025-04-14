use crate::encoding::plain_bytes::plain_bytes_vec_impl;
use crate::encoding::value_traits::{for_overwrite_via_default, TriviallyDistinguishedCollection};
use crate::encoding::{
    delegate_encoding, delegate_value_encoding, Collection, EmptyState, General, GeneralPacked,
    Packed, Unpacked,
};
use crate::DecodeErrorKind::InvalidValue;
use crate::{DecodeError, DecodeErrorKind};
use bytes::Buf;

for_overwrite_via_default!(arrayvec::ArrayVec<T, N>, with generics (T, const N: usize));

impl<T, const N: usize> EmptyState for arrayvec::ArrayVec<T, N> {
    #[inline]
    fn is_empty(&self) -> bool {
        arrayvec::ArrayVec::is_empty(self)
    }

    #[inline]
    fn clear(&mut self) {
        arrayvec::ArrayVec::clear(self)
    }
}

impl<T, const N: usize> Collection for arrayvec::ArrayVec<T, N> {
    type Item = T;
    type RefIter<'a>
        = core::slice::Iter<'a, T>
    where
        T: 'a,
        Self: 'a;
    type ReverseIter<'a>
        = core::iter::Rev<core::slice::Iter<'a, T>>
    where
        Self::Item: 'a,
        Self: 'a;

    #[inline]
    fn len(&self) -> usize {
        arrayvec::ArrayVec::len(self)
    }

    #[inline]
    fn iter(&self) -> Self::RefIter<'_> {
        self.as_slice().iter()
    }

    #[inline]
    fn reversed(&self) -> Self::ReverseIter<'_> {
        self.as_slice().iter().rev()
    }

    #[inline]
    fn insert(&mut self, item: Self::Item) -> Result<(), DecodeErrorKind> {
        self.try_push(item).map_err(|_| InvalidValue)
    }
}

impl<T, const N: usize> TriviallyDistinguishedCollection for arrayvec::ArrayVec<T, N> {}

delegate_encoding!(
    delegate from (General) to (Unpacked)
    for type (arrayvec::ArrayVec<T, N>) including distinguished
    with generics (T, const N: usize)
);
delegate_value_encoding!(
    delegate from (GeneralPacked) to (Packed)
    for type (arrayvec::ArrayVec<T, N>) including distinguished
    with generics (T, const N: usize)
);

plain_bytes_vec_impl!(
    arrayvec::ArrayVec<u8, N>,
    value,
    buf,
    chunk,
    if buf.remaining() > N {
        return Err(DecodeError::new(InvalidValue));
    },
    value.extend(chunk.iter().cloned()),
    with generics (const N: usize)
);

#[cfg(test)]
mod test {
    crate::encoding::plain_bytes::test::check_bounded!(arrayvec::ArrayVec<u8, 8>, 8);
}
