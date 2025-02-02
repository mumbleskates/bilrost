use alloc::borrow::Cow;
use alloc::boxed::Box;
use alloc::collections::{btree_map, btree_set, BTreeMap, BTreeSet};
use alloc::string::String;
use alloc::vec::Vec;
use core::cmp::Ordering::{Equal, Greater, Less};
#[cfg(feature = "std")]
use std::collections::{hash_map, hash_set, HashMap, HashSet};

use crate::Blob;
use crate::DecodeErrorKind::UnexpectedlyRepeated;
use crate::{Canonicity, DecodeErrorKind};

/// Trait for types that have a state that is considered "empty".
///
/// This type must be implemented for every type encodable as a directly included field in a bilrost
/// message.
pub trait EmptyState: ForOverwrite {
    #[inline(always)]
    /// Produces the empty state for this type.
    fn empty() -> Self
    where
        Self: Sized,
    {
        ForOverwrite::for_overwrite()
    }

    /// Returns true iff this instance is in the empty state.
    fn is_empty(&self) -> bool;

    fn clear(&mut self);
}

/// Trait for cheaply producing a new value that will always be overwritten or decoded into, rather
/// than a value that is definitely empty. This is implemented for types that can be present
/// optionally (in `Option` or `Vec`, for instance) but don't have an "empty" value, such as
/// enumerations without a zero value.
pub trait ForOverwrite {
    /// Produces a new `Self` value to be overwritten.
    fn for_overwrite() -> Self
    where
        Self: Sized;
}

/// Implements `ForOverwrite` in terms of `Default`.
macro_rules! for_overwrite_via_default {
    (
        $ty:ty
        $(, with generics ($($generics:tt)*))?
        $(, with where clause ($($where_clause:tt)*))?
    ) => {
        impl<$($($generics)*)?> $crate::encoding::ForOverwrite for $ty
        where
            Self: ::core::default::Default,
            $($($where_clause)*)?
        {
            #[inline]
            fn for_overwrite() -> Self {
                ::core::default::Default::default()
            }
        }
    };
}
pub(crate) use for_overwrite_via_default;

/// Implements `EmptyState` in terms of `Default`.
macro_rules! empty_state_via_default {
    (
        $ty:ty
        $(, with generics ($($generics:tt)*))?
        $(, with where clause ($($where_clause:tt)*))?
    ) => {
        for_overwrite_via_default!(
            $ty
            $(, with generics ($($generics)*))?
            $(, with where clause ($($where_clause)*))?
        );

        impl<$($($generics)*)?> $crate::encoding::EmptyState for $ty
        where
            Self: ::core::default::Default + ::core::cmp::PartialEq,
            $($($where_clause)*)?
        {
            #[inline]
            fn is_empty(&self) -> bool {
                *self == ::core::default::Default::default()
            }

            #[inline]
            fn clear(&mut self) {
                *self = $crate::encoding::EmptyState::empty();
            }
        }
    };
}
pub(crate) use empty_state_via_default;

empty_state_via_default!(bool);
empty_state_via_default!(u8);
empty_state_via_default!(u16);
empty_state_via_default!(u32);
empty_state_via_default!(u64);
empty_state_via_default!(usize);
empty_state_via_default!(i8);
empty_state_via_default!(i16);
empty_state_via_default!(i32);
empty_state_via_default!(i64);
empty_state_via_default!(isize);

macro_rules! empty_state_for_float {
    ($ty:ty) => {
        impl ForOverwrite for $ty {
            #[inline]
            fn for_overwrite() -> Self {
                0.0
            }
        }

        impl EmptyState for $ty {
            #[inline]
            fn is_empty(&self) -> bool {
                // Preserve -0.0. This is actually the original motivation for `EmptyState`.
                self.to_bits() == 0
            }

            #[inline]
            fn clear(&mut self) {
                *self = Self::empty();
            }
        }
    };
}
empty_state_for_float!(f32);
empty_state_for_float!(f64);

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

