use crate::encoding::plain_bytes::plain_bytes_vec_impl;
use crate::encoding::value_traits::{for_overwrite_via_default, TriviallyDistinguishedCollection};
use crate::encoding::{
    delegate_encoding, delegate_value_encoding, Collection, EmptyState, General, GeneralPacked,
    Packed, Unpacked,
};
use crate::DecodeErrorKind::InvalidValue;
use crate::{DecodeError, DecodeErrorKind};
use bytes::Buf;

for_overwrite_via_default!(tinyvec::ArrayVec<A>,
    with generics (A),
    with where clause (A: tinyvec::Array));

impl<A: tinyvec::Array> EmptyState<(), tinyvec::ArrayVec<A>> for () {
    #[inline]
    fn is_empty(val: &tinyvec::ArrayVec<A>) -> bool {
        val.is_empty()
    }

    #[inline]
    fn clear(val: &mut tinyvec::ArrayVec<A>) {
        val.clear();
    }
}

impl<T, A: tinyvec::Array<Item = T>> Collection for tinyvec::ArrayVec<A> {
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

    const BOUNDS: core::ops::RangeToInclusive<Option<usize>> = ..=Some(A::CAPACITY);

    #[inline]
    fn len(&self) -> usize {
        tinyvec::ArrayVec::len(self)
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
        match self.try_push(item) {
            None => Ok(()),
            Some(_) => Err(InvalidValue),
        }
    }
}

impl<A: tinyvec::Array> TriviallyDistinguishedCollection for tinyvec::ArrayVec<A> {}

for_overwrite_via_default!(tinyvec::TinyVec<A>,
    with generics (A),
    with where clause (A: tinyvec::Array));

impl<A: tinyvec::Array> EmptyState<(), tinyvec::TinyVec<A>> for () {
    #[inline]
    fn is_empty(val: &tinyvec::TinyVec<A>) -> bool {
        val.is_empty()
    }

    #[inline]
    fn clear(val: &mut tinyvec::TinyVec<A>) {
        val.clear();
    }
}

impl<T, A: tinyvec::Array<Item = T>> Collection for tinyvec::TinyVec<A> {
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
        tinyvec::TinyVec::len(self)
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
        tinyvec::TinyVec::push(self, item);
        Ok(())
    }
}

impl<A: tinyvec::Array> TriviallyDistinguishedCollection for tinyvec::TinyVec<A> {}

delegate_encoding!(
    delegate from (General) to (Unpacked)
    for type (tinyvec::ArrayVec<A>)
    including distinguished
    with where clause (A: tinyvec::Array<Item = T>)
    with generics (T, A)
);
delegate_encoding!(
    delegate from (General) to (Unpacked)
    for type (tinyvec::TinyVec<A>)
    including distinguished
    with where clause (A: tinyvec::Array<Item = T>)
    with generics (T, A)
);
delegate_value_encoding!(
    delegate from (GeneralPacked) to (Packed)
    for type (tinyvec::ArrayVec<A>)
    including distinguished
    with where clause for relaxed (A: tinyvec::Array<Item = T>)
    with generics (T, A)
);
delegate_value_encoding!(
    delegate from (GeneralPacked) to (Packed)
    for type (tinyvec::TinyVec<A>)
    including distinguished
    with where clause for relaxed (A: tinyvec::Array<Item = T>)
    with generics (T, A)
);

plain_bytes_vec_impl!(
    tinyvec::TinyVec<A>,
    value,
    buf,
    chunk,
    value.reserve(buf.remaining()),
    value.extend_from_slice(chunk),
    with generics (A: tinyvec::Array<Item = u8>)
);
plain_bytes_vec_impl!(
    tinyvec::ArrayVec<A>,
    value,
    buf,
    chunk,
    if buf.remaining() > A::CAPACITY {
        return Err(DecodeError::new(InvalidValue));
    },
    value.extend_from_slice(chunk),
    limit <A as tinyvec::Array>::CAPACITY,
    with generics (A: tinyvec::Array<Item = u8>)
);

#[cfg(test)]
mod test {
    crate::encoding::plain_bytes::test::check_bounded!(tinyvec::ArrayVec<[u8; 8]>, 8);
    crate::encoding::plain_bytes::test::check_unbounded!(tinyvec::TinyVec<[u8; 8]>);
}
