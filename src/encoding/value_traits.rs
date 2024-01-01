use alloc::vec::Vec;
use core::iter::Extend;

/// Trait for containers that store their values in a consistent order.
pub trait Veclike: Extend<Self::Item> {
    type Item;
    type Iter<'a>: Iterator<Item = &'a Self::Item>
    where
        Self::Item: 'a,
        Self: 'a;

    fn len(&self) -> usize;
    fn iter(&self) -> Self::Iter<'_>;
    fn push(&mut self, item: Self::Item);
    fn is_empty(&self) -> bool;
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
    fn iter(&self) -> Self::Iter<'_> {
        <[T]>::iter(self)
    }

    #[inline]
    fn push(&mut self, item: T) {
        Vec::push(self, item)
    }

    #[inline]
    fn is_empty(&self) -> bool {
        Vec::is_empty(self)
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
