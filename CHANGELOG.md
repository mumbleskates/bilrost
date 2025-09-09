## v0.1014.1-dev

### New features

* Allows intercompatibility with `hashbrown 0.16`

## v0.1014.0

### Breaking changes

There should be no breaking changes this release.

### New features

* Headline feature: Added the "message" attribute for oneof variants which
  enables oneof variants with any number of fields, encoding a sub-message as
  the variant's value without a second struct type to nest as a value.
* Added support for the `#[bilrost(reserved_tags(..))]` attribute for the
  `Oneof` derive macro as well.
* The `Enumeration` derive macro now supports `Tuple()` and `Struct { }`
  variants as long as they have no fields.
* Added the "empty" attribute for oneof variants which allows explicitly marking
  empty variants.
* Added support for `Range<T>` and `RangeInclusive<T>`, which encode and decode
  the same as `(start, end)` tuples.
* Added support for the `NonZero` integer types, which are considered non-empty.
  These types must be wrapped in an `Option` or another container to appear in
  a message field.

### Fixes

* Don't imply that that general encodings may output 32 & 64 bit integers in a
  fixed-size representation in the readme documentation.

### Cleanups

* Major cleanups to most parts of the derive macro codegen implementation.
* Various cleanups to string formatting code.

## v0.1013.0

### Breaking changes

#### For normal use

* There should be no breaking changes. Any breaking changes may be reported as
  bugs and will be either fixed or documented in a patch release.

#### Advanced usage

Since 0.1012:

* The `bilrost::encoding::General` encoding type has become a specific
  definition of a generic type, `bilrost::encoding::GeneralGeneric<P>`. Anything
  that implements encoders specifically for `General` or sets up encoding
  delegation for it may find that it no longer works inside nested values or in
  `Oneof` variants, since those values are encoded with a specific definition of
  the generic. If this is a problem, it can be solved one of two ways:
  1. explicitly annotate the encodings of the values that aren't working with
     the "general" encoding, overriding "general_packed"
  2. change the implementation or delegation to be not just for `General`, but
     for `GeneralGeneric<P>` for all `const P: u8` instead; this will include
     both of the general encodings.
