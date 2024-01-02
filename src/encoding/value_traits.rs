use alloc::collections::BTreeSet;
use alloc::vec::Vec;
#[cfg(feature = "std")]
use core::hash::Hash;
#[cfg(feature = "std")]
use std::collections::HashSet;

/// Trait for cheaply producing a new value that will always be overwritten, rather than a value
/// that really serves as a zero-valued default. This is implemented for types that can be present
/// optionally (in Option or Vec, for instance) but don't have a Default value, such as
/// enumerations.
///
/// API design note:
/// Philosophically it would be preferable to make decoding values produce owned values rather than
/// writing them into a &mut T, but this is currently not possible as reading in values may happen
/// multiple times for the same destination field (such as Vec<T>, or more challengingly Oneofs).
// TODO(widders): if we change unpacked repeated to greedily decode every available field with the
//  same tag instead of waiting for them to be provided, we gain two major things: we can return
//  decoded types by value instead of always needing to write them into a &mut, and we can do a
//  better job of complaining when we decode repeated fields with mixed packedness.
pub trait NewForOverwrite {
    /// Produces a new Self value to be overwritten.
    fn new_for_overwrite() -> Self;
}
impl<T> NewForOverwrite for T
where
    T: Default,
{
    #[inline]
    fn new_for_overwrite() -> Self {
        Self::default()
    }
}

/// Trait for containers that store multiple items, such as `Vec` and `HashSet`
pub trait Collection: Default {
    type Item;
    type RefIter<'a>: ExactSizeIterator<Item = &'a Self::Item>
    where
        Self::Item: 'a,
        Self: 'a;

    fn len(&self) -> usize;
    #[inline]
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    fn iter(&self) -> Self::RefIter<'_>;
    fn insert(&mut self, item: Self::Item) -> Result<(), &'static str>;
}

/// Trait for collections that store multiple items and have a distinguished representation, such as
/// `Vec` and `BTreeSet`. Returns an error if the values are inserted in the wrong order.
pub trait DistinguishedCollection: Collection + Eq {
    type ReverseIter<'a>: Iterator<Item = &'a Self::Item>
    where
        Self::Item: 'a,
        Self: 'a;

    fn reversed(&self) -> Self::ReverseIter<'_>;
    fn insert_distinguished(&mut self, item: Self::Item) -> Result<(), &'static str>;
}

impl<T> Collection for Vec<T> {
    type Item = T;
    type RefIter<'a> = core::slice::Iter<'a, T>
    where
        T: 'a,
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
    fn insert(&mut self, item: T) -> Result<(), &'static str> {
        Vec::push(self, item);
        Ok(())
    }
}

impl<T> DistinguishedCollection for Vec<T>
where
    T: Eq,
{
    type ReverseIter<'a> = core::iter::Rev<core::slice::Iter<'a, T>>
        where
            Self::Item: 'a,
            Self: 'a;

    #[inline]
    fn reversed(&self) -> Self::ReverseIter<'_> {
        <[T]>::iter(self).rev()
    }

    #[inline]
    fn insert_distinguished(&mut self, item: Self::Item) -> Result<(), &'static str> {
        Vec::push(self, item);
        Ok(())
    }
}

impl<T> Collection for BTreeSet<T>
where
    Self: Default,
    T: Ord,
{
    type Item = T;
    type RefIter<'a> = alloc::collections::btree_set::Iter<'a, T>
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
    fn insert(&mut self, item: Self::Item) -> Result<(), &'static str> {
        if !BTreeSet::insert(self, item) {
            return Err("values are not unique");
        }
        Ok(())
    }
}

impl<T> DistinguishedCollection for BTreeSet<T>
where
    Self: Eq,
    T: Ord,
{
    type ReverseIter<'a> = core::iter::Rev<alloc::collections::btree_set::Iter<'a, T>>
    where
        Self::Item: 'a,
        Self: 'a;

    #[inline]
    fn reversed(&self) -> Self::ReverseIter<'_> {
        BTreeSet::iter(self).rev()
    }

    #[inline]
    fn insert_distinguished(&mut self, item: Self::Item) -> Result<(), &'static str> {
        if Some(&item) <= self.last() {
            return Err("values are not unique and ascending");
        }
        self.insert(item);
        Ok(())
    }
}

#[cfg(feature = "std")]
impl<T> Collection for HashSet<T>
where
    Self: Default,
    T: Hash + Eq,
{
    type Item = T;
    type RefIter<'a> = std::collections::hash_set::Iter<'a, T>
    where
        Self::Item: 'a,
        Self: 'a;

    #[inline]
    fn len(&self) -> usize {
        HashSet::len(self)
    }

    #[inline]
    fn iter(&self) -> Self::RefIter<'_> {
        HashSet::iter(self)
    }

    #[inline]
    fn insert(&mut self, item: Self::Item) -> Result<(), &'static str> {
        if !HashSet::insert(self, item) {
            return Err("values are not unique");
        }
        Ok(())
    }
}