for_overwrite_via_default!(Cow<'_, str>);

impl EmptyState for Cow<'_, str> {
    #[inline]
    fn is_empty(&self) -> bool {
        str::is_empty(self)
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

for_overwrite_via_default!(bytes::Bytes);

impl EmptyState for bytes::Bytes {
    #[inline]
    fn is_empty(&self) -> bool {
        Self::is_empty(self)
    }

    #[inline]
    fn clear(&mut self) {
        *self = Self::empty();
    }
}

for_overwrite_via_default!(Blob);

impl EmptyState for Blob {
    fn is_empty(&self) -> bool {
        Vec::is_empty(self)
    }

    fn clear(&mut self) {
        Vec::clear(self)
    }
}

#[cfg(feature = "bytestring")]
for_overwrite_via_default!(bytestring::ByteString);

#[cfg(feature = "bytestring")]
impl EmptyState for bytestring::ByteString {
    #[inline]
    fn is_empty(&self) -> bool {
        str::is_empty(self)
    }

    #[inline]
    fn clear(&mut self) {
        *self = Self::empty();
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

impl<T, const N: usize> ForOverwrite for [T; N]
where
    T: ForOverwrite,
{
    #[inline]
    fn for_overwrite() -> Self {
        core::array::from_fn(|_| T::for_overwrite())
    }
}

impl<T, const N: usize> EmptyState for [T; N]
where
    T: EmptyState,
{
    #[inline]
    fn empty() -> Self
    where
        Self: Sized,
    {
        core::array::from_fn(|_| T::empty())
    }

    #[inline]
    fn is_empty(&self) -> bool {
        self.iter().all(EmptyState::is_empty)
    }

    #[inline]
    fn clear(&mut self) {
        for v in self {
            v.clear();
        }
    }
}

impl ForOverwrite for () {
    fn for_overwrite() -> Self {}
}

impl EmptyState for () {
    fn is_empty(&self) -> bool {
        true
    }

    fn clear(&mut self) {}
}

macro_rules! impls_for_tuple {
    (($($letters:ident),*), ($($numbers:tt),*),) => {
        impl<$($letters,)*> ForOverwrite for ($($letters,)*)
        where
            $($letters: ForOverwrite,)*
        {
            #[inline]
            fn for_overwrite() -> Self {
                ($($letters::for_overwrite(),)*)
            }
        }

        impl<$($letters,)*> EmptyState for ($($letters,)*)
        where
            $($letters: EmptyState,)*
        {
            #[inline]
            fn empty() -> Self {
                ($($letters::empty(),)*)
            }

            #[inline]
            fn is_empty(&self) -> bool {
                true $(&& self.$numbers.is_empty())*
            }

            #[inline]
            fn clear(&mut self) {
                $(self.$numbers.clear();)*
            }
        }
    };
}
impls_for_tuple!((A), (0),);
impls_for_tuple!((A, B), (0, 1),);
impls_for_tuple!((A, B, C), (0, 1, 2),);
impls_for_tuple!((A, B, C, D), (0, 1, 2, 3),);
impls_for_tuple!((A, B, C, D, E), (0, 1, 2, 3, 4),);
impls_for_tuple!((A, B, C, D, E, F), (0, 1, 2, 3, 4, 5),);
impls_for_tuple!((A, B, C, D, E, F, G), (0, 1, 2, 3, 4, 5, 6),);
impls_for_tuple!((A, B, C, D, E, F, G, H), (0, 1, 2, 3, 4, 5, 6, 7),);
impls_for_tuple!((A, B, C, D, E, F, G, H, I), (0, 1, 2, 3, 4, 5, 6, 7, 8),);
impls_for_tuple!(
    (A, B, C, D, E, F, G, H, I, J),
    (0, 1, 2, 3, 4, 5, 6, 7, 8, 9),
);
impls_for_tuple!(
    (A, B, C, D, E, F, G, H, I, J, K),
    (0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10),
);
impls_for_tuple!(
    (A, B, C, D, E, F, G, H, I, J, K, L),
    (0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11),
);

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
    type ReverseIter<'a>: Iterator<Item = &'a Self::Item>
    where
        Self::Item: 'a,
        Self: 'a;

    fn len(&self) -> usize;
    fn iter(&self) -> Self::RefIter<'_>;
    fn reversed(&self) -> Self::ReverseIter<'_>;
    fn insert(&mut self, item: Self::Item) -> Result<(), DecodeErrorKind>;
}

/// Trait for collections that store multiple items and have a distinguished representation, such as
/// `Vec` and `BTreeSet`. Returns an error if the items are inserted in the wrong order.
pub trait DistinguishedCollection: Collection + Eq {
    fn insert_distinguished(&mut self, item: Self::Item) -> Result<Canonicity, DecodeErrorKind>;
}

trait TriviallyDistinguishedCollection {}

impl<T> DistinguishedCollection for T
where
    T: Eq + Collection + TriviallyDistinguishedCollection,
{
    #[inline]
    fn insert_distinguished(&mut self, item: Self::Item) -> Result<Canonicity, DecodeErrorKind> {
        self.insert(item).map(|()| Canonicity::Canonical)
    }
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
    type ReverseIter<'a>: Iterator<Item = (&'a Self::Key, &'a Self::Value)>
    where
        Self::Key: 'a,
        Self::Value: 'a,
        Self: 'a;

    fn len(&self) -> usize;
    fn iter(&self) -> Self::RefIter<'_>;
    fn reversed(&self) -> Self::ReverseIter<'_>;
    fn insert(&mut self, key: Self::Key, value: Self::Value) -> Result<(), DecodeErrorKind>;
}

/// Trait for associative containers with a distinguished representation. Returns an error if the
/// items are inserted in the wrong order.
pub trait DistinguishedMapping: Mapping {
    fn insert_distinguished(
        &mut self,
        key: Self::Key,
        value: Self::Value,
    ) -> Result<Canonicity, DecodeErrorKind>;
}

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

for_overwrite_via_default!(Cow<'_, [T]>, with generics (T), with where clause (T: Clone));

impl<T> EmptyState for Cow<'_, [T]>
where
    T: Clone,
{
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

#[cfg(feature = "arrayvec")]
for_overwrite_via_default!(arrayvec::ArrayVec<T, N>, with generics (T, const N: usize));

#[cfg(feature = "arrayvec")]
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

#[cfg(feature = "arrayvec")]
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
        self.try_push(item)
            .map_err(|_| DecodeErrorKind::InvalidValue)
    }
}

#[cfg(feature = "arrayvec")]
impl<T, const N: usize> TriviallyDistinguishedCollection for arrayvec::ArrayVec<T, N> {}

#[cfg(feature = "smallvec")]
for_overwrite_via_default!(smallvec::SmallVec<A>,
    with generics(A),
    with where clause (A: smallvec::Array));

#[cfg(feature = "smallvec")]
impl<A: smallvec::Array> EmptyState for smallvec::SmallVec<A> {
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
        smallvec::SmallVec::len(self)
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
        smallvec::SmallVec::push(self, item);
        Ok(())
    }
}

#[cfg(feature = "smallvec")]
impl<A: smallvec::Array> TriviallyDistinguishedCollection for smallvec::SmallVec<A> {}

#[cfg(feature = "thin-vec")]
for_overwrite_via_default!(thin_vec::ThinVec<T>, with generics (T));

#[cfg(feature = "thin-vec")]
impl<T> EmptyState for thin_vec::ThinVec<T> {
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

#[cfg(feature = "thin-vec")]
impl<T> TriviallyDistinguishedCollection for thin_vec::ThinVec<T> {}

#[cfg(feature = "tinyvec")]
for_overwrite_via_default!(tinyvec::ArrayVec<A>,
    with generics (A),
    with where clause (A: tinyvec::Array));

#[cfg(feature = "tinyvec")]
impl<A: tinyvec::Array> EmptyState for tinyvec::ArrayVec<A> {
    #[inline]
    fn is_empty(&self) -> bool {
        tinyvec::ArrayVec::is_empty(self)
    }

    #[inline]
    fn clear(&mut self) {
        tinyvec::ArrayVec::clear(self)
    }
}

#[cfg(feature = "tinyvec")]
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
            Some(_) => Err(DecodeErrorKind::InvalidValue),
        }
    }
}

