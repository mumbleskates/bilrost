## v0.1004.0-dev

### Breaking changes

* The "encoder" attribute for message & oneof fields is now "encoding". This
  reads better and makes more sense, better to do it now.
* Conversion to & from `u32` now uses the `Enumeration` trait rather than
  `Into<u32>` and `TryFrom<u32>`.

### New features

* There is now an `Enumeration` trait for converting to & from `u32` from
  derived enumerations.

### Fixes

* Fixed some incorrect documentation.

### Cleanups

* Large refactor that changes the underlying `Encoder` traits from
  `impl Encoder<Value> for Encoding` to `impl Encoder<Encoding> for Value`.

## v0.1003.1

### Fixes

* Fixed some incorrect documentation.

## v0.1003.0

### Breaking changes

* Removed the recently added `RequireCanonicity` trait and fold its
  functionality into `WithCanonicity` now that we figured out how to spell that.

### Fixes

* `WithCanonicity::canonical_with_extensions` no longer swaps the meaning of
  `Canonical` and `NotCanonical`.
* More aggressive inlining for very hot functions may increase encoding/decoding
  performance significantly.
* Decoding messages with mixed packed and unpacked representations in the same
  field is now always an error, regardless of what order they appear in. This
  was formerly a constraint of the way unpacked fields were decoded.

### Cleanups

* The readme is feature-complete!

## v0.1002.1

### Fixes

* `WithCanonicity::canonical_with_extensions` and
  `RequireCanonicity::allow_extensions` no longer swap the meaning of
  `Canonical` and `NotCanonical`.

## v0.1002.0

### Breaking changes

* Distinguished decoding traits now still succeed without error when decoding
  non-canonical data, but additionally return information about the canonicity
  of the message. The three levels of canonicity are "Canonical" (the only level
  that was accepted previously), "HasExtensions" (all known fields are
  canonical, but there are unknown fields), and "NotCanonical" (known fields
  have non-canonically represented values).
* As part of fixes and expansions to traits and requirements to allow `Message`
  and `DistinguishedMessage` to be object-safe with full functionality,
  `MessageDyn` and `DistinguishedMessageDyn` have been removed.

### New features

* The `Canonicity` enum has been introduced at the crate level, returned as
  additional information by distinguished decoding traits.
* Since distinguished encoders now return `Result<Canonicity, DecodeError>`
  or `Result<(T, Canonicity), DecodeError>`, new helper traits have been added
  to allow converting this information into errors when it is
  unacceptable: `WithCanonicity` and `RequireCanonicity`.

### Fixes

* Object-safe traits were broken and unfun to use. They've been implemented in a
  much more correct way now.
* Derived Enumeration types now convert via `TryFrom<u32, Error = u32>` instead
  of `Error = DecodeError`. The old implementation wasn't really helping anyone
  by discarding the untranslated value.

### Cleanups

* Significant readme expansion and organization.

## v0.1001.0

### Breaking changes

* Changed the name of the `detailed-errors` feature to have a hyphen instead of
  an underscore, consistent with the other feature names.
* Renamed the `vecblob` encoder to `plainbytes`.
* Encoded bilrost values and semantics no longer rely on the `Default`
  implementation for their empty values.
    * `Message` no longer requires, nor does its derive macro provide, an
      implementation of `Default`.
    * `Enumeration` no longer cares whether the type has a `Default`
      implementation, only whether there is a variant whose Bilrost value is
      exactly `0`.
    * All of the above duties are taken on by the `HasEmptyState` trait, as does
      the base implementation for `NewForOverwrite`.
* Following on that, renamed the `HasEmptyState` trait to `EmptyState`.
* Usability fixes for `Blob`: `new(..)` has been changed to create an empty
  `Blob` with no arguments, and the functionality for wrapping a vec has been
  renamed to `from_vec(..)`.
* Added APIs for `replace_from(..)` etc. to the regular non-dyn `Message`
  traits, which is useful for messages with ignored fields but requires those
  same APIs in the `MessageDyn` and `DistinguishedMessageDyn` traits to be
  renamed to `replace_from_dyn(..)` etc.

### New features

* Added features that enable support of `smallvec`, `thin-vec`, and `tinyvec`.
* Added support for `u16` and `i16` with the `general` encoder, and added a
  new `varint` encoder that supports all the varint types in addition to `u8`
  and `i8`. `general` will not support one-byte integers, because this makes it
  too easy to accidentally spell a completely unintended encoding of `Vec<u8>`;
  encodings for collections of bytes like this will remain explicit.
* Added support for `[u8; N]` with the `plainbytes` encoder, which only accepts
  values of the correct length.
* Added support for `[u8; 4]` and `[u8; 8]` with the `fixed` encoder.
* Added support for marking message fields with `#[bilrost(ignore)]`, which
  causes the field to be excluded from encoding and decoding but precludes
  distinguished decoding.
* Added `replace_from_slice(&[u8])`
  and `replace_distinguished_from_slice(&[u8])` to the `MessageDyn`
  and `DistinguishedMessageDyn` traits.
* Changed value-decoding to pass down whether or not an empty value is allowed,
  allowing implementations to err sooner and cheaper by detecting that the
  encoded data is that which represents the empty value, rather than always
  checking the value for emptiness after the fact.

### Fixes

* Require the `serde_json/float_roundtrip` feature for
  the `bilrost-types/serde_json` compatibility. If that feature is desired to be
  disabled, the `serde_json` feature in `bilrost-types` currently only provides
  from/into anyway and those can be written elsewhere.

### Cleanups

* Deduplicated implementations of `Encoder` and `DistinguishedEncoder` that
  blanket all implementations for which those encoders support value-encoding.
* Great strides in expanding and cleaning up the documentation.

## v0.1000.0

This is the first rough release of `bilrost`. It is largely tested and feature
complete.

Some breaking refactors to the *internal* apis (those exposed within the
`encoding` module, still hidden from the docs for now) may appear in subsequent
versions, but the plan is that anything that works correctly and is exposed
directly in the root `bilrost` module should continue to work the same,
including everything in user-facing traits and re-exported derive macros.

Significant work in expanding the documentation and readme remains, and fuzzing
still needs to be reenabled.
