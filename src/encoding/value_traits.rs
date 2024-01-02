use alloc::collections::BTreeSet;
use alloc::vec::Vec;
use core::iter::Extend;
#[cfg(feature = "std")]
use std::collections::HashSet;

/// Trait for containers that store their values in the inserted order, like `Vec`
pub trait Veclike: Extend<Self::Item> {
    type Item;
    type Iter<'a>: Iterator<Item = &'a Self::Item>
    where
        Self::Item: 'a,
        Self: 'a;

    fn len(&self) -> usize;
    fn is_empty(&self) -> bool;
    fn iter(&self) -> Self::Iter<'_>;
    fn push(&mut self, item: Self::Item);
}

/// Trait for set containers.
pub trait Set: Default {
    type Item;
    type Iter<'a>: Iterator<Item = &'a Self::Item>
    where
        Self::Item: 'a,
        Self: 'a;

    fn len(&self) -> usize;
    fn is_empty(&self) -> bool;
    fn iter(&self) -> Self::Iter<'_>;
    fn insert(&mut self, item: Self::Item) -> bool;
}

/// Trait for set containers that store and iterate their items in sorted order.
pub trait DistinguishedSet: Set + Eq
where
    <Self as Set>::Item: Ord,
    for<'a> <Self as Set>::Iter<'a>: DoubleEndedIterator,
{
    fn last(&self) -> Option<&Self::Item>;
}

impl<T> Veclike for Vec<T> {
    type Item = T;
    type Iter<'a> = core::slice::Iter<'a, T>
    where
        T: 'a,
        Self: 'a;

    #[inline]
    fn len(&self) -> usize {
        Vec::len(self)
    }

    #[inline]
    fn is_empty(&self) -> bool {
        Vec::is_empty(self)
    }

    #[inline]
    fn iter(&self) -> Self::Iter<'_> {
        <[T]>::iter(self)
    }

    #[inline]
    fn push(&mut self, item: T) {
        Vec::push(self, item)
    }
}

impl<T> Set for BTreeSet<T>
where
    Self: Default,
{
    type Item = T;
    type Iter<'a> = alloc::collections::btree_set::Iter<'a, T>
    where
        Self::Item: 'a,
        Self: 'a;

    fn len(&self) -> usize {
        BTreeSet::len(self)
    }

    fn is_empty(&self) -> bool {
        BTreeSet::is_empty(self)
    }

    fn iter(&self) -> Self::Iter<'_> {
        BTreeSet::iter(self)
    }

    fn insert(&mut self, item: Self::Item) -> bool {
        BTreeSet::insert(self, item)
    }
}

impl<T> DistinguishedSet for BTreeSet<T>
where
    Self: Set + Eq,
{
    fn last(&self) -> Option<&T> {
        BTreeSet::last(self)
    }
}

#[cfg(feature = "std")]
impl<T> Set for HashSet<T>
where
    Self: Default,
{
    type Item = T;
    type Iter<'a> = std::collections::hash_set::Iter<'a, T>
    where
        Self::Item: 'a,
        Self: 'a;

    fn len(&self) -> usize {
        HashSet::len(self)
    }

    fn is_empty(&self) -> bool {
        HashSet::is_empty(self)
    }

    fn iter(&self) -> Self::Iter<'_> {
        HashSet::iter(self)
    }

    fn insert(&mut self, item: Self::Item) -> bool {
        HashSet::insert(self, item)
    }
}

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
    fn new_for_overwrite() -> Self {
        Self::default()
    }
}
