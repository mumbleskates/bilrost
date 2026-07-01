use crate::encoding::plain_bytes::plain_bytes_vec_impl;
use crate::encoding::value_traits::{for_overwrite_via_default, TriviallyDistinguishedCollection};
use crate::encoding::{
    delegate_encoding, delegate_value_encoding, Collection, EmptyState, General, GeneralPacked,
    Packed, Unpacked,
};
use crate::DecodeErrorKind;
use bytes::Buf;

for_overwrite_via_default!(thin_vec::ThinVec<T>, with generics (T));

impl<T> EmptyState<(), thin_vec::ThinVec<T>> for () {
    #[inline]
    fn is_empty(val: &thin_vec::ThinVec<T>) -> bool {
        val.is_empty()
    }

    #[inline]
    fn clear(val: &mut thin_vec::ThinVec<T>) {
        val.clear();
    }
}

impl<T> Collection for thin_vec::ThinVec<T> {
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
        thin_vec::ThinVec::len(self)
    }

    #[inline]
    fn iter(&self) -> Self::RefIter<'_> {
        <[T]>::iter(self)
    }

    #[inline]
    fn reversed(&self) -> Self::ReverseIter<'_> {
        <[T]>::iter(self).rev()
    }

    #[inline]
    fn insert(&mut self, item: T) -> Result<(), DecodeErrorKind> {
        thin_vec::ThinVec::push(self, item);
        Ok(())
    }
}

impl<T> TriviallyDistinguishedCollection for thin_vec::ThinVec<T> {}

delegate_encoding!(
    delegate from (General) to (Unpacked)
    for type (thin_vec::ThinVec<T>)
    including distinguished
    with generics (T)
);
delegate_value_encoding!(
    delegate from (GeneralPacked) to (Packed)
    for type (thin_vec::ThinVec<T>)
    including distinguished
    with generics (T)
);

plain_bytes_vec_impl!(
    thin_vec::ThinVec<u8>,
    value,
    buf,
    chunk,
    value.reserve(buf.remaining()),
    value.extend_from_slice(chunk)
);

#[cfg(test)]
mod test {
    crate::encoding::plain_bytes::test::check_unbounded!(thin_vec::ThinVec<u8>);
}