* Renamed `OpaqueMessage::{borrowed, convert_to_owned}` to `to_borrowed` and
  `into_owned`, and `OpaqueValue::convert_to_owned` to `into_owned` to better
  match [common naming conventions](
  https://rust-lang.github.io/api-guidelines/naming.html#ad-hoc-conversions-follow-as_-to_-into_-conventions-c-conv)
* The "fixed" encoding no longer automatically covers `Vec<T>` by delegating to
  "unpacked<fixed>" when `T` is supported by the "fixed" encoding.
* Virtually all the internal encoding traits have changed to better facilitate
  second party implementations for third-party types.
  * Each updated trait's form has changed from `impl Encoder<E> for T` to
    `impl Encoder<E, T> for ()`.

Since 0.1013.0-rc.4:

* Removed the `new_proxy` method from the `Proxiable` trait; the `Proxy` type
  that your type will be encoded as must have `ForOverwrite` in the encoding `E`
  that is used to encode it instead.

### New features

#### For normal use

* The `Message` trait now has the methods `new_empty`, `message_is_empty`, and
  `clear_message`. These provide the same functionality that was available by
  using `EmptyState` from the advanced-usage space traits previously, but that
  trait is no longer available to `dyn` messages in a usable form.
* There is a new encoding available, `general_packed` (exported as
  `bilrost::encoding::GeneralPacked`) that defaults to packed representations
  rather than unpacked for its supported collection types.
  * `general_packed` is now the implicit default when no encoding is specified
    for `Oneof` variants, as well as the default inner encoding for values
    nested inside `packed`, `unpacked`, and `map` values.
  * In all other ways, `general_packed` should behave the same as `general`.
  * This means that *many* new types can now be encoded without specifying an
    explicit field `encoding`. This can be very helpful as the compiler error
    messages from the missing trait are unlikely to ever be very good.
* Ignored fields, via the `#[bilrost(ignore)]` attr, no longer always require
  the whole message struct to implement `Default`; when the message struct is
  given the `#[bilrost(default_per_field)]` attr, only the types of the
  individually ignored fields need to implement `Default`.

#### Advanced usage

* Added (or publicized) some new macros for facilitating advanced usage:
  * `encoding_implemented_via_value_encoding!` -- implements encoding/decoding
    for message fields for all types where encoding/decoding of values is
    available and the `EmptyState` trait is implemented. Recommended for
    virtually all encodings.
  * `encoding_uses_base_empty_state!` -- delegates all `EmptyState` and
    `ForOverwrite` implementations to the "base" implementations used by the
    encodings in the `bilrost` crate. See the "proxy_own_type" example.
  * `implement_core_empty_state_rules!` -- adds some covering implementations
    of the `EmptyState` and `ForOverwrite` traits for a custom encoding intended
    to be used for types not owned by your crate. See the
    "proxy_third_party_type" example.
* The `Sized` constraints have been relaxed on the `Encoder`, `ValueEncoder`,
  and `WireTyped` traits.
* Added some examples to the crate demonstrating newly-possible advanced usage
  patterns for implementing encoding & decoding types not naturally supported by
  the `bilrost` crate, for cases when those types are and are not owned by your
  own crate.

### Fixes

* The `empty_state_via_default!` macro no longer produces malformed output when
  used with a generic.

## v0.1013.0-rc.4

### Fixes

* Fixed the broken `Unpacked` encoding for `[T]` and covered that type with
  tests by delegating existing types' encoding to it.

## v0.1013.0-rc.3

### New features

* Relaxed the `Sized` constraint in the `FieldEncoder` trait as well.
* Added explicit encoding implementations for `[T]` in the `Packed` and
  `Unpacked` encodings.

## v0.1013.0-rc.2

### Fixes

* Relaxed the `Sized` constraint on the `Encoder` and `ValueEncoder` traits.
  * This allows adding bounds like `(): Encoder<E, [T]>` when an encoding `E`
    is encoding a slice of values.

## v0.1013.0-rc.1

### Breaking changes

* The `bilrost::encoding::General` encoding type has become a specific
  definition of a generic type, `bilrost::encoding::GeneralGeneric<P>`. Anything
  that implements encoders specifically for `General` or sets up encoding
  delegation for it may find that it no longer works inside nested values or in
  `Oneof` variants, since those values are encoded with a specific definition of
  the generic. If this is a problem, it can be solved one of two ways:
  1. explicitly annotate the encodings of the values that aren't working with
     the "general" encoding, overriding "general_packed"
  2. change the implementation or delegation to be not just for `General`, but
     for `GeneralGeneric<P>` for all `const P: u8` instead; this will include
     both of the general encodings.
* Renamed `OpaqueMessage::{borrowed, convert_to_owned}` to `to_borrowed` and
  `into_owned`, and `OpaqueValue::convert_to_owned` to `into_owned` to better
  match [common naming conventions](
  https://rust-lang.github.io/api-guidelines/naming.html#ad-hoc-conversions-follow-as_-to_-into_-conventions-c-conv)
* The "fixed" encoding no longer automatically covers `Vec<T>` by delegating to
  "unpacked<fixed>" when `T` is supported by the "fixed" encoding.
* Virtually all the internal encoding traits have changed to better facilitate
  second party implementations for third-party types.
  * Each updated trait's form has changed from `impl Encoder<E> for T` to
    `impl Encoder<E, T> for ()`.

### New features

* The `Message` trait now has the methods `new_empty`, `message_is_empty`, and
  `clear_message`. These provide the same functionality that was available by
  using `EmptyState` previously, but that trait is no longer available to `dyn`
  messages in a usable form.
* There is a new encoding available, `general_packed` (exported as
  `bilrost::encoding::GeneralPacked`) that defaults to packed representations
  rather than unpacked for its supported collection types.
  * `general_packed` is now the implicit default when no encoding is specified
    for `Oneof` variants, as well as the default inner encoding for values
    nested inside `packed`, `unpacked`, and `map` values.
  * In all other ways, `general_packed` should behave the same as `general`.
  * This means that *many* new types can now be encoded without specifying an
    explicit field `encoding`. This can be very helpful as the compiler error
    messages from the missing trait are unlikely to ever be very good.
* Ignored fields, via the `#[bilrost(ignore)]` attr, no longer always require
  the whole message struct to implement `Default`; when the message struct is
  given the `#[bilrost(default_per_field)]` attr, only the types of the
  individually ignored fields need to implement `Default`.
* Added (or publicized) some new macros for facilitating advanced usage:
  * `encoding_implemented_via_value_encoding!` -- implements encoding/decoding
    for message fields for all types where encoding/decoding of values is
    available and the `EmptyState` trait is implemented. Recommended for
    virtually all encodings.
  * `encoding_uses_base_empty_state!` -- delegates all `EmptyState` and
    `ForOverwrite` implementations to the "base" implementations used by the
    encodings in the `bilrost` crate. See the "proxy_own_type" example.
  * `implement_core_empty_state_rules!` -- adds some covering implementations
    of the `EmptyState` and `ForOverwrite` traits for a custom encoding intended
    to be used for types not owned by your crate. See the
    "proxy_third_party_type" example.

### Fixes

* The `empty_state_via_default!` macro no longer produces malformed output when
  used with a generic.

## v0.1012.3

### Fixes

* Loosened some erroneous constraints on `Option<T>` that prevented borrow-only
  types from being decodable when wrapped in `Option`.

## v0.1012.2

### Fixes

* BUGFIX: Message implementations derived for oneof types no longer fail to skip
  the data in unknown fields that the mssage also contains.

## v0.1012.1

### Fixes

* Internals macros: Fixed the `empty_state_via_for_overwrite` macro, which was
  incompletely implemented and still referenced the `Default` trait.

## v0.1012.0

### Breaking changes

* This release includes a major overhaul of encoding and decoding traits for
  the library.

  | Capability                        | Old trait              | New trait                          |
  |-----------------------------------|------------------------|------------------------------------|
  | encoding                          | `Message`              | `Message`                          |
  | relaxed decoding (owned)          | `Message`              | `OwnedMessage`                     |
  | distinguished decoding (owned)    | `DistinguishedMessage` | `DistinguishedOwnedMessage`        |
  | relaxed decoding (borrowed)       | (new!)                 | `BorrowedMessage<'a>`              |
  | distinguished decoding (borrowed) | (new!)                 | `DistinguishedBorrowedMessage<'a>` |

  For very simple usage of the `bilrost` library, this will now probably mean
  importing both `Message` and `OwnedMessage` traits to have the desired
  functionality in scope.

* The `DistinguishedMessage` and `DistinguishedOneof` traits & derives are gone
  as well; rather than deriving multiple traits, simply add a
  `#[bilrost(distinguished)]` attribute to the type being derived from:

  | Old derives                                    | New derives                                                      |
  |------------------------------------------------|------------------------------------------------------------------|
  | `Message`, `DistinguishedMessage`              | `Message` with `#[bilrost(distinguished)]` on the struct         |
  | `Oneof`, `DistinguishedOneof`                  | `Oneof` with `#[bilrost(distinguished)]` on the enum             |
  | all of the above                               | `Message` & `Oneof` with `#[bilrost(distinguished)]` on the enum |
  | just using `Message`, `Oneof`, & `Enumeration` | (no change)                                                      |

### New features

* It is now possible to do borrowed zero-copy decoding, which is enabled by
  default and available in the derive macros. This decodes from a `&[u8]` slice
  with lifetime into messages that may reference its data.
  * This adds support for the types `&str`, `&[u8]`, `&[u8; N]`, and
    `&bstr::BStr`; these types can appear in message fields, oneof fields, and
    nested in other containers just like any other type. This also adds
    guaranteed behavior for `Cow` for these borrowed types also decodes as
    `Cow::Borrowed(&..)` when decoding from borrowed data.
  * With this addition, there are now two different ways to have zero-copy
    decoding that each work slightly differently:
    1. Decode directly from `bytes::Bytes` and into fields of type
       `bytes::Bytes` or `bytestring::Bytestring`. This yields owned, refcounted
       handles to the original data.
    2. Decode borrowed from `&[u8]` and into fields of type `&str`, `&[u8]`,
       `&[u8; N]`, or `&bstr::BStr`. This yields data borrowed for a lifetime at
       very low cost, protected by the borrow checker rather than a refcount.
* Derive macros are now simpler to use, so now deriving all encoding and
  decoding impls for messages and oneofs is done only with `Message` and
  `Oneof`, and distinguished implementations are switched on and off by
  attribute.
* Opened the gates for crate documentation in the `encoding` module as the crate
  is getting closer to what could become a stable release.
* Added `From<Vec<u8>>` and `From<Box<[u8]>>` impls for `ReverseBuffer`.
* Added new forms of ranges in the `reserved_tags` attribute: `5..` and `..=5`.
* **EXPERIMENTAL**: Made public a couple macros and the proxying traits &
  encoding type; see `encoding::{Proxied, Proxiable}` for details.
  * These can be used even to encode third-party types foreign to both your own
    crate and to `bilrost` (via type-tagged impls) and completely break the
    guarantees of the `bilrost` library. I do my best, but correctness is in
    your hands!

### Fixes

* Internals: It should no longer be possible for restricted and canonical
  message decoding modes to return data or canonicity that is less than the
  restriction level that was specified, if a decoding implementation returns a
  lower canonicity but forgets to check against the restriction in the context.
  The worst that should happen is that the error is raised late, at the end of
  decoding, when it is too late to add information about the location of the
  error. There are also debug-only assertions that test that this should never
  happen, and explanatory documentation about exactly when a `Canonicity` should
  be checked against the restricted context on `RestrictedDecodeContext::check`.
  * It's unlikely this should change any behavior as formerly the canonicity was
    checked very aggressively in all existing implementations, far more often
    than it had to be.

### Cleanups

* Changed internal and external phrasing from "expedient" encoding to "relaxed".
* More reorganization and file cleanups, splitting up some large files into more
  modules etc.
* Cleaned up some docs in the `encoding` module.
* Improved type coverage in the fuzz testing modules and gave the message
  definitions fixed field tags so existing fuzzing corpora will be maximally
  useful.
* Internals: Ironed out a lingering annoyance with the field decoding APIs; the
  `Decoder` traits no longer accept a `duplicated` boolean argument that
  mandates returning an error when it is true. Instead, message implementations
  that have defined fields are responsible for creating the
  `UnexpectedlyRepeated` decoding error themselves.

## V0.1011.1

### Fixes

* Oneof enums can now implement distinguished decoding even when one or more of
  their variants has a type with no "empty" state. 🎊

## v0.1011.0

### Breaking changes

* The (unstable) internal encoding traits & types continue to evolve.
  * `Oneof` traits now encode and decode slightly differently and the traits
    bearing an empty state now have special responsibility for guarding against
    value duplication and recording error locations.
  * Distinguished encoding traits now use a different context type,
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
