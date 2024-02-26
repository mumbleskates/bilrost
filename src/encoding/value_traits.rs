use alloc::borrow::Cow;
use alloc::collections::{btree_map, btree_set, BTreeMap, BTreeSet};
use alloc::vec::Vec;
use core::cmp::Ordering::{Equal, Greater, Less};
#[cfg(feature = "std")]
use core::hash::Hash;
#[cfg(feature = "std")]
use std::collections::{hash_map, hash_set, HashMap, HashSet};

use crate::DecodeErrorKind::UnexpectedlyRepeated;
use crate::{Canonicity, DecodeErrorKind};

/// Trait for cheaply producing a new value that will always be overwritten or decoded into, rather
/// than a value that is definitely empty. This is implemented for types that can be present
/// optionally (in `Option` or `Vec`, for instance) but don't have an "empty" value, such as
/// enumerations without a zero value.
pub trait NewForOverwrite {
    /// Produces a new `Self` value to be overwritten.
    fn new_for_overwrite() -> Self;
}

impl<T> NewForOverwrite for T
where
    T: EmptyState,
{
    #[inline]
    fn new_for_overwrite() -> Self {
        Self::empty()
    }
}

/// Trait for types that have a state that is considered "empty".
///
/// This type must be implemented for every type encodable as a directly included field in a bilrost
/// message.
pub trait EmptyState {
    /// Produces the empty state for this type.
    fn empty() -> Self
    where
        Self: Sized;

    /// Returns true iff this instance is in the empty state.
    fn is_empty(&self) -> bool;

    fn clear(&mut self);
}

impl<T> EmptyState for Option<T> {
    #[inline]
    fn empty() -> Self {
        None
    }

    #[inline]
    fn is_empty(&self) -> bool {
        Self::is_none(self)
    }

    #[inline]
    fn clear(&mut self) {
        *self = Self::empty();
    }
}

/// Proxy trait for enumeration types conversions to and from `u32`
pub trait Enumeration: Eq + Sized {
    /// Gets the numeric value of the enumeration.
    fn to_number(&self) -> u32;

    /// Tries to convert from the given number to the enumeration type.
    fn try_from_number(n: u32) -> Result<Self, u32>;

    /// Returns `true` if the given number represents a variant of the enumeration.
    fn is_valid(n: u32) -> bool;
}

/// Trait for containers that store multiple items such as `Vec`, `BTreeSet`, and `HashSet`
pub trait Collection: EmptyState {
    type Item;
    type RefIter<'a>: ExactSizeIterator<Item = &'a Self::Item>
    where
        Self::Item: 'a,
        Self: 'a;

    fn len(&self) -> usize;
    fn iter(&self) -> Self::RefIter<'_>;
    fn insert(&mut self, item: Self::Item) -> Result<(), DecodeErrorKind>;
}

/// Trait for collections that store multiple items and have a distinguished representation, such as
/// `Vec` and `BTreeSet`. Returns an error if the items are inserted in the wrong order.
pub trait DistinguishedCollection: Collection + Eq {
    type ReverseIter<'a>: Iterator<Item = &'a Self::Item>
    where
        Self::Item: 'a,
        Self: 'a;

    fn reversed(&self) -> Self::ReverseIter<'_>;
    fn insert_distinguished(&mut self, item: Self::Item) -> Result<Canonicity, DecodeErrorKind>;
}

/// Trait for associative containers, such as `BTreeMap` and `HashMap`.
pub trait Mapping: EmptyState {
    type Key;
    type Value;
    type RefIter<'a>: ExactSizeIterator<Item = (&'a Self::Key, &'a Self::Value)>
    where
        Self::Key: 'a,
        Self::Value: 'a,
        Self: 'a;

    fn len(&self) -> usize;
    #[inline]
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    fn iter(&self) -> Self::RefIter<'_>;
    fn insert(&mut self, key: Self::Key, value: Self::Value) -> Result<(), DecodeErrorKind>;
}

