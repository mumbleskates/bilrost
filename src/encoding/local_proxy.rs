use crate::encoding::value_traits::{
    Collection, EmptyState, ForOverwrite, TriviallyDistinguishedCollection,
};
use crate::Canonicity::{Canonical, NotCanonical};
use crate::{Canonicity, DecodeErrorKind};
use core::ops::Deref;

/// This type is a locally implemented stand-in for types like tinyvec::ArrayVec with bare-minimum
/// functionality to assist encoding some third party types.
#[derive(Debug, Clone)]
pub(crate) struct LocalProxy<T, const N: usize> {
    arr: [T; N],
    size: usize,
}

impl<T: EmptyState, const N: usize> Deref for LocalProxy<T, N> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        // SAFETY: self.size is only ever initialized to zero or to N. it is only ever increased in
        // Collection::insert, which always checks that it is not yet equal to N. Therefore there
        // should be no way to create a LocalProxy value with an illegal size field, and we do not
        // have to perform a bounds check here.
        unsafe { self.arr.get_unchecked(..self.size) }
    }
}

impl<T: EmptyState, const N: usize> LocalProxy<T, N> {
    /// Creates a new, empty array-list proxy.
    pub fn new_empty() -> Self {
        <_ as EmptyState>::empty()
    }

    /// Creates a new value that only contains the values in the given backing array that are not
    /// contiguously empty at the end of the array. This is equivalent to creating a new empty proxy
    /// and then inserting each value in order until all remaining values that would be inserted are
    /// empty.
    pub fn new_without_empty_suffix(arr: [T; N]) -> Self {
        let mut size = N;
        for item in arr.iter().rev() {
            if item.is_empty() {
                size -= 1;
            } else {
                break;
            }
        }
        Self { arr, size }
    }

    /// Returns the backing array for this proxy.
    pub fn into_inner(self) -> [T; N] {
        self.arr
    }

    /// Returns the backing array for this proxy, returning NotCanonical if values that were decoded
    /// or inserted contained extraneous empty items at the end.
    ///
    /// For example: when decoding into an empty LocalProxy<i64, 3> value, the backing array will
    /// always be an [i64; 3]. If a single value "5" is decoded, then the inner value will be
    /// [5, 0, 0] and the encoding was canonical. If the value was decoded as two values "5" and "0"
    /// then the backing array still contains [5, 0, 0] but the latter decoded value wouldn't have
    /// been encoded if we were using new_without_empty_suffix, and thus isn't canonical.
    pub fn into_inner_distinguished(self) -> ([T; N], Canonicity) {
        // MSRV: this could be is_some_and(..)
        let canon = if matches!(self.reversed().next(), Some(last_item) if last_item.is_empty()) {
            NotCanonical
        } else {
            Canonical
        };
        (self.arr, canon)
    }
}

impl<T: EmptyState + PartialEq, const N: usize> PartialEq for LocalProxy<T, N> {
    fn eq(&self, other: &Self) -> bool {
        **self == **other
    }
}

impl<T: EmptyState + Eq, const N: usize> Eq for LocalProxy<T, N> {}

impl<T: EmptyState, const N: usize> ForOverwrite for LocalProxy<T, N> {
    fn for_overwrite() -> Self {
        Self {
            arr: <_ as EmptyState>::empty(),
            size: 0,
        }
    }
}

impl<T: EmptyState, const N: usize> EmptyState for LocalProxy<T, N> {
    fn is_empty(&self) -> bool {
        self.size == 0
    }

    fn clear(&mut self) {
        self.size = 0;
    }
}

impl<T: EmptyState, const N: usize> Collection for LocalProxy<T, N> {
    type Item = T;
    type RefIter<'a>
        = core::slice::Iter<'a, T>
    where
        Self::Item: 'a,
        Self: 'a;
    type ReverseIter<'a>
        = core::iter::Rev<core::slice::Iter<'a, T>>
    where
        Self::Item: 'a,
        Self: 'a;

    fn len(&self) -> usize {
        self.size
    }

    fn iter(&self) -> Self::RefIter<'_> {
        self.deref().iter()
    }

    fn reversed(&self) -> Self::ReverseIter<'_> {
        self.deref().iter().rev()
    }

    fn insert(&mut self, item: Self::Item) -> Result<(), DecodeErrorKind> {
        if self.size == N {
            return Err(DecodeErrorKind::InvalidValue);
        }
        self.arr[self.size] = item;
        self.size += 1;
        Ok(())
    }
}

impl<T: EmptyState, const N: usize> TriviallyDistinguishedCollection for LocalProxy<T, N> {}
