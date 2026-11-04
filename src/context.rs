use crate::encoding::{Canonicity, DecodeContext, RestrictedDecodeContext};
use crate::error::{DecodeError, DecodeErrorKind, OversizeDecodingError, RecursionError};
use core::cell::Cell;

#[cfg(feature = "no-recursion-limit")]
type DefaultCtx = PlainDecodeCtx;
#[cfg(not(feature = "no-recursion-limit"))]
type DefaultCtx = RecursiveDecodeCtx;

/// Creates a basic decode context, with or without recursion limits depending on the library
/// configuration.
pub(crate) fn default_ctx() -> impl DecodeContext {
    DefaultCtx::default()
}

// TODO: friendlier user-facing api for creating contexts

/// This is the basic decode context type with no restriction on recursion.
#[derive(Clone, Debug, Default)]
pub struct PlainDecodeCtx {}

impl DecodeContext for PlainDecodeCtx {
    type Unrestricted = Self;

    #[inline]
    fn enter_recursion(&self) -> Result<Self, RecursionError> {
        Ok(self.clone())
    }

    #[inline]
    fn heap_used(&self, _bytes_used: usize) -> Result<(), OversizeDecodingError> {
        Ok(())
    }

    #[inline]
    fn unrestricted(&self) -> Self {
        self.clone()
    }
}

/// This is a basic decode context that limits recursion while decoding.
#[derive(Clone, Debug)]
pub struct RecursiveDecodeCtx {
    /// How many times we can recurse in the current decode stack before we hit
    /// the recursion limit.
    recurse_count: u32,
}

impl RecursiveDecodeCtx {
    // TODO: doc
    pub fn new(limit: u32) -> Self {
        RecursiveDecodeCtx {
            recurse_count: limit,
        }
    }
}

impl Default for RecursiveDecodeCtx {
    #[inline]
    fn default() -> RecursiveDecodeCtx {
        RecursiveDecodeCtx {
            recurse_count: crate::DEFAULT_RECURSION_LIMIT,
        }
    }
}

impl DecodeContext for RecursiveDecodeCtx {
    type Unrestricted = Self;

    #[inline]
    fn enter_recursion(&self) -> Result<RecursiveDecodeCtx, RecursionError> {
        if self.recurse_count == 0 {
            return Err(RecursionError);
        }
        Ok(RecursiveDecodeCtx {
            recurse_count: self.recurse_count - 1,
        })
    }

    #[inline]
    fn heap_used(&self, _bytes_used: usize) -> Result<(), OversizeDecodingError> {
        Ok(())
    }

    #[inline]
    fn unrestricted(&self) -> Self {
        self.clone()
    }
}

/// Basic context that records additional heap memory used.
#[derive(Clone, Debug)]
pub struct HeapTrackingCtx<'a, D: DecodeContext = DefaultCtx> {
    context: D,
    tracker: &'a Cell<usize>,
}

impl<'a> HeapTrackingCtx<'a> {
    pub fn new(tracker: &'a Cell<usize>) -> Self {
        Self::new_with(Default::default(), tracker)
    }
}

impl<'a, D: DecodeContext> HeapTrackingCtx<'a, D> {
    pub fn new_with(context: D, tracker: &'a Cell<usize>) -> Self {
        Self { context, tracker }
    }
}

impl<D: DecodeContext> DecodeContext for HeapTrackingCtx<'_, D> {
    type Unrestricted = Self;

    #[inline]
    fn enter_recursion(&self) -> Result<Self, RecursionError> {
        Ok(Self {
            context: self.context.enter_recursion()?,
            tracker: self.tracker,
        })
    }

    #[inline]
    fn heap_used(&self, bytes_used: usize) -> Result<(), OversizeDecodingError> {
        self.tracker.set(self.tracker.get() + bytes_used);
        Ok(())
    }

    #[inline]
    fn unrestricted(&self) -> Self {
        self.clone()
    }
}

/// Basic context that records additional heap memory used and returns an error if a quota is
/// exceeded.
#[derive(Clone, Debug)]
pub struct HeapQuotaCtx<'a, D: DecodeContext = DefaultCtx> {
    context: D,
    tracker_max: &'a (Cell<usize>, usize),
}

impl<'a> HeapQuotaCtx<'a> {
    // TODO: turn tracker_max into a real type
    pub fn new(tracker_max: &'a (Cell<usize>, usize)) -> Self {
        Self::new_with(Default::default(), tracker_max)
    }
}

impl<'a, D: DecodeContext> HeapQuotaCtx<'a, D> {
    pub fn new_with(context: D, tracker_max: &'a (Cell<usize>, usize)) -> Self {
        Self {
            context,
            tracker_max,
        }
    }
}

impl<D: DecodeContext> DecodeContext for HeapQuotaCtx<'_, D> {
    type Unrestricted = Self;

    #[inline]
    fn enter_recursion(&self) -> Result<Self, RecursionError> {
        Ok(Self {
            context: self.context.enter_recursion()?,
            tracker_max: self.tracker_max,
        })
    }

    #[inline]
    fn heap_used(&self, bytes_used: usize) -> Result<(), OversizeDecodingError> {
        let (tracker, max) = self.tracker_max;
        let new_tracked = tracker.get() + bytes_used;
        tracker.set(new_tracked);
        if new_tracked > *max {
            return Err(OversizeDecodingError);
        }
        Ok(())
    }

    #[inline]
    fn unrestricted(&self) -> Self {
        self.clone()
    }
}

/// Additional information passed to every distinguished decode/merge function.
///
/// The context should be passed by value and can be freely cloned. When passing
/// to a function which is decoding a nested object, then use `enter_recursion`.
#[derive(Clone, Debug)]
pub struct RestrictedCtx<D: DecodeContext = DefaultCtx> {
    context: D,
    min_canonicity: Canonicity,
}

impl RestrictedCtx {
    pub fn new(min_canonicity: Canonicity) -> Self {
        Self::new_with(Default::default(), min_canonicity)
    }
}

impl<D: DecodeContext> RestrictedCtx<D> {
    /// Creates a new context with a given minimum canonicity.
    pub fn new_with(context: D, min_canonicity: Canonicity) -> Self {
        Self {
            context,
            min_canonicity,
        }
    }
}

impl<D: DecodeContext> DecodeContext for RestrictedCtx<D> {
    type Unrestricted = D::Unrestricted;

    #[inline]
    fn enter_recursion(&self) -> Result<Self, RecursionError> {
        Ok(Self {
            context: self.context.enter_recursion()?,
            min_canonicity: self.min_canonicity,
        })
    }

    #[inline]
    fn heap_used(&self, bytes_used: usize) -> Result<(), OversizeDecodingError> {
        self.context.heap_used(bytes_used)
    }

    fn unrestricted(&self) -> Self::Unrestricted {
        self.context.unrestricted()
    }
}

impl<D: DecodeContext> RestrictedDecodeContext for RestrictedCtx<D> {
    #[inline]
    fn check(&self, canon: Canonicity) -> Result<Canonicity, DecodeError> {
        match (canon < self.min_canonicity, canon) {
            (true, Canonicity::NotCanonical) => {
                Err(DecodeError::new(DecodeErrorKind::NotCanonical))
            }
            (true, Canonicity::HasExtensions) => {
                Err(DecodeError::new(DecodeErrorKind::UnknownField))
            }
            _ => Ok(canon),
        }
    }
}
