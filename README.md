# *BILROST!*

Bilrost is a binary encoding format designed for storing and transmitting
structured data. The encoding is binary, and unsuitable for reading directly by
humans; however, it does have other other useful properties and advantages. This
crate, `bilrost`, is its first implementation and its first instantiation.
Bilrost (as a specification) strives to provide a superset of the capabilities
of protocol buffers while reducing some of the surface area for mistakes and
surprises; `bilrost` (the implementing library) strives to provide access to all
of those capabilities with maximum convenience.

Bilrost at the encoding level is based upon [Protocol Buffers][pb] (protobuf)
and shares many of its traits. It is in some ways simpler and less rigid in its
specification, and is designed to improve on some of protobuf's deficiencies. In
doing so it breaks wire-compatibility with protobuf.

`bilrost` is implemented for the [Rust Language][rs]. It is a direct fork of
[`prost`][p], and shares many of its performance characteristics. (It is not the
fastest possible encoding library, but it is still pretty fast and comes with
unique advantages.) Like `prost` bilrost can enable writing simple, idiomatic
Rust code with `derive` macros that serialize and deserialize structs from
binary data. Unlike `prost`, `bilrost` is free from most of the constraints of
the protobuf ecosystem and required semantics of protobuf message types.
Bilrost (the specification) and this library are free to allow much more
compatibility with existing types and their normal semantics. Rather than
relying on producing generated code from a protobuf `.proto` schema definition,
`bilrost` is designed to be easily used by hand.

🌈

[pb]: https://developers.google.com/protocol-buffers/

[rs]: https://www.rust-lang.org/

[p]: https://github.com/tokio-rs/prost

TODO: fill out this outline for a better introduction

## Conceptual overview

* the overall concepts
    * tagged fields
    * forwards and backwards compatibility as message types are extended
    * distinguished encoding
    * some semantics depend upon the types themselves, like defaults and
      maybe ordering

## Design philosophy

* the philosophy
    * data has meaning based on where you find it
    * encodings with explicit schemas can be easier to guess the meaning of
      if you don't already know, but most of the time this is wasted bytes
    * therefore it is often very sensible to encode data in a way that is
      legible to you, with implicit schemas and room for extending
    * the data you get back when decoding should not transform any of the field
      values except by omitting extensions (no int coercion)
    * as an encoding, bilrost works to make invalid states unrepresentable
      when practical where it doesn't greatly increase complexity
    * bilrost is designed to aid, but not require, distinguished encoding

## Using the library

* using the library
    * how to derive
        * field annotations
    * types that work with `bilrost`
    * encoders
    * custom encoders

## What Bilrost and the library won't do

* the (current lack of) ecosystem compared to protobuf
    * no reflection yet
    * no DSL for specifying schemas yet
    * no support across other languages yet

## Encoding specification

* encoding specification
    * messages as strings of bytes that encode zero or more fields
    * varint encoding
    * fixed-width encodings must be little-endian
    * field keys and wire types
    * complex types
        * unpacked encodings for vecs and sets
        * packed encodings for vecs and sets
        * map encodings
    * disallowed decoding constraints
        * unexpectedly repeated fields must err
        * out-of-domain values must err
        * text strings with invalid utf-8 must err
        * sets with duplicated items must err
        * maps with duplicated keys must err
        * oneofs with conflicting fields must err
    * additional decoding constraints for distinguished
        * fields must implement `Eq`
        * fields must never be present in the decoded data when they have the
          default value
        * unknown fields must err
        * maps' keys and sets' items must be ordered

## Differences from Protobuf encoding & semantics

* major changes in relation to protobuf and the history there
    * bijective varints
        * what leb128 varints gave protobuf
            * simplicity
            * zero-extension
        * what bijective varints give us
            * ez distinguished encoding
            * very very close to the same read/write cost
            * smaller size
    * no non-zigzag signed varints
        * these are just an efficiency footgun really
        * even the `int32` protobuf type uses 10 entire bytes to encode negative
          numbers just in case the type is widened to `int64` later. savings
          seem minimal
    * no integer domain coercion
        * the data you get back should mean what it said or fail
    * field ordering
        * what unordered fields gave protobuf
            * easy catting of partial messages
            * overlays
                * can be horrible, e.g. message fields upgraded to repeated
        * what ordered fields give us
            * no value stomping
            * easy detection of repeated violation without presence data
            * even makes required fields possible to detect, though that's
              not implemented
    * less constrained field tags
        * protobuf constrained the whole field key, including wire type, to 32
          bits. we can just not do that instead
    * no groups
        * historically these seem to be the original way data was nested, rather
          than nesting messages as length-delimited values
    * allowing packed length-delimited types
        * risk of upgrading them is considered the user's responsibility
    * first class maps
        * maps in protobuf were a pain and seem like a bodge
        * theoretically it's possible to widen that schema into a repeated
          nested message with more fields, but this is almost never done

