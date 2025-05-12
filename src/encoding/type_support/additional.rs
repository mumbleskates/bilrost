use crate::encoding::value_traits::for_overwrite_via_default;
use crate::encoding::EmptyState;
use crate::Blob;
use alloc::vec::Vec;

for_overwrite_via_default!(Blob);

impl EmptyState<(), Blob> for () {
    fn is_empty(val: &Blob) -> bool {
        Vec::is_empty(val)
    }

    fn clear(val: &mut Blob) {
        Vec::clear(val)
    }
}

for_overwrite_via_default!(bytes::Bytes);

impl EmptyState<(), bytes::Bytes> for () {
    #[inline]
    fn is_empty(val: &bytes::Bytes) -> bool {
        bytes::Bytes::is_empty(val)
    }

    #[inline]
    fn clear(val: &mut bytes::Bytes) {
        *val = <() as EmptyState<(), _>>::empty();
    }
}
