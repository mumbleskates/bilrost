//! Bilrost encoding and decoding errors.

use core::fmt;

/// Bilrost message decoding error types.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DecodeErrorKind {
    /// Decoded data was truncated.
    Truncated,
    /// Invalid varint. (The only invalid varints are ones that would encode values > `u64::MAX`.)
    InvalidVarint,
    /// A field key encoded a tag greater than `u32::MAX`.
    TagOverflowed,
    /// A field's wire type was encountered that cannot encode a valid value.
    WrongWireType,
    /// Value was out of domain for its type.
    OutOfDomainValue,
    /// Value was invalid, such as non-UTF-8 data in a `String` field.
    InvalidValue,
    /// Conflicting mutually exclusive fields.
    ConflictingFields,
    /// A field or part of a value occurred multiple times when it should not.
    UnexpectedlyRepeated,
    /// A value was not encoded canonically. (Distinguished-mode error)
    NotCanonical,
    /// Unknown fields were encountered. (Distinguished-mode error)
    UnknownField,
    /// Recursion limit was reached when parsing.
    RecursionLimitReached,
    /// Size of a length-delimited region exceeds what is supported on this platform.
    Oversize,
    /// Something else.
    Other,
}

use DecodeErrorKind::*;

impl fmt::Display for DecodeErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Truncated => "message or region truncated",
            InvalidVarint => "invalid varint",
            TagOverflowed => "tag overflowed",
            WrongWireType => "wire type not understood by encoder",
            OutOfDomainValue => "value out of domain",
            InvalidValue => "invalid value",
            ConflictingFields => "conflicting mutually-exclusive fields",
            UnexpectedlyRepeated => "unexpectedly repeated item",
            NotCanonical => "value not encoded canonically",
            UnknownField => "unknown field",
            RecursionLimitReached => "recursion limit reached",
            Oversize => "region too large to decode",
            Other => "other error",
        })
    }
}

/// A Bilrost message decoding error.
///
/// `DecodeError` indicates that the input buffer does not contain a valid Bilrost message. The
/// error details should be considered 'best effort': in general it is not possible to exactly
/// pinpoint why data is malformed.
///
/// `DecodeError` is 1 word plus 1 byte in size with the "detailed-errors" feature enabled; without
/// that feature, it is only 1 byte, and the error will not include any information about the path
/// to the fields that encountered the error while decoding.
#[derive(Clone, PartialEq, Eq)]
pub struct DecodeError {
    /// A 'best effort' root cause description.
    kind: DecodeErrorKind,
    #[cfg(feature = "detailed-errors")]
    /// A stack of (message, field) name pairs, which identify the specific
    /// message type and field where decoding failed. The stack contains an
    /// entry per level of nesting.
    stack: thin_vec::ThinVec<(&'static str, &'static str)>,
}

impl DecodeError {
    /// Creates a new `DecodeError` with a 'best effort' root cause description.
    ///
    /// Meant to be used only by `Message` implementations.
    #[doc(hidden)]
    #[cold]
    pub fn new(kind: DecodeErrorKind) -> DecodeError {
        DecodeError {
            kind,
            #[cfg(feature = "detailed-errors")]
            stack: Default::default(),
        }
    }

    /// Returns the kind of this error.
    pub fn kind(&self) -> DecodeErrorKind {
        self.kind
    }

    /// Pushes a (message, field) name location pair on to the location stack.
    ///
    /// Meant to be used only by `Message` implementations.
    #[doc(hidden)]
    pub fn push(&mut self, message: &'static str, field: &'static str) {
        #[cfg(feature = "detailed-errors")]
        self.stack.push((message, field));
        _ = (message, field);
    }
}

impl fmt::Debug for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut s = f.debug_struct("DecodeError");
        s.field("description", &self.kind);
        #[cfg(feature = "detailed-errors")]
        s.field("stack", &self.stack);
        s.finish()
    }
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("failed to decode Bilrost message: ")?;
        #[cfg(feature = "detailed-errors")]
        for (message, field) in self.stack.iter() {
            write!(f, "{}.{}: ", message, field)?;
        }
        self.kind.fmt(f)
    }
}

#[cfg(feature = "std")]
impl std::error::Error for DecodeError {}

#[cfg(feature = "std")]
impl From<DecodeError> for std::io::Error {
    fn from(error: DecodeError) -> std::io::Error {
        std::io::Error::new(std::io::ErrorKind::InvalidData, error)
    }
}

/// A Bilrost message encoding error.
///
/// `EncodeError` always indicates that a message failed to encode because the
/// provided buffer had insufficient capacity. Message encoding is otherwise
/// infallible.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct EncodeError {
    required: usize,
    remaining: usize,
}

impl EncodeError {
    /// Creates a new `EncodeError`.
    pub(crate) fn new(required: usize, remaining: usize) -> EncodeError {
        EncodeError {
            required,
            remaining,
        }
    }

    /// Returns the required buffer capacity to encode the message.
    pub fn required_capacity(&self) -> usize {
        self.required
    }

    /// Returns the remaining length in the provided buffer at the time of encoding.
    pub fn remaining(&self) -> usize {
        self.remaining
    }
}

impl fmt::Display for EncodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "failed to encode Bilrost message; insufficient buffer capacity \
            (required: {}, remaining: {})",
            self.required, self.remaining
        )
    }
}

#[cfg(feature = "std")]
impl std::error::Error for EncodeError {}

#[cfg(feature = "std")]
impl From<EncodeError> for std::io::Error {
    fn from(error: EncodeError) -> std::io::Error {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, error)
    }
}