## `bilrost` vs. `prost`

* comparisons to `prost`
    * does not generate implementations and structs from schemas, but rather
      makes deriving traits by hand ergonomic
    * `bilrost` uses trait-dispatched encoding instead of rigid types, which
      allows it to have far better type support
    * binary encoding is quite different, but just as capable (or more). for
      better or worse this is not a protobuf library.
        * a protobuf library, or fork of prost, could be created that uses the
          trait-based dispatch to be much easier to use
    * bilrost inherently supports deterministic & canonical outputs as a banner
      feature
    * message traits are now usefully object-safe, and all the encoder traits
      can function with `&dyn Buf` and so on
    * (look over more unsolved complaints in the prost issues)

---

---

### Comparisons to protobuf

* All varints (including tag fields and lengths) use [bijective numeration][bn],
  which cannot be length-extended with trailing zeros the way protobuf varints
  can (and are slightly more compact, especially at the 64bit limit where they
  take up 9 bytes instead of 10). Varint encodings which would represent a value
  greater than `u64::MAX` are invalid.
* "Groups" are completely omitted as a wire-type, reducing the number of wire
  types from 6 to 4.
* These four wire types are packed into 2 bits instead of 3.
* Fields are obligated to be encoded in order, and fields' tag numbers are
  encoded as the difference from the previous field's tag. This means that
  out-of-order fields cannot be represented in the encoding, and messages with
  many different fields consume relatively fewer bytes encoding their tags. The
  only time a field's tag is more than 1 byte is when more than 31 tag ids have
  been skipped.
* Field tags can be any `u32` value, without restriction.
* Signed varint types that aren't encoded with their sign bit in the LSB ("zig
  zag encoding") are omitted. There are no "gotcha" integer types whose negative
  values always take up the maximum amount of space.
* Enumerations are unsigned `u32`-typed, not `i32`.
* Narrow integral types that encode as a varint field, such as `i32` and `bool`,
  are checked and will cause decoding to err if their encoded range is
  overflowed rather than coercing to a valid value (such as any nonzero value
  becoming `true` in a `bool` field, or `u64::MAX` silently coercing to
  `u32::MAX`). Another way to spell this behavior is that values that would not
  round-trip are rejected during decoding.
* Generally, varint fields can always be upgraded to a wider type with the same
  representation and keep backwards compatibility. Likewise, fields can be
  "widened" from optional to repeated, but the encoded values are never
  backwards compatible when known fields' values would be altered or truncated
  to fit: decoding a bilrost message that has multiple occurrences of a non-
  repeated field in it is also an error.

[bn]: https://en.wikipedia.org/wiki/Bijective_numeration

### Strengths, Aims, and Advantages

Strengths of bilrost's encoding include those of protocol buffers:

* the encoded messages are very durable, with greatly extensible forward
  compatibility
* the encoded messages are relatively very compact, and their representation "on
  the wire" is very simple
* the encoding is minimally* platform-dependent; each byte is specified, and
  there are no endianness incompatibility issues
* when decoding, string and byte-string data is represented verbatim and can be
  referenced without copying
* skipping irrelevant or undesired data is inexpensive, as most nested and
  repeated is stored with a length prefix

...as well as more:

* bilrost supports distinguished encoding for types where it makes sense, and is
  designed from a protocol level to make invalid values unrepresentable where
  possible
* bilrost is more compact than protobuf without incurring significant overhead.
  nuances of representation in protobuf that bilrost cannot represent or has no
  analog for are either permanently deprecated, or all conforming decoders are
  required to discard the difference anyway.
* bilrost aims to be as ergonomic as is practical in plain rust, with basic
  annotations and derive macros. It's possible for such a library to be quite
  nice to use!

(*The main area of potential incompatibility is with the representation of
signaling vs. quiet NaN floating point values; see `f64::to_bits()`.)

#### Bilrost is *not...*

