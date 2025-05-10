use crate::{Canonicity, DecodeErrorKind};

/// Trait for types that have a state that is considered "empty".
///
/// This type must be implemented for every type encodable as a directly included field in a bilrost
/// message.
///
// TODO(widders): document () delegation and tagged implementations
pub trait EmptyState<E = ()>: ForOverwrite<E> {
    #[inline(always)]
    /// Produces the empty state for this type.
    fn empty() -> Self
    where
        Self: Sized,
    {
        ForOverwrite::<E>::for_overwrite()
    }

    /// Returns true iff this instance is in the empty state.
    fn is_empty(&self) -> bool;

    fn clear(&mut self);
}

/// Trait for cheaply producing a new value that will always be overwritten or decoded into, rather
/// than a value that is definitely empty. This is implemented for types that can be present
/// optionally (in `Option` or `Vec`, for instance) but don't have an "empty" value, such as
/// enumerations without a zero value.
///
// TODO(widders): document () delegation and tagged implementations
pub trait ForOverwrite<E = ()> {
    /// Produces a new `Self` value to be overwritten.
    fn for_overwrite() -> Self
    where
        Self: Sized;
}

/// Implements `ForOverwrite` in terms of `Default`.
#[macro_export]
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
pub use for_overwrite_via_default;

/// Implements `EmptyState` in terms of `ForOverwrite`.
#[macro_export]
macro_rules! empty_state_via_for_overwrite {
    (
        $ty:ty
        $(, with generics ($($generics:tt)*))?
        $(, with where clause ($($where_clause:tt)*))?
    ) => {
        impl<$($($generics)*)?> $crate::encoding::EmptyState for $ty
        where
            Self: $crate::encoding::ForOverwrite + ::core::cmp::PartialEq,
            $($($where_clause)*)?
        {
            #[inline]
            fn is_empty(&self) -> bool {
                *self == Self::empty()
            }

            #[inline]
            fn clear(&mut self) {
                *self = Self::empty();
            }
        }
    };
}
pub use empty_state_via_for_overwrite;

/// Implements both `EmptyState` and `ForOverwrite` in terms of `Default`.
#[macro_export]
macro_rules! empty_state_via_default {
    (
        $ty:ty
        $(, with generics ($($generics:tt)*))?
        $(, with where clause ($($where_clause:tt)*))?
    ) => {
        $crate::for_overwrite_via_default!(
            $ty
            $(, with generics ($($generics*)*))?
            $(, with where clause ($($where_clause)*))?
        );
        $crate::empty_state_via_for_overwrite!(
            $ty
            $(, with generics ($($generics*)*))?
            $(, with where clause ($($where_clause)*))?
        );
    };
}
pub use empty_state_via_default;

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

pub(crate) trait TriviallyDistinguishedCollection {}

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
