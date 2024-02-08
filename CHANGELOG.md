## v0.1001.0-dev

### Breaking changes

* Changed the name of the `detailed-errors` feature to have a hyphen instead of
  an underscore, consistent with the other feature names.
* Renamed the `vecblob` encoder to `plainbytes`.

### New features

* Added features that enable support of `smallvec`, `thin-vec`, and `tinyvec`.

### Fixes

### Cleanups

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
