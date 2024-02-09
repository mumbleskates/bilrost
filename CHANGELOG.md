## v0.1001.0-dev

### Breaking changes

* Changed the name of the `detailed-errors` feature to have a hyphen instead of
  an underscore, consistent with the other feature names.
* Renamed the `vecblob` encoder to `plainbytes`.

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

### Fixes

* Require the `serde_json/float_roundtrip` feature for
  the `bilrost-types/serde_json` compatibility. If that feature is desired to be
  disabled, the `serde_json` feature in `bilrost-types` currently only provides
  from/into anyway and those can be written elsewhere.

### Cleanups

* Deduplicated implementations of `Encoder` and `DistinguishedEncoder` that
  blanket all implementations for which those encoders support value-encoding.

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