Bilrost does *not* have a robust reflection ecosystem. It does not (yet) have an
intermediate schema language like protobuf does, nor implementations for very
many languages, nor RPC framework support, nor an independent validation
framework. These things are possible, they just don't exist yet.

### Values and Encodings

Bilrost's basic unit of encoding is the message. Bilrost messages may have zero
or more fields, which each bear a corresponding numeric tag and are assigned an
encoder which determines how it is read and written from raw bytes.

#### Expedient vs. Distinguished Encoding

It is possible to derive an extended trait, `DistinguishedMessage`, which
provides a distinguished decoding mode. Decoding in distinguished mode comes
with an additional guarantee that the resulting message value will re-encode to
the exact same sequence of bytes, and that *every* different sequence of bytes
will either decode to a different value or fail to decode. Formally, values of
the message type are *bijective* to a subset of all byte strings, and all other
byte strings are considered invalid and will err when decoded (in distinguished
mode).

Normal ("expedient") decoding may accept other byte strings as valid
encodings, such as encodings that contain unknown fields or non-canonically
encoded values. Most of the time, this is what is desired.

#### Field types

Bilrost structs can encode fields with a wide variety of types:

| Encoder              | Value type             | Encoded representation |
|----------------------|------------------------|------------------------|
| `general` & `fixed`  | `f32`, `u32`, `i32`    | fixed-size 32 bits     |
| `general` & `fixed`  | `f64`, `u64`, `i64`    | fixed-size 64 bits     |
| `general` & `varint` | `u64`, `u32`, `u16`    | varint                 |
| `general` & `varint` | `i64`, `i32`, `i16`    | varint                 |
| `general` & `varint` | `bool`                 | varint                 |
| `general`            | derived `Enumeration`* | varint                 |
| `general`            | `String`**             | length-delimited       |
| `general`            | derived `Message`      | length-delimited       |
| `varint`             | `u8`, `i8`             | varint                 |
| `plainbytes`         | `Vec<u8>`**            | length-delimited       |

*`Enumeration` types can be directly included if they implement `Default`;
otherwise they must always be nested.

**Alternative types are available! See below.

Any of these types may be included directly in a Bilrost message struct. If that
field's value is defaulted, no bytes will be emitted when it is encoded.

In addition to including them directly, these types can also be nested within
several different containers:

<!-- TODO(widders): detail encoders and value-encoders -->

| Encoder       | Value type              | Encoded representation                                                         | Re-nestable |
|---------------|-------------------------|--------------------------------------------------------------------------------|-------------|
| any encoder   | `Option<T>`             | identical; at least some bytes are always encoded if `Some`, nothing if `None` | no          |
| `unpacked<E>` | `Vec<T>`, `BTreeSet<T>` | the same as encoder `E`, one field per value                                   | no          |
| `unpacked`    | *                       | (the same as `unpacked<general>`)                                              | no          |
| `packed<E>`   | `Vec<T>`, `BTreeSet<T>` | length-delimited, successively encoded with `E`                                | yes         |
| `packed`      | *                       | (the same as `packed<general>`)                                                | yes         |
| `map<KE, VE>` | `BTreeMap<K, V>`        | length-delimited, alternately encoded with `KE` and `VE`                       | yes         |
| `general`     | `Vec<T>`, `BTreeSet<T>` | (the same as `unpacked`)                                                       | no          |
| `general`     | `BTreeMap`              | (the same as `map<general, general>`)                                          | yes         |

Many alternative types are also available for both scalar values and containers!

| Value type | Alternative              | Supporting encoder | Feature to enable |
|------------|--------------------------|--------------------|-------------------|
| `Vec<u8>`  | `Blob`*                  | `general`          | (none)            |
| `Vec<u8>`  | `Cow<[u8]>`              | `plainbytes`       | (none)            |
| `Vec<u8>`  | `Bytes`                  | `general`          | (none)            |
| `String`   | `Cow<str>`               | `general`          | (none)            |
| `String`   | `bytestring::ByteString` | `general`          | "bytestring"      |

*`bilrost::Blob` is a transparent wrapper for `Vec<u8>` in that is a drop-in
replacement in most situations and is supported by the default `general` encoder
for maximum ease of use. If nothing but `Vec<u8>` will do, the `plainbytes`
encoder will still encode a plain `Vec<u8>` as its bytes value.