#[cfg(feature = "tinyvec")]
impl<A: tinyvec::Array> TriviallyDistinguishedCollection for tinyvec::ArrayVec<A> {}

#[cfg(feature = "tinyvec")]
for_overwrite_via_default!(tinyvec::TinyVec<A>,
    with generics (A),
    with where clause (A: tinyvec::Array));

#[cfg(feature = "tinyvec")]
impl<A: tinyvec::Array> EmptyState for tinyvec::TinyVec<A> {
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

#[cfg(feature = "tinyvec")]
impl<A: tinyvec::Array> TriviallyDistinguishedCollection for tinyvec::TinyVec<A> {}

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

#[cfg(feature = "std")]
for_overwrite_via_default!(HashSet<T, S>,
    with generics (T, S),
    with where clause (S: Default + core::hash::BuildHasher));

#[cfg(feature = "std")]
impl<T, S> EmptyState for HashSet<T, S>
where
    S: Default + core::hash::BuildHasher,
{
    #[inline]
    fn is_empty(&self) -> bool {
        HashSet::is_empty(self)
    }

    #[inline]
    fn clear(&mut self) {
        HashSet::clear(self)
    }
}

#[cfg(feature = "std")]
impl<T, S> Collection for HashSet<T, S>
where
    T: Eq + core::hash::Hash,
    S: Default + core::hash::BuildHasher,
{
    type Item = T;
    type RefIter<'a>
        = hash_set::Iter<'a, T>
    where
        Self::Item: 'a,
        Self: 'a;
    type ReverseIter<'a>
        = Self::RefIter<'a>
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
    fn reversed(&self) -> Self::ReverseIter<'_> {
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
for_overwrite_via_default!(hashbrown::HashSet<T, S>,
    with generics (T, S),
    with where clause (S: Default + core::hash::BuildHasher));

#[cfg(feature = "hashbrown")]
impl<T, S> EmptyState for hashbrown::HashSet<T, S>
where
    S: Default + core::hash::BuildHasher,
{
    #[inline]
    fn is_empty(&self) -> bool {
        hashbrown::HashSet::is_empty(self)
    }

    #[inline]
    fn clear(&mut self) {
        hashbrown::HashSet::clear(self)
    }
}

#[cfg(feature = "hashbrown")]
impl<T, S> Collection for hashbrown::HashSet<T, S>
where
    T: Eq + core::hash::Hash,
    S: Default + core::hash::BuildHasher,
{
    type Item = T;
    type RefIter<'a>
        = hashbrown::hash_set::Iter<'a, T>
    where
        Self::Item: 'a,
        Self: 'a;
    type ReverseIter<'a>
        = Self::RefIter<'a>
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
    fn reversed(&self) -> Self::ReverseIter<'_> {
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

#[cfg(feature = "std")]
for_overwrite_via_default!(HashMap<K, V, S>,
    with generics (K, V, S),
    with where clause (S: Default + core::hash::BuildHasher));

#[cfg(feature = "std")]
impl<K, V, S> EmptyState for HashMap<K, V, S>
where
    S: Default + core::hash::BuildHasher,
{
    #[inline]
    fn is_empty(&self) -> bool {
        HashMap::is_empty(self)
    }

    #[inline]
    fn clear(&mut self) {
        HashMap::clear(self)
    }
}

#[cfg(feature = "std")]
impl<K, V, S> Mapping for HashMap<K, V, S>
where
    K: Eq + core::hash::Hash,
    S: Default + core::hash::BuildHasher,
{
    type Key = K;
    type Value = V;
    type RefIter<'a>
        = hash_map::Iter<'a, K, V>
    where
        K: 'a,
        V: 'a,
        Self: 'a;
    type ReverseIter<'a>
        = Self::RefIter<'a>
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
    fn reversed(&self) -> Self::ReverseIter<'_> {
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
for_overwrite_via_default!(hashbrown::HashMap<K, V, S>,
    with generics (K, V, S),
    with where clause (S: Default + core::hash::BuildHasher));

#[cfg(feature = "hashbrown")]
impl<K, V, S> EmptyState for hashbrown::HashMap<K, V, S>
where
    S: Default + core::hash::BuildHasher,
{
    #[inline]
    fn is_empty(&self) -> bool {
        hashbrown::HashMap::is_empty(self)
    }

    #[inline]
    fn clear(&mut self) {
        hashbrown::HashMap::clear(self)
    }
}

#[cfg(feature = "hashbrown")]
impl<K, V, S> Mapping for hashbrown::HashMap<K, V, S>
where
    K: Eq + core::hash::Hash,
    S: Default + core::hash::BuildHasher,
{
    type Key = K;
    type Value = V;
    type RefIter<'a>
        = hashbrown::hash_map::Iter<'a, K, V>
    where
        K: 'a,
        V: 'a,
        Self: 'a;
    type ReverseIter<'a>
        = Self::RefIter<'a>
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
    fn reversed(&self) -> Self::ReverseIter<'_> {
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