/// Trait for associative containers with a distinguished representation. Returns an error if the
/// items are inserted in the wrong order.
pub trait DistinguishedMapping: Mapping {
    type ReverseIter<'a>: Iterator<Item = (&'a Self::Key, &'a Self::Value)>
    where
        Self::Key: 'a,
        Self::Value: 'a,
        Self: 'a;

    fn reversed(&self) -> Self::ReverseIter<'_>;
    fn insert_distinguished(
        &mut self,
        key: Self::Key,
        value: Self::Value,
    ) -> Result<Canonicity, DecodeErrorKind>;
}

impl<T> EmptyState for Vec<T> {
    #[inline]
    fn empty() -> Self {
        Self::new()
    }

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
    fn insert(&mut self, item: T) -> Result<(), DecodeErrorKind> {
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
    fn insert_distinguished(&mut self, item: Self::Item) -> Result<Canonicity, DecodeErrorKind> {
        Vec::push(self, item);
        Ok(Canonicity::Canonical)
    }
}

impl<T> EmptyState for Cow<'_, [T]>
where
    T: Clone,
{
    #[inline]
    fn empty() -> Self {
        Self::default()
    }

    #[inline]
    fn is_empty(&self) -> bool {
        <[T]>::is_empty(self)
    }

    #[inline]
    fn clear(&mut self) {
        match self {
            Cow::Borrowed(_) => {
                *self = Cow::default();
            }
            Cow::Owned(owned) => {
                owned.clear();
            }
        }
    }
}

impl<T> Collection for Cow<'_, [T]>
where
    T: Clone,
{
    type Item = T;
    type RefIter<'a> = core::slice::Iter<'a, T>
        where
            T: 'a,
            Self: 'a;
    fn len(&self) -> usize {
        <[T]>::len(self)
    }

    fn iter(&self) -> Self::RefIter<'_> {
        <[T]>::iter(self)
    }

    fn insert(&mut self, item: Self::Item) -> Result<(), DecodeErrorKind> {
        self.to_mut().push(item);
        Ok(())
    }
}

impl<T> DistinguishedCollection for Cow<'_, [T]>
where
    T: Clone + Eq,
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
    fn insert_distinguished(&mut self, item: Self::Item) -> Result<Canonicity, DecodeErrorKind> {
        self.to_mut().push(item);
        Ok(Canonicity::Canonical)
    }
}

#[cfg(feature = "smallvec")]
impl<T, A: smallvec::Array<Item = T>> EmptyState for smallvec::SmallVec<A> {
    #[inline]
    fn empty() -> Self {
        Self::new()
    }

    #[inline]
    fn is_empty(&self) -> bool {
        Self::is_empty(self)
    }

    #[inline]
    fn clear(&mut self) {
        Self::clear(self)
    }
}

#[cfg(feature = "smallvec")]
impl<T, A: smallvec::Array<Item = T>> Collection for smallvec::SmallVec<A> {
    type Item = T;
    type RefIter<'a> = core::slice::Iter<'a, T>
        where
            T: 'a,
            Self: 'a;

    #[inline]
    fn len(&self) -> usize {
        smallvec::SmallVec::len(self)
    }

    #[inline]
    fn iter(&self) -> Self::RefIter<'_> {
        <[T]>::iter(self)
    }

    #[inline]
    fn insert(&mut self, item: T) -> Result<(), DecodeErrorKind> {
        smallvec::SmallVec::push(self, item);
        Ok(())
    }
}

#[cfg(feature = "smallvec")]
impl<T, A: smallvec::Array<Item = T>> DistinguishedCollection for smallvec::SmallVec<A>
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
    fn insert_distinguished(&mut self, item: Self::Item) -> Result<Canonicity, DecodeErrorKind> {
        smallvec::SmallVec::push(self, item);
        Ok(Canonicity::Canonical)
    }
}

