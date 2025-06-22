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

impl EmptyState<(), String> for () {
    #[inline]
    fn is_empty(val: &String) -> bool {
        val.is_empty()
    }

    #[inline]
    fn clear(val: &mut String) {
        val.clear();
    }
}

for_overwrite_via_default!(
    Cow<'a, T>,
    with generics ('a, T),
    with where clause (
        T: 'a + ?Sized + ToOwned,
        T::Owned: Default,
        (): ForOverwrite<(), &'a T> + ForOverwrite<(), T::Owned>
    )
);

impl<'a, T> EmptyState<(), Cow<'a, T>> for ()
where
    T: 'a + ?Sized + ToOwned,
    (): ForOverwrite<(), Cow<'a, T>> + EmptyState<(), &'a T> + EmptyState<(), T::Owned>,
{
    #[inline]
    fn is_empty(val: &Cow<'a, T>) -> bool {
        match val {
            Cow::Borrowed(b) => <() as EmptyState<(), _>>::is_empty(b),
            Cow::Owned(o) => <() as EmptyState<(), _>>::is_empty(o),
        }
    }

    #[inline]
    fn clear(val: &mut Cow<'a, T>) {
        match val {
            Cow::Borrowed(_) => {
                *val = Cow::Owned(<() as EmptyState<(), T::Owned>>::empty());
            }
            Cow::Owned(owned) => {
                <() as EmptyState<(), _>>::clear(owned);
            }
        }
    }
}

impl<T> ForOverwrite<(), Box<T>> for ()
where
    (): ForOverwrite<(), T>,
{
    #[inline(always)]
    fn for_overwrite() -> Box<T> {
        Box::new(<() as ForOverwrite<(), T>>::for_overwrite())
    }
}

impl<T> EmptyState<(), Box<T>> for ()
where
    (): EmptyState<(), T>,
{
    #[inline]
    fn empty() -> Box<T> {
        Box::new(<() as EmptyState<(), T>>::empty())
    }

    #[inline]
    fn is_empty(val: &Box<T>) -> bool {
        <() as EmptyState<(), T>>::is_empty(val.as_ref())
    }

    #[inline]
    fn clear(val: &mut Box<T>) {
        <() as EmptyState<(), T>>::clear(val.as_mut())
    }
}

empty_state_via_default!(core::time::Duration);

for_overwrite_via_default!(Vec<T>, with generics (T));

impl<T> EmptyState<(), Vec<T>> for () {
    #[inline]
    fn is_empty(val: &Vec<T>) -> bool {
        val.is_empty()
    }

    #[inline]
    fn clear(val: &mut Vec<T>) {
        val.clear();
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

impl<T> EmptyState<(), BTreeSet<T>> for () {
    #[inline]
    fn is_empty(val: &BTreeSet<T>) -> bool {
        val.is_empty()
    }

    #[inline]
    fn clear(val: &mut BTreeSet<T>) {
        val.clear();
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
        #[cfg(not(rustc_1_66))]
        let last = &self.iter().next_back();
        #[cfg(rustc_1_66)]
        #[allow(clippy::incompatible_msrv)]
        let last = self.last();
        match Some(&item).cmp(&last) {
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

impl<K, V> EmptyState<(), BTreeMap<K, V>> for () {
    #[inline]
    fn is_empty(val: &BTreeMap<K, V>) -> bool {
        val.is_empty()
    }

    #[inline]
    fn clear(val: &mut BTreeMap<K, V>) {
        val.clear();
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
        #[cfg(not(rustc_1_66))]
        let last_key = &self.keys().next_back();
        #[cfg(rustc_1_66)]
        #[allow(clippy::incompatible_msrv)]
        let last_key = self.last_key_value().map(|(k, ..)| k);
        match Some(&key).cmp(&last_key) {
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
