use crate::encoding::value_traits::for_overwrite_via_default;
use crate::encoding::{
    delegate_encoding, delegate_value_encoding, Collection, EmptyState, General, GeneralGeneric,
    GeneralPacked, Map, Mapping, Packed, Unpacked,
};
use crate::DecodeErrorKind;
use crate::DecodeErrorKind::UnexpectedlyRepeated;
use core::hash::Hash;

for_overwrite_via_default!(hashbrown::HashSet<T, S>,
        with generics (T, S),
        with where clause (T: Eq + Hash, S: Default + core::hash::BuildHasher));

impl<T, S> EmptyState for hashbrown::HashSet<T, S>
where
    T: Eq + Hash,
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

impl<T, S> Collection for hashbrown::HashSet<T, S>
where
    T: Eq + Hash,
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

for_overwrite_via_default!(hashbrown::HashMap<K, V, S>,
        with generics (K, V, S),
        with where clause (K: Eq + Hash, S: Default + core::hash::BuildHasher));

impl<K, V, S> EmptyState for hashbrown::HashMap<K, V, S>
where
    K: Eq + Hash,
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

impl<K, V, S> Mapping for hashbrown::HashMap<K, V, S>
where
    K: Eq + Hash,
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

delegate_encoding!(
    delegate from (General) to (Unpacked)
    for type (hashbrown::HashSet<T, S>)
    with where clause (T: Eq + Hash, S: Default + core::hash::BuildHasher)
    with generics (T, S)
);
delegate_value_encoding!(
    delegate from (GeneralPacked) to (Packed)
    for type (hashbrown::HashSet<T, S>)
    with where clause (T: Eq + Hash, S: Default + core::hash::BuildHasher)
    with generics (T, S)
);
delegate_value_encoding!(
    delegate from (GeneralGeneric<P>) to (Map)
    for type (hashbrown::HashMap<K, V, S>)
    with where clause (K: Eq + Hash, S: Default + core::hash::BuildHasher)
    with generics (const P: u8, K, V, S)
);

#[cfg(test)]
mod test {
    mod hashbrown_hashmap {
        mod general {
            use crate::encoding::test::check_type_test;
            use crate::encoding::{General, Map};
            use alloc::collections::BTreeMap;
            use hashbrown::HashMap;
            check_type_test!(
                Map<General, General>,
                relaxed,
                from BTreeMap<u64, f32>,
                into HashMap<u64, f32>,
                converter(value) {
                    <HashMap<u64, f32> as FromIterator<_>>::from_iter(value.into_iter())
                },
                WireType::LengthDelimited
            );
        }

        mod fixed {
            use crate::encoding::test::check_type_test;
            use crate::encoding::{Fixed, Map};
            use alloc::collections::BTreeMap;
            use hashbrown::HashMap;
            check_type_test!(
                Map<Fixed, Fixed>,
                relaxed,
                from BTreeMap<u64, f32>,
                into HashMap<u64, f32>,
                converter(value) {
                    <HashMap<u64, f32> as FromIterator<_>>::from_iter(value.into_iter())
                },
                WireType::LengthDelimited
            );
        }

        mod delegated_from_general {
            use crate::encoding::test::check_type_test;
            use crate::encoding::General;
            use alloc::collections::BTreeMap;
            use hashbrown::HashMap;
            check_type_test!(
                General,
                relaxed,
                from BTreeMap<bool, u32>,
                into HashMap<bool, u32>,
                converter(value) {
                    <HashMap<bool, u32> as FromIterator<_>>::from_iter(value.into_iter())
                },
                WireType::LengthDelimited
            );
        }
    }
}