#[cfg(feature = "thin-vec")]
impl<T> EmptyState for thin_vec::ThinVec<T> {
    #[inline]
    fn empty() -> Self {
        Self::new()
    }

    #[inline]
    fn is_empty(&self) -> bool {
        Self::is_empty(self)
    }

    #[inline]
    fn clear(&mut self) {
        Self::clear(self)
    }
}

#[cfg(feature = "thin-vec")]
impl<T> Collection for thin_vec::ThinVec<T> {
    type Item = T;
    type RefIter<'a> = core::slice::Iter<'a, T>
        where
            T: 'a,
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
    fn insert(&mut self, item: T) -> Result<(), DecodeErrorKind> {
        thin_vec::ThinVec::push(self, item);
        Ok(())
    }
}

#[cfg(feature = "thin-vec")]
impl<T> DistinguishedCollection for thin_vec::ThinVec<T>
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
    fn insert_distinguished(&mut self, item: Self::Item) -> Result<Canonicity, DecodeErrorKind> {
        thin_vec::ThinVec::push(self, item);
        Ok(Canonicity::Canonical)
    }
}

#[cfg(feature = "tinyvec")]
impl<T, A: tinyvec::Array<Item = T>> EmptyState for tinyvec::TinyVec<A> {
    #[inline]
    fn empty() -> Self {
        Self::new()
    }

    #[inline]
    fn is_empty(&self) -> bool {
        Self::is_empty(self)
    }

    #[inline]
    fn clear(&mut self) {
        Self::clear(self)
    }
}

#[cfg(feature = "tinyvec")]
impl<T, A: tinyvec::Array<Item = T>> Collection for tinyvec::TinyVec<A> {
    type Item = T;
    type RefIter<'a> = core::slice::Iter<'a, T>
        where
            T: 'a,
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
    fn insert(&mut self, item: T) -> Result<(), DecodeErrorKind> {
        tinyvec::TinyVec::push(self, item);
        Ok(())
    }
}

#[cfg(feature = "tinyvec")]
impl<T, A: tinyvec::Array<Item = T>> DistinguishedCollection for tinyvec::TinyVec<A>
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
    fn insert_distinguished(&mut self, item: Self::Item) -> Result<Canonicity, DecodeErrorKind> {
        tinyvec::TinyVec::push(self, item);
        Ok(Canonicity::Canonical)
    }
}

impl<T> EmptyState for BTreeSet<T> {
    #[inline]
    fn empty() -> Self {
        Self::new()
    }

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
    type RefIter<'a> = btree_set::Iter<'a, T>
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
    type ReverseIter<'a> = core::iter::Rev<btree_set::Iter<'a, T>>
        where
            Self::Item: 'a,
            Self: 'a;

    #[inline]
    fn reversed(&self) -> Self::ReverseIter<'_> {
        BTreeSet::iter(self).rev()
    }

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

#[cfg(feature = "std")]
impl<T> EmptyState for HashSet<T> {
    #[inline]
    fn empty() -> Self {
        Self::new()
    }

    #[inline]
    fn is_empty(&self) -> bool {
        Self::is_empty(self)
    }

    #[inline]
    fn clear(&mut self) {
        Self::clear(self)
    }
}

#[cfg(feature = "std")]
impl<T> Collection for HashSet<T>
where
    T: Eq + Hash,
{
    type Item = T;
    type RefIter<'a> = hash_set::Iter<'a, T>
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
    fn insert(&mut self, item: Self::Item) -> Result<(), DecodeErrorKind> {
        if !HashSet::insert(self, item) {
            return Err(UnexpectedlyRepeated);
        }
        Ok(())
    }
}

#[cfg(feature = "hashbrown")]
impl<T> EmptyState for hashbrown::HashSet<T> {
    #[inline]
    fn empty() -> Self {
        Self::new()
    }

    #[inline]
    fn is_empty(&self) -> bool {
        Self::is_empty(self)
    }

