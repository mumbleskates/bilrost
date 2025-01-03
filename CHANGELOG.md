## v0.1011.0

### Breaking changes

* The (unstable) internal encoding traits & types continue to evolve.
  * `Oneof` traits now encode and decode slightly differently and the traits
    bearing an empty state now have special responsibility for guarding against
    value duplication and recording error locations.
  * Distinguished encoding traits now use a different context type,.
    `RestrictedDecodeContext`, which restricts the minimum tolerated canonicity
    and allows for early exits and detailed errors about the location of
    non-canonical data problems.
  * `DecodeContext` now has public method visibility.

### New features

* Added support for `core::time::Duration` and `std::time::SystemTime`.
* Added 3rd party type support for the `bstr` crate and its `BString` type,
  which is a wrapper around `Vec<u8>` that acts like text but does not guarantee
  UTF-8 and does not require any validation.
* Added 3rd party type support for the `chrono` and `time` crates and most of
  their important types, available through new crate features.
* `Oneof` types can now be wrapped in `Box` (multiple times even, and either
  side of `Option` if you really want.)
* `DistinguishedMessage`: added "restricted" and "canonical" decoding methods
  alongside the existing "distinguished" ones, allowing decoding to stop early
  on canonicity errors.

### Fixes

* Rectify an ambiguous usage of `PartialEq` that could potentially cause
  compilation failures when supported types in the program support cross-type
  equality.
* `bilrost-derive`: simplify some codegen to remove a needless `let mut` in
  derived decoding implementations.

### Cleanups

* `bilrost-types`: parsing fractional seconds with more than 9 digits now
  simply truncates instead of erring.
* `bilrost-types`: parsing timestamps with "unknown local offset" timezones
  ("-00:00") is now tolerated, since the timezone information is discarded
  anyway.
* `bilrost-types`: improved fuzzing binaries for datetime parsing.
* New keywords and categories have been added to the crate metadata.
* Various small cleanups to the readme and to the code as `rustfmt` and `clippy`
  develop more and stronger opinions.
* Major reorganization of type implementations, especially of common value-trait
  implementations and third-party types. All that code is now filed under
  `encoding::type_support` and conditionally enabled at the file level. Impls
  for primitive and core types in the builtin encoders are still in those
  encoders' modules, but the `value_traits` module now contains only traits and
  macros and all conditionally-enabled code has been moved into `type_support`.
* A new fuzzer binary is available specifically for the newer types which have
  the newer, slightly more abstract encoding paths. These fuzzers are typically
  run in the order of hundreds to thousands of CPU hours per significant change,
  and are available for you to run as well.

## v0.1010.1

### Cleanups

* `bilrost-types`: parsing fractional seconds with more than 9 digits now
  simply truncates instead of erring.
* `bilrost-types`: parsing timestamps with "unknown local offset" timezones
  ("-00:00") is now tolerated, since the timezone information is discarded
  anyway.
* `bilrost-types`: improved fuzzing binaries for datetime parsing.

## v0.1010.0

### New features

* The optimization-controlling crate features have changed. There are now three
  features for each: from lowest to highest priority, "auto-feature",
  "prefer-no-feature", and "feature". This gives downstream crates the ability
  to turn a given optimization either on or off even if a library enables the
  "auto" feature. Only the "prefer-no-*" crate features are new.

### Fixes

* Fixed a crash, wrong results for dates in early year 1900, and tolerance of
  some incorrect inputs in `bilrost_types::Timestamp`'s conversion from strings.

## v0.1009.0

### New features

* Added `encode_contiguous` and `encode_length_delimited_contiguous` APIs to the
  `Message` trait which use the `encode_fast` prepend-encoding code path but
  measure the size first to always produce a contiguous buffer.
* Added `into_vec(self)` to `ReverseBuffer` complete with a non-copying
  optimization when the buffer is full and contiguous, such as when it is
  produced by `Message::encode_contiguous`.

## v0.1008.0

### Breaking changes

* The (unstable) internal encoding traits continue to evolve, now making checks
  for whether a value decoded in distinguished mode was empty optional at the
  value-decoder trait level. It is not always faster to check emptiness while
  decoding a value, and letting the distinguished value decoder implementation
  specify whether it takes responsibility for checking emptiness enables new
  type support like better nesting of never-empty types as described below.

### Fixes

* Enumerations that do not have a defined zero value (and therefore do not have
  an "empty" state) can now be nested in fixed-size arrays as long as they are
  nested again, such as in an `Option` or `Vec`.

### Cleanups

* Renamed the `NewForOverwrite` value trait to `ForOverwrite`.
* Restructured so that `ForOverwrite` is now a supertrait of `EmptyState`, and
  every existing implementation of `EmptyState::empty` defers to `ForOverwrite`.

## v0.1007.0

### Breaking changes

* Tuple typed structs (with anonymous fields accessed like `value.0`) now start
  their field numbering from zero by default, matching the names of the fields.
* Renamed the `.reader()` method on `ReverseBuffer` to `.buf_reader()` so it no
  longer conflicts with the `bytes::Buf` method.

### New features

* Added support for encoding `usize` and `isize` pointer-sized integers. They
  will still have different supported maximums on platforms with different sized
  values, but this still has completely reasonable failure modes.
* Added support for deriving `Message` for `enum` types, which is implemented in
  terms of `Oneof`. The message implementations encode and decode exactly like a
  message which contains only that oneof; see the readme for more details.
* Changed the `Value` type in `bilrost-types` to be a message-via-oneof in this
  way, making it much nicer to use without changing its meaning.
