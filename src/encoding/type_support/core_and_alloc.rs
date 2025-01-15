use crate::encoding::value_traits::{
    empty_state_via_default, for_overwrite_via_default, TriviallyDistinguishedCollection,
};
use crate::encoding::{
    Collection, DistinguishedCollection, DistinguishedMapping, EmptyState, ForOverwrite, Mapping,
};
use crate::DecodeErrorKind::UnexpectedlyRepeated;
use crate::{Canonicity, DecodeErrorKind};
use alloc::borrow::{Cow, ToOwned};
use alloc::boxed::Box;
use alloc::collections::{btree_map, btree_set, BTreeMap, BTreeSet};
use alloc::string::String;
use alloc::vec::Vec;
use core::cmp::Ordering::{Equal, Greater, Less};

for_overwrite_via_default!(String);

impl EmptyState for String {
    #[inline]
    fn is_empty(&self) -> bool {
        Self::is_empty(self)
    }

    #[inline]
    fn clear(&mut self) {
        Self::clear(self)
    }
}

for_overwrite_via_default!(
    Cow<'a, T>,
    with generics ('a, T),
    with where clause (
        T: 'a + ?Sized + ToOwned,
        &'a T: ForOverwrite,
        T::Owned: ForOverwrite
    )
);

impl<'a, T> EmptyState for Cow<'a, T>
where
    Self: ForOverwrite,
    T: 'a + ?Sized + ToOwned,
    &'a T: EmptyState,
    T::Owned: EmptyState,
{
    #[inline]
    fn is_empty(&self) -> bool {
        match self {
            Cow::Borrowed(b) => b.is_empty(),
            Cow::Owned(o) => o.is_empty(),
        }
    }

    #[inline]
    fn clear(&mut self) {
        match self {
            Cow::Borrowed(_) => {
                *self = Cow::Owned(T::Owned::empty());
            }
            Cow::Owned(owned) => {
                owned.clear();
            }
        }
    }
}

impl<T> ForOverwrite for Option<T> {
    #[inline]
    fn for_overwrite() -> Self {
        None
    }
}

impl<T> EmptyState for Option<T> {
    #[inline]
    fn is_empty(&self) -> bool {
        self.is_none()
    }

    #[inline]
    fn clear(&mut self) {
        *self = None;
    }
}

impl<T> ForOverwrite for Box<T>
where
    T: ForOverwrite,
{
    #[inline(always)]
    fn for_overwrite() -> Self {
        Box::new(T::for_overwrite())
    }
}

impl<T> EmptyState for Box<T>
where
    T: EmptyState,
{
    #[inline]
    fn empty() -> Self {
        Self::new(T::empty())
    }

    #[inline]
    fn is_empty(&self) -> bool {
        self.as_ref().is_empty()
    }

    #[inline]
    fn clear(&mut self) {
        self.as_mut().clear()
    }
}

empty_state_via_default!(core::time::Duration);

for_overwrite_via_default!(Vec<T>, with generics (T));

impl<T> EmptyState for Vec<T> {
    #[inline]
    fn is_empty(&self) -> bool {
        Self::is_empty(self)
    }

    #[inline]
    fn clear(&mut self) {
        Self::clear(self)
    }
}

impl<T> Collection for Vec<T> {
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
        Vec::len(self)
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
        Vec::push(self, item);
        Ok(())
    }
}

impl<T> TriviallyDistinguishedCollection for Vec<T> {}

impl<T> Collection for Cow<'_, [T]>
where
    T: Clone,
{
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
        <[T]>::len(self)
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
    fn insert(&mut self, item: Self::Item) -> Result<(), DecodeErrorKind> {
        self.to_mut().push(item);
        Ok(())
    }
}

impl<T> TriviallyDistinguishedCollection for Cow<'_, [T]> where T: Clone {}

for_overwrite_via_default!(BTreeSet<T>, with generics(T));

impl<T> EmptyState for BTreeSet<T> {
    #[inline]
    fn is_empty(&self) -> bool {
        Self::is_empty(self)
    }

