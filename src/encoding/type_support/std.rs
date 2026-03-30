use crate::encoding::proxy::SealedBilrostTag;
use crate::encoding::value_traits::for_overwrite_via_default;
use crate::encoding::{
    delegate_encoding, delegate_proxied_encoding, delegate_value_encoding, Collection, EmptyState,
    ForOverwrite, General, GeneralGeneric, GeneralPacked, Map, Mapping, Packed, Proxiable,
    Unpacked, Varint,
};
use crate::DecodeErrorKind::{self, InvalidValue, OutOfDomainValue, UnexpectedlyRepeated};
use core::cmp::Ordering;
use std::collections::{hash_map, hash_set, HashMap, HashSet};
use std::time::{SystemTime, UNIX_EPOCH};

for_overwrite_via_default!(HashSet<T, S>,
    with generics (T, S),
    with where clause (S: Default + core::hash::BuildHasher));

impl<T, S> EmptyState<(), HashSet<T, S>> for ()
where
    S: Default + core::hash::BuildHasher,
{
    #[inline]
    fn is_empty(val: &HashSet<T, S>) -> bool {
        val.is_empty()
    }

    #[inline]
    fn clear(val: &mut HashSet<T, S>) {
        val.clear();
    }
}

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

    const RESTRICTIONS: Option<&'static str> = Some("unique");

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

for_overwrite_via_default!(HashMap<K, V, S>,
    with generics (K, V, S),
    with where clause (S: Default + core::hash::BuildHasher));

impl<K, V, S> EmptyState<(), HashMap<K, V, S>> for ()
where
    S: Default + core::hash::BuildHasher,
{
    #[inline]
    fn is_empty(val: &HashMap<K, V, S>) -> bool {
        val.is_empty()
    }

    #[inline]
    fn clear(val: &mut HashMap<K, V, S>) {
        val.clear();
    }
}

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

impl ForOverwrite<(), SystemTime> for () {
    fn for_overwrite() -> SystemTime {
        UNIX_EPOCH
    }
}

impl EmptyState<(), SystemTime> for () {
    fn is_empty(val: &SystemTime) -> bool {
        *val == UNIX_EPOCH
    }

    fn clear(val: &mut SystemTime) {
        *val = UNIX_EPOCH;
    }
}

impl Proxiable<SealedBilrostTag> for SystemTime {
    type Proxy = crate::encoding::local_proxy::LocalProxy<u64, 3>;

    fn encode_proxy(&self) -> Self::Proxy {
        let (symbol, small, big) = match self.cmp(&UNIX_EPOCH) {
            Ordering::Equal => return <() as EmptyState<(), Self::Proxy>>::empty(),
            // lacking a simpler way, we put a literal ascii + or - to indicate the sign of the
            // timestamp.
            Ordering::Greater => ('+', &UNIX_EPOCH, self),
            Ordering::Less => ('-', self, &UNIX_EPOCH),
        };
        let magnitude = big
            .duration_since(*small)
            .expect("SystemTime dates ordered wrong");
        Self::Proxy::new_without_empty_suffix([
            symbol as u64,
            magnitude.as_secs(),
            magnitude.subsec_nanos() as u64,
        ])
    }

    fn decode_proxy(&mut self, proxy: Self::Proxy) -> Result<(), DecodeErrorKind> {
        const PLUS: u64 = '+' as u64;
        const MINUS: u64 = '-' as u64;
        let (operation, secs, nanos): (fn(_, _) -> _, u64, u64) = match proxy.into_inner() {
            [0, 0, 0] => {
                *self = UNIX_EPOCH;
                return Ok(());
            }
            [symbol @ (PLUS | MINUS), secs, nanos @ 0..=999_999_999] => (
                if symbol == PLUS {
                    SystemTime::checked_add
                } else {
                    SystemTime::checked_sub
                },
                secs,
                nanos,
            ),
            _ => return Err(InvalidValue),
        };
        *self = operation(&UNIX_EPOCH, core::time::Duration::new(secs, nanos as u32))
            .ok_or(OutOfDomainValue)?;
        Ok(())
    }
}

delegate_proxied_encoding!(
    use encoding (Packed<Varint>) to encode proxied type (SystemTime)
    using proxy tag (SealedBilrostTag)
    with general encodings
);

#[cfg(test)]
mod systemtime {
    use super::*;
    use crate::encoding::test::{check_type_empty, check_type_test};

    check_type_empty!(SystemTime, via proxy with tag SealedBilrostTag);
    check_type_test!(General, relaxed, SystemTime, WireType::LengthDelimited);
}

delegate_encoding!(
    delegate from (General) to (Unpacked)
    for type (HashSet<T, S>)
    including schema
    with where clause (S: Default + core::hash::BuildHasher)
    with generics (T, S)
);
delegate_value_encoding!(
    delegate from (GeneralPacked) to (Packed)
    for type (HashSet<T, S>)
    including schema
    with where clause (S: Default + core::hash::BuildHasher)
    with generics (T, S)
);

delegate_value_encoding!(
    delegate from (GeneralGeneric<P>) to (Map)
    for type (HashMap<K, V, S>)
    including schema
    with where clause (K: Eq + core::hash::Hash, S: Default + core::hash::BuildHasher)
    with generics (const P: u8, K, V, S)
);

#[cfg(test)]
mod test {
    mod hash_map {
        mod general {
            use crate::encoding::test::check_type_test;
            use crate::encoding::{General, Map};
            use std::collections::HashMap;
            check_type_test!(
                Map<General, General>,
                relaxed,
                HashMap<u64, f32>,
                WireType::LengthDelimited
            );
        }

        mod fixed {
            use crate::encoding::test::check_type_test;
            use crate::encoding::{Fixed, Map};
            use std::collections::HashMap;
            check_type_test!(
                Map<Fixed, Fixed>,
                relaxed,
                HashMap<u64, f32>,
                WireType::LengthDelimited
            );
        }

        mod delegated_from_general {
            use crate::encoding::test::check_type_test;
            use crate::encoding::General;
            use std::collections::HashMap;
            check_type_test!(
                General,
                relaxed,
                HashMap<bool, u32>,
                WireType::LengthDelimited
            );
        }

        mod delegated_from_general_in_oneof {
            use crate::encoding::test::check_type_test;
            use crate::encoding::GeneralPacked;
            use std::collections::HashMap;
            check_type_test!(
                GeneralPacked,
                relaxed,
                HashMap<bool, u32>,
                WireType::LengthDelimited
            );
        }
    }
}