    #[inline]
    fn clear(&mut self) {
        Self::clear(self)
    }
}

#[cfg(feature = "hashbrown")]
impl<T> Collection for hashbrown::HashSet<T>
where
    T: Eq + Hash,
{
    type Item = T;
    type RefIter<'a> = hashbrown::hash_set::Iter<'a, T>
        where
            Self::Item: 'a,
            Self: 'a;

    #[inline]
    fn len(&self) -> usize {
        hashbrown::HashSet::len(self)
    }

    #[inline]
    fn iter(&self) -> Self::RefIter<'_> {
        hashbrown::HashSet::iter(self)
    }

    #[inline]
    fn insert(&mut self, item: Self::Item) -> Result<(), DecodeErrorKind> {
        if !hashbrown::HashSet::insert(self, item) {
            return Err(UnexpectedlyRepeated);
        }
        Ok(())
    }
}

impl<K, V> EmptyState for BTreeMap<K, V> {
    #[inline]
    fn empty() -> Self {
        Self::new()
    }

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
    type RefIter<'a> = btree_map::Iter<'a, K, V>
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
    type ReverseIter<'a> = core::iter::Rev<btree_map::Iter<'a, K, V>>
        where
            K: 'a,
            V: 'a,
            Self: 'a;

    #[inline]
    fn reversed(&self) -> Self::ReverseIter<'_> {
        BTreeMap::iter(self).rev()
    }

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

#[cfg(feature = "std")]
impl<K, V> EmptyState for HashMap<K, V> {
    #[inline]
    fn empty() -> Self {
        Self::new()
    }

    #[inline]
    fn is_empty(&self) -> bool {
        Self::is_empty(self)
    }

    #[inline]
    fn clear(&mut self) {
        Self::clear(self)
    }
}

#[cfg(feature = "std")]
impl<K, V> Mapping for HashMap<K, V>
where
    K: Eq + Hash,
{
    type Key = K;
    type Value = V;
    type RefIter<'a> = hash_map::Iter<'a, K, V>
        where
            K: 'a,
            V: 'a,
            Self: 'a;

    #[inline]
    fn len(&self) -> usize {
        HashMap::len(self)
    }

    #[inline]
    fn iter(&self) -> Self::RefIter<'_> {
        HashMap::iter(self)
    }

    #[inline]
    fn insert(&mut self, key: K, value: V) -> Result<(), DecodeErrorKind> {
        if let hash_map::Entry::Vacant(entry) = self.entry(key) {
            entry.insert(value);
            Ok(())
        } else {
            Err(UnexpectedlyRepeated)
        }
    }
}

#[cfg(feature = "hashbrown")]
impl<K, V> EmptyState for hashbrown::HashMap<K, V> {
    #[inline]
    fn empty() -> Self {
        Self::new()
    }

    #[inline]
    fn is_empty(&self) -> bool {
        Self::is_empty(self)
    }

    #[inline]
    fn clear(&mut self) {
        Self::clear(self)
    }
}

#[cfg(feature = "hashbrown")]
impl<K, V> Mapping for hashbrown::HashMap<K, V>
where
    K: Eq + Hash,
{
    type Key = K;
    type Value = V;
    type RefIter<'a> = hashbrown::hash_map::Iter<'a, K, V>
        where
            K: 'a,
            V: 'a,
            Self: 'a;

    #[inline]
    fn len(&self) -> usize {
        hashbrown::HashMap::len(self)
    }

    #[inline]
    fn iter(&self) -> Self::RefIter<'_> {
        hashbrown::HashMap::iter(self)
    }

    #[inline]
    fn insert(&mut self, key: K, value: V) -> Result<(), DecodeErrorKind> {
        if let hashbrown::hash_map::Entry::Vacant(entry) = self.entry(key) {
            entry.insert(value);
            Ok(())
        } else {
            Err(UnexpectedlyRepeated)
        }
    }
}