| Container type | Alternative              | Feature to enable |
|----------------|--------------------------|-------------------|
| `Vec<T>`       | `Cow<[T]>`               | (none)            |
| `BTreeMap<T>`  | `HashMap<T>`*            | "std" (default)   |
| `BTreeSet<T>`  | `HashSet<T>`*            | "std" (default)   |
| `BTreeMap<T>`  | `hashbrown::HashMap<T>`* | "hashbrown"       |
| `BTreeSet<T>`  | `hashbrown::HashSet<T>`* | "hashbrown"       |

*Hash-table-based maps and sets are implemented, but are not compatible with
distinguished encoding or decoding. If distinguished encoding is required, a
container which stores its values in sorted order must be used.

#### Compatible Widening

Widening fields along these routes is always supported in the following way:
Old message data will always decode to an equivalent/corresponding value, and
those corresponding values will of course re-encode from the new widened struct
into the same representation.

| Change                                               | Corresponding values                | Backwards compatibility breaks when...                         |
|------------------------------------------------------|-------------------------------------|----------------------------------------------------------------| 
| `bool` --> `u32` --> `u64` with `general` encoding   | `true`/`false` becomes 1/0          | value is out of range of the narrower type                     |
| `bool` --> `i32` --> `i64` with `general` encoding   | `true`/`false` becomes -1/0         | value is out of range of the narrower type                     |
| `String` --> `Vec<u8>`                               | string becomes its UTF-8 data       | value contains invalid UTF-8                                   |
| `T` --> `Option<T>`                                  | default value of `T` becomes `None` | `Some(default)` is encoded, then decoded in distinguished mode |
| `Option<T>` --> `Vec<T>` (with `unpacked` encodings) | maybe-contained value is identical  | multiple values are in the `Vec`                               |

`Vec<T>` can also be changed between `unpacked` and `packed` encoding, as long
as `T` does not have a length-delimited representation. This will break
compatibility with distinguished decoding in both directions whenever the field
is present and not default (non-optional and empty or None) because it will
also change the bytes of the encoding, but expedient decoding will still work.

#### Enumerations

`bilrost` can derive an enumeration type from an `enum` with no fields in its
variants, where each variant has either

* an explicit discriminant that is a valid `u32` value, or
* a `#[bilrost = 123]` or `#[bilrost(123)]` attribute that specifies a valid
  `u32` const expression (here with the example value `123`).

```rust
const FOUR: u32 = 4;

#[derive(Clone, PartialEq, Eq, bilrost::Enumeration)]
#[repr(u8)] // The type needn't have a u32 repr
enum Foo {
    One = 1,
    #[bilrost = 2]
    Two,
    #[bilrost(3)]
    Three,
    #[bilrost(FOUR)]
    Four,
    // When both discriminant and attribute exist, bilrost uses the attribute.
    #[bilrost(5)]
    Five = 8,
}
```

All enumeration types are encoded and decoded by conversion to and from the Rust
`u32` type, using `Into<u32>` and `TryFrom<u32, Error = bilrost::DecodeError>`.
In addition to deriving trait impls with `Enumeration`, the following additional
traits are also mandatory: `Clone` and `Eq` (and thus `PartialEq` as well).

Enumeration types are not required to implement `Default`, but they may. It is
strongly recommended, but not mandatory, that the default variant be one that
has a discriminant value of zero (`0`). If a different discriminant value is
used, it may not be possible to change an enum type in a field to a `u32` to
support decoding unknown enumeration values. This is because the default value
of each field in a bilrost struct always encodes and decodes from no data, and
changing the type to one where the default value represents a different number
would change the meaning of every encoding in which that field is default.

<!-- TODO(widders): document enumeration helpers -->

#### Oneof Fields

Oneof fields are enums with their own derive macro, which represent multiple
fields in the message, only one of which may be present in a valid message.

<!-- TODO(widders): impl and doc -->

## Using `bilrost` in a `no_std` Crate

`bilrost` is compatible with `no_std` crates. To enable `no_std` support,
disable the `std` features in `bilrost` and `bilrost-types`:

```toml
[dependencies]
bilrost = { version = "0.1001.0-dev", default-features = false, features = ["derive"] }
```

## Serializing Existing Types

`bilrost` uses a custom derive macro to handle encoding and decoding types,
which means that if your existing Rust type is compatible with `bilrost`
encoders, you can serialize and deserialize it by adding the appropriate derive
and field annotations.

### Tag Inference for Existing Types

If not otherwise specified, fields are tagged sequentially in the order they
are specified in the struct, starting with `1`.

