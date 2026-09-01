use crate::encoding::value_traits::{empty_state_via_default, for_overwrite_via_default};
use crate::encoding::{EmptyState, ForOverwrite};

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
        impl ForOverwrite<(), $ty> for () {
            #[inline]
            fn for_overwrite() -> $ty {
                0.0
            }
        }

        impl EmptyState<(), $ty> for () {
            #[inline]
            fn is_empty(val: &$ty) -> bool {
                // Preserve -0.0. This is actually the original motivation for `EmptyState`.
                val.to_bits() == 0
            }

            #[inline]
            fn clear(val: &mut $ty) {
                *val = 0.0;
            }
        }
    };
}
empty_state_for_float!(f32);
empty_state_for_float!(f64);

empty_state_via_default!(&'a str, with generics ('a));

for_overwrite_via_default!(&'a [T], with generics ('a, T));

impl<T> EmptyState<(), &[T]> for () {
    fn is_empty(val: &&[T]) -> bool {
        <[T]>::is_empty(val)
    }

    fn clear(val: &mut &[T]) {
        *val = &[];
    }
}

impl<'a, const N: usize> ForOverwrite<(), &'a [u8; N]> for () {
    fn for_overwrite() -> &'a [u8; N] {
        &[0; N]
    }
}

impl<const N: usize> EmptyState<(), &[u8; N]> for () {
    fn is_empty(val: &&[u8; N]) -> bool {
        *val == &[0; N]
    }

    fn clear(val: &mut &[u8; N]) {
        *val = &[0; N];
    }
}

macro_rules! impls_for_tuple {
    (($($letters:ident),*), ($($numbers:tt),*)$(,)?) => {
        impl<$($letters,)*> ForOverwrite<(), ($($letters,)*)> for ()
        where
            $((): ForOverwrite<(), $letters>,)*
        {
            const INIT_HEAP: usize = {
                0
                $(+ <() as ForOverwrite<(), $letters>>::INIT_HEAP)*
            };

            #[inline]
            fn for_overwrite() -> ($($letters,)*) {
                ($(<() as ForOverwrite<(), $letters>>::for_overwrite(),)*)
            }
        }

        impl<$($letters,)*> EmptyState<(), ($($letters,)*)> for ()
        where
            $((): EmptyState<(), $letters>,)*
        {
            #[inline]
            fn empty() -> ($($letters,)*) {
                ($(<() as EmptyState<(), $letters>>::empty(),)*)
            }

            #[inline]
            fn is_empty(val: &($($letters,)*)) -> bool {
                true $(&& <() as EmptyState<(), $letters>>::is_empty(&val.$numbers))*
            }

            #[inline]
            fn clear(val: &mut ($($letters,)*)) {
                $(<() as EmptyState<(), $letters>>::clear(&mut val.$numbers);)*
            }
        }
    };
}
impls_for_tuple!((A), (0));
impls_for_tuple!((A, B), (0, 1));
impls_for_tuple!((A, B, C), (0, 1, 2));
impls_for_tuple!((A, B, C, D), (0, 1, 2, 3));
impls_for_tuple!((A, B, C, D, E), (0, 1, 2, 3, 4));
impls_for_tuple!((A, B, C, D, E, F), (0, 1, 2, 3, 4, 5));
impls_for_tuple!((A, B, C, D, E, F, G), (0, 1, 2, 3, 4, 5, 6));
impls_for_tuple!((A, B, C, D, E, F, G, H), (0, 1, 2, 3, 4, 5, 6, 7));
impls_for_tuple!((A, B, C, D, E, F, G, H, I), (0, 1, 2, 3, 4, 5, 6, 7, 8));
impls_for_tuple!(
    (A, B, C, D, E, F, G, H, I, J),
    (0, 1, 2, 3, 4, 5, 6, 7, 8, 9)
);
impls_for_tuple!(
    (A, B, C, D, E, F, G, H, I, J, K),
    (0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10)
);
impls_for_tuple!(
    (A, B, C, D, E, F, G, H, I, J, K, L),
    (0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11)
);

impl ForOverwrite<(), ()> for () {
    fn for_overwrite() -> Self {}
}

impl EmptyState<(), ()> for () {
    fn is_empty(_: &()) -> bool {
        true
    }

    fn clear(_: &mut ()) {}
}
