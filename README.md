[![continuous integration](https://github.com/mumbleskates/bilrost/actions/workflows/ci.yml/badge.svg)](https://github.com/mumbleskates/bilrost/actions/workflows/ci.yml)
[![Documentation](https://docs.rs/bilrost/badge.svg)](https://docs.rs/bilrost/)
[![Crate](https://img.shields.io/crates/v/bilrost.svg)](https://crates.io/crates/bilrost)
[![Dependency Status](https://deps.rs/repo/github/mumbleskates/bilrost/status.svg)](https://deps.rs/repo/github/mumbleskates/bilrost)

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
and shares many of its traits, but is incompatible. It is in some ways simpler
and less rigid in its specification, and is designed to improve on some of
protobuf's deficiencies. In doing so it breaks wire-compatibility with protobuf.

`bilrost` is implemented for the [Rust Language][rs]. It is a direct fork of
[`prost`][p], and shares many of its performance characteristics. (It is not the
fastest possible encoding library, but it is still pretty fast and comes with
unique advantages.) Like `prost`, `bilrost` can enable writing simple, idiomatic
Rust code with `derive` macros that serialize and deserialize structs from
binary data. Unlike `prost`, `bilrost` is free from most of the constraints of
the protobuf ecosystem and required semantics of protobuf message types.
Bilrost (the specification) and this library allow much wider compatibility with
existing struct types and their normal semantics. Rather than relying on
producing generated code from a protobuf `.proto` schema definition, `bilrost`
is designed to be easily used by hand.

🌈

[pb]: https://developers.google.com/protocol-buffers/

[rs]: https://www.rust-lang.org/

[p]: https://github.com/tokio-rs/prost

## Contents

TODO: reorder the whole document to reconcile with the TOC

- [Quick start](#getting-started)
    - [Using the derive macros](#example-derive-macro-applications)
    - [`no_std` support](#no_std-support)
    - [Changelog](./CHANGELOG.md) ([on github][ghchangelog])
- [Differences from `prost`](#bilrost-vs-prost)
- [Differences from Protobuf](#differences-from-protobuf)
    - [Distinguished representation of data](#distinguished-decoding) and [how
      this is achieved](#distinguished-representation-on-the-wire-in-bilrost)
- [Compared to other encodings, distinguished and not](
  #comparisons-to-other-encodings)
- [Why use Bilrost?](#strengths-aims-and-advantages)
- [Why *not* use Bilrost?](#what-bilrost-and-the-library-wont-do)
- [How does it work?](#conceptual-overview)
    - [How *exactly* does it work?](#encoding-specification)
- [License & copyright](#license)

[ghchangelog]: https://github.com/mumbleskates/bilrost/blob/bilrost/CHANGELOG.md

## Conceptual overview

Bilrost is an encoding scheme for converting in-memory data structs into plain
byte strings and vice versa. It's generally suitable for both network transport
and data retained over the long-term. Its encoded data is not human-readable,
but it is encoded quite simply. It supports integral and floating point numbers,
strings and byte strings, nested messages, and recursively nested messages. All
of the above are supported as optional values, repeated values, sets of unique
values, and key/value mappings where sensible. With appropriate choices of
encoders (which determine the representation), most of these constructs can be
nested almost arbitrarily.

Encoded Bilrost data does not include the names of its fields; they are instead
assigned numbers agreed upon in advance by the message schema that specifies it.
This can make the data much more compact than "schemaless" encodings like JSON,
CBOR, etc., without sacrificing its extensibility: new fields can be added, and
old fields removed, without necessarily breaking backwards compatibility with
older versions of the encoding program. In the typical "expedient" decoding
mode, any field not in the message schema is ignored when decoding, so if fields
are added or removed over time the fields that remain in common will still be
mutually intelligible between the two versions of the schema. In this way,
Bilrost is very similar to [Protobuf][pb]. See also:
[Design philosophy](#design-philosophy), [Comparisons to other encodings](
#comparisons-to-other-encodings).

Bilrost also has the ability to encode and decode data that is guaranteed to be
canonically represented; see the [next section](#distinguished-decoding).

Bilrost specifies most of what is required to make these message schemas
portable not just across architectures and programs, but to other programming
languages as well. There is currently one minor caveat: The *sort order* of
values in Bilrost may matter.

In distinguished encoding mode, canonical data must always be represented with
*sets* and *maps* having their items in sorted order. When the item type of a
set (or the key type of a map) is not a simple type with an already-standardized
sorting order (such as an integer or string), the canonical order of the items
depends on that type's implementation, and care must be taken to standardize
that order in addition to the schema of the message's fields when defining
distinguished types.

### Distinguished decoding

It is possible in `bilrost` to derive an extended trait, `DistinguishedMessage`,
which provides a distinguished decoding mode. Decoding in distinguished mode
comes with an additional guarantee that the resulting message value will
re-encode to the exact same sequence of bytes, and that *every* different
sequence of bytes will either decode to a different value or fail to decode. Any
message type that *can* implement distinguished encoding *will* always encode in
its distinguished form; there is not an alternate encoding mode that is "more
canonical".

Formally, when a message type implements `DistinguishedMessage`, values of
the message type are *bijective* to a subset of all byte strings. Each different
possible byte string decodes in distinguished mode to a message value that is
distinct from the message values decoded from every other such byte string, or
will produce an error when decoded in this mode. If a message is successfully
decoded from a byte string in distinguished mode, is not modified, and is then
re-encoded, it will emit the exact same byte string.

The best proxy of this expectation of an [equivalence relation][equiv] in Rust
is the [`Eq`][eq] trait, which denotes that there is an equivalence relation
between all values of any type that implements it. Therefore, this trait is
required of all field and message types in order to implement distinguished
encoding in `bilrost`.

[equiv]: https://en.wikipedia.org/wiki/Equivalence_relation

[eq]: https://doc.rust-lang.org/std/cmp/trait.Eq.html

Normal ("expedient") decoding may accept other byte strings as valid
encodings of a given value, such as encodings that contain unknown fields or
non-canonically encoded values*. Most of the time, this is what is desired.

*"Non-canonical" value encodings in Bilrost principally include fields that are
represented in the encoding even though their value is considered empty. For
message types, such as nested messages, it also encompasses the message
representation containing fields with unknown tags.

To support this "exactly 1:1" expectation for distinguished messages, certain
types are forbidden and not implemented in disinguished mode, even though they
theoretically could be. This primarily includes floating point numbers, which
have incompatible equality semantics. In the Bilrost encoding, floating point
numbers are represented in their standard [IEEE 754][ieee754] binary format
standard to most computers today. This comes with particular rules for equality
semantics that are generally uniform across all languages, and which don't form
an equivalence relation. "NaN" values are never equal to each other or to
themselves.

[ieee754]: https://en.wikipedia.org/wiki/IEEE_754

#### Floating point values and distinguished encoding

Equivalence relations are also not quite sufficient to describe the desired
properties of a distinguished type in Bilrost, either; not only must the values
*themselves* be considered equivalent, they must also *encode* to the same
bytes. When encoding and decoding floating point values, `bilrost` takes care to
preserve even the distinction between +0.0 and -0.0, which are considered to be
equal to each other in IEEE 754; this [has been a problem][protonegzero] for
other encodings in the past. Even if it is not always necessary, when a value is
encoded in `bilrost`, decoding that value again is guaranteed to produce the
same value with the exact same bits.

[protonegzero]: https://github.com/protocolbuffers/protobuf/issues/7062

For this reason it is not yet considered a good idea to implement distinguished
encoding for third-party wrappers for Rust's floating point types that implement
[`Eq`][eq] and [`Ord`][ord] such as [`ordered_float`][ordered_float] and
[`decorum`][decorum] because they still consider some sets of values that have
*different bits* to be equal. Any future implementation of such a type would
have to take special care to unify the encoded representation of any equivalence
classes in these types *and standardize this in a portable way*, which also
de facto induces some data loss when round tripping. It is not guaranteed this
will ever be considered worthwhile or implemented.

[ord]: https://doc.rust-lang.org/std/cmp/trait.Ord.html

[ordered_float]: https://docs.rs/ordered-float/latest/ordered_float/

[decorum]: https://docs.rs/decorum/latest/decorum/

**If it is desirable to have a distinguished encoding for the bit-wise
representations of a floating point value**, it should first be cast to its bits
as an unsigned integer and encoded that way. This reduces the surface area for
mistakes, and makes it clearer that floating point numbers need special handling
in code that cares very much about distinguished representations.

## Design philosophy

* the philosophy
    * data has meaning based on where you find it
    * encodings with explicit schemas can be easier to guess the meaning of
      if you don't already know, but most of the time this is wasted bytes
    * therefore it is often very sensible to encode data in a way that is
      legible to you, with implicit schemas and room for extending
    * the data you get back when decoding should not transform any of the field
      values except by omitting extensions (no int coercion)
    * as an encoding, Bilrost works to make invalid states unrepresentable
      when practical where it doesn't greatly increase complexity
    * Bilrost is designed to aid, but not require, distinguished encoding

## Using the library

### Getting started

To use `bilrost`, first add it as a dependency in `Cargo.toml`, either with
`cargo add bilrost` or manually:

```toml
bilrost = "0.1001.0-dev"
```

Then, derive `bilrost::Message` for your struct type:

```rust,
use bilrost::Message;

#[derive(Debug, PartialEq, Message)]
struct BucketFile {
    name: String,
    shared: bool,
    storage_key: String,
}

let foo_file = BucketFile {
    name: "foo.txt".to_string(),
    shared: true,
    storage_key: "public/foo.txt".to_string(),
};

// Encoding data is simple.
let encoded = foo_file.encode_to_vec();
// The encoded data is compact, but not very human-readable.
assert_eq!(encoded, b"\x05\x07foo.txt\x04\x01\x05\x0epublic/foo.txt");

// Decoding data is likewise simple!
let decoded = BucketFile::decode(encoded.as_slice()).unwrap();
assert_eq!(foo_file, decoded);
```

Later, more fields can be added to that same struct and it will still decode the
same data.

```rust,
# use bilrost::Message;
#[derive(Debug, Default, PartialEq, Message)]
struct BucketFile {
    #[bilrost(1)]
    name: String,
    #[bilrost(5)]
    mime_type: Option<String>,
    #[bilrost(6)]
    size: Option<u64>,
    #[bilrost(2)]
    shared: bool,
    #[bilrost(3)]
    storage_key: String,
    #[bilrost(4)]
    bucket_name: String,
}

let new_file = BucketFile::decode(
    &b"\x05\x07foo.txt\x04\x01\x05\x0epublic/foo.txt"[..],
)
.unwrap();
assert_eq!(
    new_file,
    BucketFile {
        name: "foo.txt".to_string(),
        shared: true,
        storage_key: "public/foo.txt".to_string(),
        ..Default::default()
    }
);
```

#### Crate features

The `bilrost` crate has several optional features:

* "std" (default): provides support for `HashMap` and `HashSet`.
* "derive" (default): includes the `bilrost-derive` crate and re-exports its
  derive macros. It's unlikely this should ever be disabled if you are
  using `bilrost` normally.
* "detailed-errors" (default): the decode error type returned by messages will
  have more information on the path to the exact field in the decoded data that
  encountered an error. With this disabled errors are more opaque, but may be
  smaller and faster.
* "no-recursion-limit": removes the recursion limit designed to keep data from
  nesting too deeply.
* "extended-diagnostics": with a small added dependency, attempts to provide
  better compile-time diagnostics when derives and derived implementations don't
  work. Somewhat experimental.
* "opaque": enables `bilrost::encoding::opaque::{OpaqueMessage, OpaqueValue}`
  which can decode, represent, and reencode *any* potentially valid `bilrost`
  data.
* "bytestring": provides first-party support for `bytestring::Bytestring`
* "hashbrown": provides first-party support for `hashbrown::{HashMap, HashSet}`
* "smallvec": provides first-party support for `smallvec::SmallVec`
* "thin-vec": provides first-party support for `thin-vec::ThinVec`
* "tinyvec": provides first-party support for `tinyvec::TinyVec`

#### `no_std` support

With the "std" feature disabled, `bilrost` has full `no_std` support.
`no_std`-compatible hash-maps are still available if desired by enabling the
"hashbrown" feature.

To enable `no_std` support, disable the `std` features in `bilrost` (and
`bilrost-types`, if it is used):

```toml
[dependencies]
bilrost = { version = "0.1001.0-dev", default-features = false, features = ["derive"] }
```

### Derive macros

You can then import and use its traits and derive macros. The main three are:

* `Message`: This is the basic working unit. Derive this for structs to enable
  encoding and decoding them to and from binary data.
* `Enumeration`: This is a derive only, not a trait, which implements support
  for encoding an enum type with `bilrost`. The enum must have no fields, and
  each of its variants will correspond to a different `u32` value that will
  represent it in the encoding.
* `Oneof`: This is a trait and derive macro for enumerations representing
  mutually exclusive fields within a message struct. Each variant must have one
  field, and each variant must have a unique field tag assigned to it, *both*
  within the oneof and within the message of which it is a part. Types
  with `Oneof` derived do not have `bilrost` APIs useful to library users except
  when they are included in a `Message` struct.

#### Example derive macro applications

If not otherwise specified, fields are tagged sequentially in the order they
are specified in the struct, starting with `1`.

You may skip tags which have been reserved, or where there are gaps between
sequentially occurring tag values by specifying the tag number to skip to with
the `tag` attribute on the first field after the gap. The following fields will
be tagged sequentially starting from the next number.

When defining message types for interoperation, or when fields are likely to
be added, removed, or shuffled, it may be good practice to explicitly specify
the tags of all fields in a struct instead, but this is not mandatory.

TODO: clean up this example

```rust,
use bilrost::{Enumeration, Message};

#[derive(Clone, PartialEq, Message)]
struct Person {
    #[bilrost(tag = 1)]
    pub id: String, // tag=1
    // NOTE: Old "name" field has been removed
    // pub name: String, // tag=2 (Removed)
    #[bilrost(6)]
    pub given_name: String, // tag=6
    pub family_name: String,    // tag=7
    pub formatted_name: String, // tag=8
    #[bilrost(tag = "3")]
    pub age: u32, // tag=3
    pub height: u32,            // tag=4
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

#### Oneof Fields

Oneof fields are enums with their own derive macro, which represent multiple
fields in the message, only one of which may be present in a valid message.

TODO: expand

* example usage
* with & without empty variant

#### Enumerations

Implementations for numeric enumerations without fields can be derived for
inclusion in `bilrost` structs:

```rust,
# use bilrost::Enumeration;
#[derive(Clone, PartialEq, Eq, Enumeration)]
enum Foo {
    A = 1,
    B = 2,
}
```

This implements several traits, including `Into<u32>`, `TryFrom<u32>`, and
expedient & distinguished encoding support via the `general` encoder. At minimum
the type must also support `Clone` and `Eq`.

Enumerations' numeric discriminant values in Bilrost can be overridden or wholly
specified by attributes:

```rust,
# use bilrost::Enumeration;
#[derive(Clone, PartialEq, Eq, Enumeration)]
enum Foo {
    #[bilrost(2)]
    A = 1,
    #[bilrost(1)]
    B = 2,
    #[bilrost(3)]
    C,
}
```

If the discriminants conflict at all, compilation will fail; the discriminants
must be unique within that enumeration.

```rust,compile_fail
# use bilrost::Enumeration;
#[derive(Clone, PartialEq, Eq, Enumeration)]
enum Foo {
    A = 1,
    #[bilrost(1)] // error: unreachable pattern
    B = 2,
}
```

The discriminants can be specified in any of several ways:

* As a plain discriminant, `A = 1`
* `#[bilrost(1)]`
* As another valid discriminant, `A = CONSTANT_U32_ONE`
* `#[bilrost(CONSTANT_U32_ONE)]`

For an enumeration to qualify for direct inclusion as a message field rather
than only as a nested value (within `Option`, `Vec`, etc.), one of the
discriminants must be spelled exactly "0".

TODO: enumeration helpers

#### Encoders and other attributes

TODO: expand

* different encoders and what they do
* "enumeration" helpers
* "recurses"

### Distinguished derive macros

There are two derivable companion traits, `DistinguishedMessage`
and `DistinguishedOneof`, that implement the extended traits for distinguished
decoding when possible. Both messages and oneofs must contain only fields that
support distinguished decoding in order to support it themselves. Distinguished
encoding requires `Eq` and its semantics of each field, oneof, and message type.

### Encoding and decoding messages

TODO: this

#### Using `dyn` with object-safe message traits

TODO: this

### Supported message field types

`bilrost` structs can encode fields with a wide variety of types:

| Encoder              | Value type             | Encoded representation | Distinguished |
|----------------------|------------------------|------------------------|---------------|
| `general` & `fixed`  | `f32`                  | fixed-size 32 bits     | no            |
| `general` & `fixed`  | `u32`, `i32`           | fixed-size 32 bits     | yes           |
| `general` & `fixed`  | `f64`                  | fixed-size 64 bits     | no            |
| `general` & `fixed`  | `u64`, `i64`           | fixed-size 64 bits     | yes           |
| `general` & `varint` | `u64`, `u32`, `u16`    | varint                 | yes           |
| `general` & `varint` | `i64`, `i32`, `i16`    | varint                 | yes           |
| `general` & `varint` | `bool`                 | varint                 | yes           |
| `general`            | derived `Enumeration`* | varint                 | yes           |
| `general`            | [`String`][str]**      | length-delimited       | yes           |
| `general`            | impl `Message`***      | length-delimited       | maybe         |
| `varint`             | `u8`, `i8`             | varint                 | yes           |
| `plainbytes`         | [`Vec<u8>`][vec]**     | length-delimited       | yes           |

*`Enumeration` types can be directly included if they have a value that has a
Bilrost representation of zero (represented as exactly the expression `0` either
via a `#[bilrost(0)]` attribute or, absent an attribute, via a normal
discriminant value). Otherwise, enumeration types must always be nested.

**Alternative types are available! See below.

***`Message` types inside `Box` still impl `Message`, with a covering impl;
message types can nest recursively this way.

Any of these types may be included directly in a `bilrost` message struct. If
that field's value is empty, no bytes will be emitted when it is encoded.

In addition to including them directly, these types can also be nested within
several different containers:

| Encoder       | Value type                              | Encoded representation                                                         | Re-nestable | Distinguished      |
|---------------|-----------------------------------------|--------------------------------------------------------------------------------|-------------|--------------------|
| any encoder   | [`Option<T>`][opt]                      | identical; at least some bytes are always encoded if `Some`, nothing if `None` | no          | when `T` is        |
| `unpacked<E>` | [`Vec<T>`][vec], [`BTreeSet<T>`][btset] | the same as encoder `E`, one field per value                                   | no          | when `T` is        |
| `unpacked`    | *                                       | (the same as `unpacked<general>`)                                              | no          | *                  |
| `packed<E>`   | `Vec<T>`, `BTreeSet<T>`                 | always length-delimited, successively encoded with `E`                         | yes         | when `T` is        |
| `packed`      | *                                       | (the same as `packed<general>`)                                                | yes         | *                  |
| `map<KE, VE>` | [`BTreeMap<K, V>`][btmap]               | always length-delimited, alternately encoded with `KE` and `VE`                | yes         | when `K` & `V` are |
| `general`     | `Vec<T>`, `BTreeSet<T>`                 | (the same as `unpacked`)                                                       | no          | *                  |
| `general`     | `BTreeMap`                              | (the same as `map<general, general>`)                                          | yes         | *                  |

Many alternative types are also available for both scalar values and containers!

| Value type   | Alternative                       | Supporting encoder | Distinguished | Feature to enable |
|--------------|-----------------------------------|--------------------|---------------|-------------------|
| `Vec<u8>`    | `Blob`***                         | `general`          | yes           | (none)            |
| `Vec<u8>`    | [`Cow<[u8]>`][cow]                | `plainbytes`       | yes           | (none)            |
| `Vec<u8>`    | [`bytes::Bytes`][bytes]*          | `general`          | yes           | (none)            |
| `Vec<u8>`    | [`[u8; N]`][arr]**                | `plainbytes`       | yes           | (none)            |
| `u32`, `u64` | `[u8; 4]`, `[u8; 8]`              | `fixed`            | yes           | (none)            |
| `String`     | [`Cow<str>`][cow]                 | `general`          | yes           | (none)            |
| `String`     | [`bytestring::ByteString`][bstr]* | `general`          | yes           | "bytestring"      |

*When decoding from a `bytes::Bytes` object, both `bytes::Bytes` and
`bytes::ByteString` have a zero-copy optimization and will reference the decoded
buffer rather than copying. (This would also work for any other input type that
has a zero-copy `bytes::Buf::copy_to_bytes()` optimization.)

**Plain byte arrays, as you might expect, only accept one exact length of data;
other lengths are considered invalid values.

***`bilrost::Blob` is a transparent wrapper for `Vec<u8>` in that is a drop-in
replacement in most situations and is supported by the default `general` encoder
for maximum ease of use. If nothing but `Vec<u8>` will do, the `plainbytes`
encoder will still encode a plain `Vec<u8>` as its bytes value.

| Container type | Alternative                           | Distinguished | Feature to enable |
|----------------|---------------------------------------|---------------|-------------------|
| `Vec<T>`       | [`Cow<[T]>`][cow]                     | when `T` is   | (none)            |
| `Vec<T>`       | [`smallvec::SmallVec<[T]>`][smallvec] | when `T` is   | "smallvec"        |
| `Vec<T>`       | [`thin_vec::ThinVec<[T]>`][thinvec]   | when `T` is   | "thin_vec"        |
| `Vec<T>`       | [`tinyvec::TinyVec<[T]>`][tinyvec]    | when `T` is   | "tinyvec"         |
| `BTreeMap<T>`  | [`HashMap<T>`][hashmap]*              | no            | "std" (default)   |
| `BTreeSet<T>`  | [`HashSet<T>`][hashset]*              | no            | "std" (default)   |
| `BTreeMap<T>`  | [`hashbrown::HashMap<T>`][hbmap]*     | no            | "hashbrown"       |
| `BTreeSet<T>`  | [`hashbrown::HashSet<T>`][hbset]*     | no            | "hashbrown"       |

[str]: https://doc.rust-lang.org/std/string/struct.String.html

[vec]: https://doc.rust-lang.org/std/vec/struct.Vec.html

[opt]: https://doc.rust-lang.org/std/option/enum.Option.html

[btset]: https://doc.rust-lang.org/std/collections/struct.BTreeSet.html

[btmap]: https://doc.rust-lang.org/std/collections/btree_map/struct.BTreeMap.html

[cow]: https://doc.rust-lang.org/std/borrow/enum.Cow.html

[bytes]: https://docs.rs/bytes/latest/bytes/struct.Bytes.html

[arr]: https://doc.rust-lang.org/std/primitive.array.html

[bstr]: https://docs.rs/bytestring/latest/bytestring/struct.ByteString.html

[smallvec]: https://docs.rs/smallvec/latest/smallvec/struct.SmallVec.html

[thinvec]: https://docs.rs/thin-vec/latest/thin_vec/struct.ThinVec.html

[tinyvec]: https://docs.rs/tinyvec/latest/tinyvec/enum.TinyVec.html

[hashmap]: https://doc.rust-lang.org/std/collections/struct.HashMap.html

[hashset]: https://doc.rust-lang.org/std/collections/struct.HashSet.html

[hbmap]: https://docs.rs/hashbrown/latest/hashbrown/struct.HashMap.html

[hbset]: https://docs.rs/hashbrown/latest/hashbrown/struct.HashSet.html

*Hash-table-based maps and sets are implemented, but are not compatible with
distinguished encoding or decoding. If distinguished encoding is required, a
container which stores its values in sorted order must be used.

While it's possible to nest and recursively nest `Message` types with `Box`,
`Vec`, etc., `bilrost` does not do any kind of runtime check to avoid infinite
recursion in the event of a cycle. The chosen supported types and containers
should not be able to become *infinite* as implemented, but if the situation
were induced to happen anyway it would not end well. (Note that creative usage
of `Cow<[T]>` can create messages that encode absurdly large, but the borrow
checker keeps them from becoming infinite mathematically if not practically.)

#### Enumerations

`bilrost` can derive the required implementations for a numeric enumeration type
from an `enum` with no fields in its variants, where each variant has either

1. an explicit discriminant that is a valid `u32` value, or
2. a `#[bilrost = 123]` or `#[bilrost(123)]` attribute that specifies a valid
   `u32` const expression (here with the example value `123`).

```rust
#[derive(Clone, PartialEq, Eq, bilrost::Enumeration)]
enum SimpleEnum {
    Unknown = 0,
    A = 1,
    B = 2,
    C = 3,
}

const FOUR: u32 = 4;

#[derive(Clone, PartialEq, Eq, bilrost::Enumeration)]
#[repr(u8)] // The type needn't have a u32 repr
enum ComplexEnum {
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

#### Compatible Widening

While many types have different representations and interpretations in the
encoding, there are several classes of types which have the same encoding *and*
the same interpretation as long as the values are in range for both types. For
example, it's possible to change an `i16` field and change its type to `i32`,
and any number that can be represented in `i16` will have the same encoded
representation for both types.

Widening fields along these routes is always supported in the following way:
Old message data will always decode to an equivalent/corresponding value, and
those corresponding values will re-encode from the new widened struct into the
same representation.

| Change                                                                                | Corresponding values                | Backwards compatibility breaks when...                         |
|---------------------------------------------------------------------------------------|-------------------------------------|----------------------------------------------------------------|
| `bool` --> `u8` --> `u16` --> `u32` --> `u64`, all with `general` or `varint` encoder | `true`/`false` becomes 1/0          | value is out of range of the narrower type                     |
| `bool` --> `i8` --> `i16` --> `i32` --> `i64`, all with `general` or `varint` encoder | `true`/`false` becomes -1/0         | value is out of range of the narrower type                     |
| `String` --> `Vec<u8>`                                                                | string becomes its UTF-8 data       | value contains invalid UTF-8                                   |
| `[u8; N]` with `general` encoder --> `Vec<u8>`                                        | no change                           | data is a different length than the array                      |
| `T` --> `Option<T>`                                                                   | default value of `T` becomes `None` | `Some(default)` is encoded, then decoded in distinguished mode |
| `Option<T>` --> `Vec<T>` (with `unpacked` encoding)                                   | maybe-contained value is identical  | multiple values are in the `Vec`                               |

`Vec<T>` and other list- and set-like collections that contain repeated values
can also be changed between `unpacked` and `packed` encoding, as long as the
inner value type `T` does not have a length-delimited representation. This will
break compatibility with distinguished decoding in both directions whenever the
field is present and not empty (non-optional and empty or None) because it will
also change the encoded representation, but expedient decoding will still work.

## What Bilrost and the library won't do

Bilrost does *not* have a robust reflection ecosystem. It does not (yet) have an
intermediate schema language like protobuf does, nor implementations for very
many languages, nor RPC framework support, nor an independent validation
framework. These things are possible, they just don't exist yet.

This library also does not have support for encoding/decoding its message types
to and from JSON or other readable text formats. However, because it supports
deriving Bilrost encoding implementations from existing structs, it is possible
(and recommended) to use other, preexisting tools to do this. `Debug` can also
be derived for a `bilrost` message type, as can other encodings that similarly
support deriving implementations from preexisting types.

## Encoding specification

Philosophically, there are two "sides" to the encoding scheme: the opaque data
that comprises it, and conventions for how that data is interpreted.

### Opaque format

Values in bilrost are encoded opaquely as strings of bytes or as non-negative
integers not greater than the maximum value representable in an unsigned 64 bit
integer (2^64-1). The only four scalar types supported by the encoding format
itself are these integers, byte strings of any (64-bit representable) length,
and byte strings with lengths of exactly 4 or exactly 8.

#### Messages

The basic functional unit of encoded Bilrost data is a message. An encoded
message is some string of zero or more bytes with a specific length.

#### Fields

Encoded messages are comprised of zero or more encoded fields.

Each field is encoded as two parts: first its key, and then its value. The
field's key is always encoded as a varint. The interpretation of the encoded
value of that varint is in two parts: the value divided by 4 is the *tag-delta*,
and the remainder of that division determines the value's *wire-type*. The
tag-delta encodes the non-negative difference between the tag of the
previously-encoded field (or zero, if it is the first field) and the field the
key is part of. Wire-types map to the remainder, and determine the form and
representation of the field value as follows:

**0: varint** - the value is an opaque number, encoded as a single varint.

**1: length-delimited** - the value is a string of bytes; its length in bytes is
encoded first as a single varint, then immediately followed by exactly that many
bytes comprising the value itself.

**2: fixed-length 32 bits** - the value is a string of exactly 4 bytes, encoded
with no additional prelude.

**3: fixed-length 64 bits** - the value is a string of exaclty 8 bytes, encoded
with no additional prelude.

Note that because field keys encode only the *delta* from the previous tag, it
is not possible to encode fields in anything but sorted order according to their
tags. Unsorted fields are *unrepresentable*.

If a field key's tag-delta indicates a tag that is greater than would fit in an
unsigned 32 bit integer (2^32-1), the encoded message is not valid and **must**
be rejected.

#### Varints (LEB128-bijective encoding)

Varints are a variable-length encoding of a non-negative integer, not greater
than the maximum value representable by an unsigned 64 bit integer (2^64-1).
Encoded varints are between one and nine bytes, with lesser numeric values
having shorter representations in the encoding. At the same time, each number in
this range has exactly one possible encoded representation.

The bytes that comprise an encoded varint have their most significant bit set
whenever they are not the final byte, as a continuation bit. In varints that are
nine bytes in length, the ninth and final byte may or may not have its most
significant bit set. The first byte that does *not* have its most significant
bit set (or the ninth if each byte's most significant bit is set) is the final
byte.

The value of the encoded varint is the sum of each byte's unsigned integer
value, multiplied by 128 (shifted left by 7 bits) for each byte that preceded
it.

##### Mathematics

Bilrost's varint representation is a base 128 [bijective numeration][bn] scheme
with a continuation bit. In such a numbering scheme, each possible values in a
given scheme is greater than each possible value with fewer digits.

[bn]: https://en.wikipedia.org/wiki/Bijective_numeration

Typical bijective numerations have no zero digit, but represent zero with the
empty string. This doesn't work for us because we must always encode at least
one byte to avoid ambiguity. Consider instead:

* A base 128 bijective numeration
* which represents the digits valued 1 through 128 with the byte values 0
  through 127,
* is encoded least significant digit first with a continuation bit in the most
  significant bit of each byte,
* and encodes the represented value plus one...

...this is *almost exactly* the Bilrost varint encoding. The sole exception is
that, starting at the value 9295997013522923648 (hexadecimal
0x8102_0408_1020_4080, encoded as
`[128, 128, 128, 128, 128, 128, 128, 128, 128, 0]`) and the maximum
18446744073709551615 (hexadecimal 0xffff_ffff_ffff_ffff, encoded as
`[255, 254, 254, 254, 254, 254, 254, 254, 254, 0]`), there is always a tenth
byte and it is always zero.

For practical applications it's not necessary to be able to encode byte lengths
outside the 64 bit range, it is rare to need to encode values outside the range,
and if it were desirable to encode integer-like values larger than this (for
example, 128-bit UUIDs) it is more efficient to represent them in
length-delimited values, which take 1 extra byte to represent their size. For
these reasons, in the Bilrost varint encoding we do not encode this trailing
zero byte.

##### Example varint values and algorithms

<details><summary>Some examples of encoded varints</summary>

| Value                   | Bytes (decimal)                                 |
|-------------------------|-------------------------------------------------|
| 0                       | `[0]`                                           |
| 1                       | `[1]`                                           |
| 101                     | `[101]`                                         |
| 127                     | `[127]`                                         |
| 128                     | `[128, 0]`                                      |
| 255                     | `[255, 0]`                                      |
| 256                     | `[128, 1]`                                      |
| 1001                    | `[233, 6]`                                      |
| 16511                   | `[255, 127]`                                    |
| 16512                   | `[128, 128, 0]`                                 |
| 32895                   | `[255, 255, 0]`                                 |
| 32896                   | `[128, 128, 1]`                                 |
| 1000001                 | `[193, 131, 60]`                                |
| 1234567890              | `[150, 180, 252, 207, 3]`                       |
| 987654321123456789      | `[149, 237, 196, 218, 243, 202, 181, 217, 12]`  |
| 12345678900987654321    | `[177, 224, 156, 226, 204, 176, 169, 169, 170]` |
| (maximum `u64`: 2^64-1) | `[255, 254, 254, 254, 254, 254, 254, 254, 254]` |

</details>

<details><summary>Varint algorithm</summary>

The following is python example code, written for clarity rather than
performance:

```python
def encode_varint(n: int) -> bytes:
    assert 0 <= n < 2**64
    bytes_to_encode = []
    # Encode up to 8 preceding bytes
    while n >= 128 and len(bytes_to_encode) < 8:
        bytes_to_encode.append(128 + (n % 128))
        n = (n // 128) - 1
    # Always encode at least one byte
    bytes_to_encode.append(n)
    return bytes(bytes_to_encode)


def decode_varint_from_byte_iterator(it: Iterable[int]) -> int:
    n = 0
    num_bytes_read = 0
    for byte_value in it:
        assert 0 <= byte_value < 256
        n += byte_value * (128**num_bytes_read)
        num_bytes_read += 1
        if byte_value < 128 or num_bytes_read == 9:
            # Varints encoding values greater than 64 bits MUST be rejected
            if n >= 2**64:
                raise ValueError("invalid varint")
            return n
    # Reached end of data before the end of the varint
    raise ValueError("varint truncated")
```

</details>

### Standard interpretation

To make the encoding useful, these opaque values have standard interpretations
for many common data types.

* fixed-width encodings must be little-endian, text must be utf-8
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
    * fields' types must have an equivalence relation via their encoded
      representations
    * fields must never be present in the decoded data when they have an empty
      value
    * unknown fields must err
    * maps' keys and sets' items must be ordered

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
    * Bilrost inherently supports deterministic & canonical outputs as a banner
      feature
    * message traits are now usefully object-safe, and all the encoder traits
      can function with `&dyn Buf` and so on
    * (look over more unsolved complaints in the prost issues)

## Differences from Protobuf

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
  to fit: decoding a Bilrost message that has multiple occurrences of a non-
  repeated field in it is also an error.

### Distinguished representation on the wire in Bilrost

TODO: enumerate key differences

## Comparisons to other encodings

TODO: compare here (big table: schemaful, schemaless, distinguished) with
features, traits, and why to prefer bilrost or the other one

uses schema:

* protobuf
* capnp
* flatbuffers

schemaful but extensions break compatibility:

* rkyv
* borsh
* bincode

schemaless (key names are encoded in the data):

* asn.1 / X.690
* JSON
* bson
* msgpack
* cbor
* ion
* XML

## Strengths, Aims, and Advantages

Strengths of Bilrost's encoding include those of protocol buffers:

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

* Bilrost supports distinguished encoding for types where it makes sense, and is
  designed from a protocol level to make invalid values unrepresentable where
  possible
* Bilrost is more compact than protobuf without incurring significant overhead.
  nuances of representation in protobuf that Bilrost cannot represent or has no
  analog for are either permanently deprecated, or all conforming decoders are
  required to discard the difference anyway.
* `bilrost` aims to be as ergonomic as is practical in plain rust, with basic
  annotations and derive macros. It's possible for such a library to be quite
  nice to use!

(*The main area of potential incompatibility is with the representation of
signaling vs. quiet NaN floating point values; see `f64::to_bits()`.)

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

2. **Could the Bilrost encoding be implemented as a serializer for
   [Serde][se]?**

Probably not, though `serde` experts are free to weigh in. There are multiple
complications with trying to serialize Bilrost messages with Serde:

- Bilrost fields bear a numbered tag, and currently there appears to be no
  mechanism suitable for this in `serde`.
- Bilrost fields are also associated with a specific encoder, such as `general`
  or `fixed`, which may alter their encoding. Purely trait-based dispatch will
  work poorly for this, especially when the values become nested within other
  data structures like maps and `Vec` and encoders may begin to look
  like `map<plainbytes, packed<fixed>>`.
- Bilrost messages must encode their fields in tag order, which may (in the case
  of `oneof` fields) vary depending on their value, and it's not clear how or if
  this could be solved in `serde`.
- Bilrost has both expedient and distinguished encoding modes, and promises that
  encoding a message that implements `DistinguishedMessage` always produces
  canonical output. This may be beyond what is practical to implement.

Despite all this, it is possible to place `serde` derive tags onto the generated
types, so the same structure can support both `bilrost` and `Serde`.

[se]: https://serde.rs/

## Why "Bilrost?"

Protocol Buffers, originating at Google, took on the portmanteau "protobuf". In
turn, Protobuf for Rust became "prost".

To fork that library, one might call it... "Frost"? But that name is taken.
"Bifrost" is a nice name, and a sort of pun on "frost, 2"; but that is also
taken. "Bilrost" is another name for the original Norse "Bifrost", and it is
quite nice, so here we are.

## License

`bilrost` is distributed under the terms of the Apache License (Version 2.0).

See [LICENSE](./LICENSE) & [NOTICE](./NOTICE) in the source for details, or the
[license][ghlicense] and [notice][ghnotice] on github.

[ghlicense]: https://github.com/mumbleskates/bilrost/blob/bilrost/LICENSE

[ghnotice]: https://github.com/mumbleskates/bilrost/blob/bilrost/NOTICE

Copyright 2023-2024 Kent Ross  
Copyright 2022 Dan Burkert & Tokio Contributors
