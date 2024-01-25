# *BILROST!*

<!-- TODO(widders): normalize capitalization of "bilrost" -->

`bilrost` is a
[Protocol Buffers](https://developers.google.com/protocol-buffers/)-alike
encoding for the [Rust Language](https://www.rust-lang.org/). It is a direct
fork of
[`prost`](https://github.com/tokio-rs/prost). Like `prost`, bilrost can
enable writing simple, idiomatic Rust code with `derive` macros that serializes
and deserializes structs as portable and durable binary data, using an encoding
scheme similar to that of protocol buffers but slightly different and mutually
incompatible. Bilrost (as a specification) strives to provide a superset of the
capabilities of protocol buffers while reducing the surface area for mistakes
and surprises; `bilrost` (the Rust Language implementation library) strives to
provide access to all of those capabilities with maximum convenience.

TODO: fill out this outline for a better introduction

* the overall concepts
    * tagged fields
    * forwards and backwards compatibility as message types are extended
    * distinguished encoding
    * some semantics depend upon the types themselves, like defaults and
      maybe ordering
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
* using the library
    * how to derive
        * field annotations
    * types that work with `bilrost`
    * encoders
    * custom encoders
* the (current lack of) ecosystem compared to protobuf
    * no reflection yet
    * no DSL for specifying schemas yet
    * no support across other languages yet
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

* All varints (including tag fields and lengths) use
  [bijective numeration](https://en.wikipedia.org/wiki/Bijective_numeration),
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

| Encoder             | Value type             | Encoded representation |
|---------------------|------------------------|------------------------|
| `general` & `fixed` | `f32`                  | fixed-size 32 bits     |
| `general` & `fixed` | `f64`                  | fixed-size 64 bits     |
| `general`           | `u32`                  | varint                 |
| `fixed`             | `u32`                  | fixed-size 32 bits     |
| `general`           | `u64`                  | varint                 |
| `fixed`             | `u64`                  | fixed-size 64 bits     |
| `general`           | `i32`                  | varint                 |
| `fixed`             | `i32`                  | fixed-size 32 bits     |
| `general`           | `i64`                  | varint                 |
| `fixed`             | `i64`                  | fixed-size 64 bits     |
| `general`           | `bool`                 | varint                 |
| `general`           | derived `Enumeration`* | varint                 |
| `general`           | `String`               | length-delimited       |
| `general`           | `Blob`**, `Bytes`      | length-delimited       |
| `vecblob`           | `Vec<u8>`              | length-delimited       |
| `general`           | derived `Message`      | length-delimited       |

*`Enumeration` types can be directly included if they implement `Default`;
otherwise they must always be nested.

**`Blob` is a transparent wrapper for `Vec<u8>` that is a drop-in replacement
in most situations. If nothing but `Vec<u8>` will do, the `vecblob` encoder
will encode a plain `Vec<u8>` as a bytes value.

Any of these types may be included directly in a Bilrost message struct. If that
field's value is defaulted, no bytes will be emitted when it is encoded.

In addition to including them directly, these types can also be nested within
several different containers:

<!-- TODO(widders): detail encoders and value-encoders -->

| Encoder       | Value type                            | Encoded representation                                                         | Re-nestable |
|---------------|---------------------------------------|--------------------------------------------------------------------------------|-------------|
| any encoder   | `Option<T>`                           | identical; at least some bytes are always encoded if `Some`, nothing if `None` | no          |
| `unpacked<E>` | `Vec<T>`, `HashSet<T>`, `BTreeSet<T>` | the same as encoder `E`, one field per value                                   | no          |
| `unpacked`    | *                                     | (the same as `unpacked<general>`)                                              | no          |
| `packed<E>`   | `Vec<T>`, `HashSet<T>`, `BTreeSet<T>` | length-delimited, successively encoded with `E`                                | yes         |
| `packed`      | *                                     | (the same as `packed<general>`)                                                | yes         |
| `map<KE, VE>` | `BTreeMap<K, V>`, `HashMap<K, V>`     | length-delimited, alternately encoded with `KE` and `VE`                       | yes         |
| `general`     | `Vec`, `HashSet`, `BTreeSet`          | (the same as `unpacked`)                                                       | no          |
| `general`     | `BTreeMap`, `HashMap`                 | (the same as `map<general, general>`)                                          | yes         |

#### Compatible Widening

Widening fields along these routes is always supported in the following way:
Old message data will always decode to an equivalent/corresponding value, and
those corresponding values will of course re-encode from the new widened struct
into the same representation.

| Change                                               | Corresponding values                | Backwards compatibility breaks when...                         |
|------------------------------------------------------|-------------------------------------|----------------------------------------------------------------| 
| `bool` --> `u32` --> `u64` with `general` encoding   | `true`/`false` becomes 1/0          | value is out of range of the narrower type                     |
| `bool` --> `i32` --> `i64` with `general` encoding   | `true`/`false` becomes -1/0         | value is out of range of the narrower type                     |
| `String` --> `Blob`/`Bytes`                          | string becomes its UTF-8 data       | value contains invalid UTF-8                                   |
| `T` --> `Option<T>`                                  | default value of `T` becomes `None` | `Some(default)` is encoded, then decoded in distinguished mode |
| `Option<T>` --> `Vec<T>` (with `unpacked` encodings) | maybe-contained value is identical  | multiple values are in the `Vec`                               |

`Vec<T>` can also be changed between `unpacked` and `packed` encoding, as long
as `T` does not have a length-delimited representation. This will break
compatibility with distinguished decoding in both directions whenever the field
is present and not default (non-optional and empty or None) because it will
also change the bytes of the encoding, but expedient decoding will still work.

#### Enumerations

All enumeration types convert to and from the Rust `u32` type.

<!-- TODO(widders): document what else there is about enums and their deriver -->

#### Oneof Fields

Oneof fields are enums with their own derive macro, which represent multiple
fields in the message, only one of which may be present in a valid message.

<!-- TODO(widders): impl and doc -->

## Using `bilrost` in a `no_std` Crate

`bilrost` is compatible with `no_std` crates. To enable `no_std` support,
disable the `std` features in `bilrost` and `bilrost-types`:

```ignore
[dependencies]
bilrost = { version = "0.12", default-features = false, features = ["derive"] }
```

## Serializing Existing Types

`bilrost` uses a custom derive macro to handle encoding and decoding types,
which means that if your existing Rust type is compatible with `bilrost`
encoders, you can serialize and deserialize it by adding the appropriate derive
and field annotations.

### Tag Inference for Existing Types

Bilrost automatically infers tags for the struct.

Fields are tagged sequentially in the order they
are specified, starting with `1`.

You may skip tags which have been reserved, or where there are gaps between
sequentially occurring tag values by specifying the tag number to skip to with
the `tag` attribute on the first field after the gap. The following fields will
be tagged sequentially starting from the next number.

<!-- TODO(widders): fix this example -->

```rust,ignore
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
    // TODO(widders): implement u32 enumeration helpers
    pub gender: u32, // tag=5
    // NOTE: Skip to less commonly occurring fields
    #[bilrost(tag(16))]
    pub name_prefix: String, // tag=16  (eg. mr/mrs/ms)
    pub name_suffix: String, // tag=17  (eg. jr/esq)
    pub maiden_name: String, // tag=18
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Enumeration)]
pub enum Gender {
    Unknown = 0,
    Female = 1,
    Male = 2,
    Nonbinary = 3,
}
```

## FAQ

1. **Could `bilrost` be implemented as a serializer for
   [Serde](https://serde.rs/)?**

Probably not, however I would like to hear from a Serde expert on the matter.
There are two complications with trying to serialize Protobuf messages with
Serde:

- Bilrost fields bear a numbered tag, and currently there appears to be no
  mechanism suitable for this in `serde`.
- The mapping from `bilrost` message field's type to its Rust encoding is not
  1-to-1. As a result, trait-based approaches to dispatching don't work very
  well. Example: four different Protobuf field encoders can handle a
  Rust `Vec<i32>`, each producing a different encoded
  representation: `general`, `fixed`, `packed<general>`, and `packed<fixed>`.

  But it is possible to place `serde` derive tags onto the generated types, so
  the same structure can support both `bilrost` and `Serde`.

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

Copyright 2022 Dan Burkert & Tokio Contributors
Copyright 2023-2024 Kent Ross