* Added a `.slices()` method to `ReverseBuffer` and `ReverseBufferReader` which
  iterates its slices, useful for sending to `write_vectored`.

### Fixes

* Fixed some incorrect tests, including coverage of third-party inline vecs and
  packed/unpacked fixed-size arrays.

### Cleanups

* Cleaned up and re-added fuzzers! Fuzzers are available via both `libfuzzer`
  and `afl`; see [`FUZZING.md`](./FUZZING.md) for usage details. These fuzzers
  would have caught the bug fixed in 0.1006.1 :)
* Deduplicated and somewhat improved codegen for oneofs. All four kinds of oneof
  decoder function are now generated by the same code, which is a huge
  improvement over four independent generators.
* Added tests for detailed errors attribution of decoding errors to the specific
  field that caused the problem.

## v0.1006.1

### Fixes

* BUGFIX: Messages that have multiple occurrences of the *same* field that is
  part of a oneof will now always error, rather than silently replacing the
  value.

## v0.1006.0

### Breaking changes

* The (unstable) internal encoding traits continue to evolve, now varying the
  implementation of `TagMeasurer` and moving the "allow empty" argument of
  distinguished value decoding into a const generic param.

### New features

* Hash maps and hash sets can now have any kind of hasher, as long as the
  `BuildHasher` implements `Default`.
* Plain tuples can now be encoded as message fields! Tuples encode exactly the
  same as messages with field tags 0 through N-1, but the encoding of each field
  in the tuple can be specified. See the readme for more information.
* Fixed-size arrays (`[T; N]`) can now also be encoded as message fields. Arrays
  encode exactly the same as a collection like `Vec` would, except if each value
  in the array is empty the whole array is considered empty. When decoding an
  array, if a nonzero number of items is present and it is not the same number
  of items as the size of the array, the value and message being decoded are
  considered to be invalid.
* A couple third-party alternative types for `Vec` that have bounded size and
  inline-only storage (`ArrayVec` from the `arrayvec` and `tinyvec` crates) have
  been added.
* Third-party alternative types for `Vec` now also work with `u8` items and the
  `plainbytes` encoding.

### Fixes

* Added a `Self: Default` bound to the impl for `Message` when there are ignored
  fields. This means that it can be possible to have a message type with ignored
  fields and generically typed fields that don't implement `Default`; rather
  than failing to compile, it will now simply not implement `Message`.

### Cleanups

* Consolidated (almost) all the standard impls of the `EmptyState` trait into
  the same module.

## v0.1005.1

This release changes nothing, just fixes the link to the logo in the docs :)

## v0.1005.0

### Breaking changes

* The (unstable) internal encoding traits continue to evolve, this time to
  support prepend-encoding.

### New features

* Added prepend-encoding: messages can be encoded to a prepend-only buffer, with 
  `.prepend(reverse_buf)` or `.encode_fast()`. This encoding method writes
  messages in reverse, avoiding any need to "look ahead" in order to encode the
  correct length for data that is not yet written. This enables writing an
  arbitrarily nested message without visiting any field more than once, removing
  a quadratic hazard and general inefficiency in the encoding path.
* New `reserved_tags` attribute on messages to prevent tags from being used,
  even by accident.
* Both the new `reserved_tags` and the old `oneof` attributes can now specify
  inclusive ranges of tag numbers instead of only single tags. For now, in
  `oneof` this is limited to 100 tags per range because more than that is just
  too many.
* `bilrost-derive`, which contains the derive macros, is now `no_std`. This
  doesn't really change what it's capable of at all but it does make it easier
  to prove it doesn't accidentally preclude using `std`.

### Fixes

* Explicitly instantiate and invoke the const-time assertions that check the
  tags match between a Oneof type and its inclusion in a Message. Before this
  the asserts probably would never run.

### Cleanups

* The "opaque" message types are now always available, and no longer require a
  dependency or a crate feature. The "opaque" feature will be removed in a
  future version.
* The "derived message tests" have been moved from a binary with required
  features to an integration test. The "derive" feature and the "bilrost-derive"
  crate dependency are now always enabled in tests to support this.

## v0.1004.0

### Breaking changes

* The "encoder" attribute for message & oneof fields is now "encoding". This
  reads better and makes more sense, better to do it now.
* Conversion to & from `u32` now uses the `Enumeration` trait rather than
  `Into<u32>` and `TryFrom<u32>`.
* The "opaque" types, `bilrost::encoding::opaque::{OpaqueMessage, OpaqueValue}`
  now use `Cow` under the hood instead of `Vec`, allowing them to hold borrowed
  data rather than only owned.

### New features

* There is now an `Enumeration` trait for converting to & from `u32` from
  derived enumerations.

### Fixes

* Fixed some incorrect documentation.

### Cleanups

* Large refactor that changes the underlying `Encoder` traits from
  `impl Encoder<Value> for Encoding` to `impl Encoder<Encoding> for Value`. This
  avoids issues with a new "non_local_definitions" lint which fires when trait
  implementations are derived for a function-local type.

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
  and `DistinguishedMessage` to be object-safe (dyn compatible) with full
  functionality, `MessageDyn` and `DistinguishedMessageDyn` have been removed.

### New features

* The `Canonicity` enum has been introduced at the crate level, returned as
  additional information by distinguished decoding traits.
* Since distinguished encoders now return `Result<Canonicity, DecodeError>`
  or `Result<(T, Canonicity), DecodeError>`, new helper traits have been added
  to allow converting this information into errors when it is
  unacceptable: `WithCanonicity` and `RequireCanonicity`.

### Fixes

* Object-safe (dyn compatible) traits were broken and unfun to use. They've been
  implemented in a much more correct way now.
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
