use crate::encoding::value_traits::for_overwrite_via_default;
use crate::encoding::EmptyState;
use crate::Blob;
use alloc::vec::Vec;

for_overwrite_via_default!(Blob);

impl EmptyState for Blob {
    fn is_empty(&self) -> bool {
        Vec::is_empty(self)
    }

    fn clear(&mut self) {
        Vec::clear(self)
    }
}

for_overwrite_via_default!(bytes::Bytes);

impl EmptyState for bytes::Bytes {
    #[inline]
    fn is_empty(&self) -> bool {
        bytes::Bytes::is_empty(self)
    }

    #[inline]
    fn clear(&mut self) {
        *self = <Self as EmptyState>::empty();
    }
}