You may skip tags which have been reserved, or where there are gaps between
sequentially occurring tag values by specifying the tag number to skip to with
the `tag` attribute on the first field after the gap. The following fields will
be tagged sequentially starting from the next number.

When defining message types for interoperation, or when fields are likely to
be added, removed, or shuffled, it may be good practice to explicitly specify
the tags of all fields in a struct instead, but this is not mandatory.

<!-- TODO(widders): fix this example -->

```
use bilrost;
use bilrost::{Enumeration, Message};

#[derive(Clone, PartialEq, Message)]
struct Person {
    #[bilrost(tag = 1)]
    pub id: String, // tag=1
    // NOTE: Old "name" field has been removed
    // pub name: String, // tag=2 (Removed)
    #[bilrost(6)]
    pub given_name: String, // tag=6
    pub family_name: String, // tag=7
    pub formatted_name: String, // tag=8
    #[bilrost(tag = "3")]
    pub age: u32, // tag=3
    pub height: u32, // tag=4
    #[bilrost(enumeration(Gender))]
    pub gender: u32, // tag=5
    // NOTE: Skip to less commonly occurring fields
    #[bilrost(tag(16))]
    pub name_prefix: String, // tag=16  (eg. mr/mrs/ms)
    pub name_suffix: String, // tag=17  (eg. jr/esq)
    pub maiden_name: String, // tag=18
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Enumeration)]
#[non_exhaustive]
pub enum Gender {
    #[default]
    Unknown = 0,
    Female = 1,
    Male = 2,
    Nonbinary = 3,
}
```

## FAQ

1. **Why another one?**

Because I can make one that does what I want.

Protobuf, for all its power and grace, is burdened with decades of legacy in
both stored data and usage in practice that [prevent it from changing][hy].
Bizarre corner case behaviors in practice that were originally implemented out
of expediency have deeply ramified themselves into the official specification of
the encoding (such as how repeated presence of nested messages in a non-repeated
field merges them together, etc.).

[hy]: https://www.hyrumslaw.com/

With a careful approach to a newer standard, we can solve many of these problems
and make a very similar encoding that is far more robust against shenanigans and
edge cases with little overhead (if fields are unordered, detecting that they
have repeated requires overhead, but if they *must* be ordered it is trivial).
Along with this, with only a little more work, we also achieve inherent
canonicalization for our distinguished message types. Accomplishing the same
thing in Protobuf is an onerous task, and one I have almost never seen correctly
described in the wild. Quite a few people have, as the saying goes, tried and
died.

tl;dr: I had the conceit that I could make the protobuf encoding better. For my
personal purposes, this is true. Perhaps the same will be even true for you as
well.

2. **Could the bilrost encoding be implemented as a serializer for
   [Serde][se]?**

Probably not, though `serde` experts are free to weigh in. There are multiple
complications with trying to serialize bilrost messages with Serde:

- Bilrost fields bear a numbered tag, and currently there appears to be no
  mechanism suitable for this in `serde`.
- Bilrost fields are also associated with a specific encoder, such as `general`
  or `fixed`, which may alter their encoding. Purely trait-based dispatch will
  work poorly for this, especially when the values become nested within other
  data structures like maps and `Vec` and encoders may begin to look
  like `map<general, packed<fixed>>`.
- Bilrost messages must encode their fields in tag order, which may (in the case
  of `oneof` fields) vary depending on their value, and it's not clear how or if
  this could be solved in `serde`.
- Bilrost has both expedient and distinguished encoding modes, and promises that
  encoding a message that implements `DistinguishedMessage` always produces
  canonical output. This may be beyond what is practical to implement.

Despite all this, it is possible to place `serde` derive tags onto the generated
types, so the same structure can support both `bilrost` and `Serde`.

[se]: https://serde.rs/

## Why *Bilrost?*

Protocol Buffers, originating at Google, took on the portmanteau "protobuf". In
turn, Protobuf for Rust became "prost".

To fork that library, one might call it... "Frost"? But that name is taken.
"Bifrost" is a nice name, and a sort of pun on "frost, 2"; but that is also
taken. "Bilrost" is another name for the original Norse "Bifrost", and it is
quite nice, so here we are.

## License

`bilrost` is distributed under the terms of the Apache License (Version 2.0).

See [LICENSE](./LICENSE) & [NOTICE](./NOTICE) for details.

Copyright 2023-2024 Kent Ross
Copyright 2022 Dan Burkert & Tokio Contributors