    #[inline]
    fn clear(&mut self) {
        Self::clear(self)
    }
}

impl<T> Collection for BTreeSet<T>
where
    T: Ord,
{
    type Item = T;
    type RefIter<'a>
        = btree_set::Iter<'a, T>
    where
        Self::Item: 'a,
        Self: 'a;
    type ReverseIter<'a>
        = core::iter::Rev<btree_set::Iter<'a, T>>
    where
        Self::Item: 'a,
        Self: 'a;

    #[inline]
    fn len(&self) -> usize {
        BTreeSet::len(self)
    }

    #[inline]
    fn iter(&self) -> Self::RefIter<'_> {
        BTreeSet::iter(self)
    }

    #[inline]
    fn reversed(&self) -> Self::ReverseIter<'_> {
        BTreeSet::iter(self).rev()
    }

    #[inline]
    fn insert(&mut self, item: Self::Item) -> Result<(), DecodeErrorKind> {
        if !BTreeSet::insert(self, item) {
            return Err(UnexpectedlyRepeated);
        }
        Ok(())
    }
}

impl<T> DistinguishedCollection for BTreeSet<T>
where
    T: Ord,
{
    #[inline]
    fn insert_distinguished(&mut self, item: Self::Item) -> Result<Canonicity, DecodeErrorKind> {
        // MSRV: can't use .last()
        match Some(&item).cmp(&self.iter().next_back()) {
            Less => {
                if self.insert(item) {
                    Ok(Canonicity::NotCanonical)
                } else {
                    Err(UnexpectedlyRepeated)
                }
            }
            Equal => Err(UnexpectedlyRepeated),
            Greater => {
                self.insert(item);
                Ok(Canonicity::Canonical)
            }
        }
    }
}

for_overwrite_via_default!(BTreeMap<K, V>, with generics (K, V));

impl<K, V> EmptyState for BTreeMap<K, V> {
    #[inline]
    fn is_empty(&self) -> bool {
        Self::is_empty(self)
    }

    #[inline]
    fn clear(&mut self) {
        Self::clear(self)
    }
}

impl<K, V> Mapping for BTreeMap<K, V>
where
    K: Ord,
{
    type Key = K;
    type Value = V;
    type RefIter<'a>
        = btree_map::Iter<'a, K, V>
    where
        K: 'a,
        V: 'a,
        Self: 'a;
    type ReverseIter<'a>
        = core::iter::Rev<btree_map::Iter<'a, K, V>>
    where
        K: 'a,
        V: 'a,
        Self: 'a;

    #[inline]
    fn len(&self) -> usize {
        BTreeMap::len(self)
    }

    #[inline]
    fn iter(&self) -> Self::RefIter<'_> {
        BTreeMap::iter(self)
    }

    #[inline]
    fn reversed(&self) -> Self::ReverseIter<'_> {
        BTreeMap::iter(self).rev()
    }

    #[inline]
    fn insert(&mut self, key: K, value: V) -> Result<(), DecodeErrorKind> {
        if let btree_map::Entry::Vacant(entry) = self.entry(key) {
            entry.insert(value);
            Ok(())
        } else {
            Err(UnexpectedlyRepeated)
        }
    }
}

impl<K, V> DistinguishedMapping for BTreeMap<K, V>
where
    Self: Eq,
    K: Ord,
{
    #[inline]
    fn insert_distinguished(
        &mut self,
        key: Self::Key,
        value: Self::Value,
    ) -> Result<Canonicity, DecodeErrorKind> {
        match Some(&key).cmp(&self.keys().next_back()) {
            Less => {
                if self.insert(key, value).is_none() {
                    Ok(Canonicity::NotCanonical)
                } else {
                    Err(UnexpectedlyRepeated)
                }
            }
            Equal => Err(UnexpectedlyRepeated),
            Greater => {
                self.insert(key, value);
                Ok(Canonicity::Canonical)
            }
        }
    }
}
