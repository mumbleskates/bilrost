[![continuous integration](https://github.com/mumbleskates/bilrost/actions/workflows/ci.yml/badge.svg)](https://github.com/mumbleskates/bilrost/actions/workflows/ci.yml)
[![Documentation](https://docs.rs/bilrost/badge.svg)](https://docs.rs/bilrost/)
[![Crate](https://img.shields.io/crates/v/bilrost.svg)](https://crates.io/crates/bilrost)
[![Dependency Status](https://deps.rs/repo/github/mumbleskates/bilrost/status.svg)](https://deps.rs/repo/github/mumbleskates/bilrost)

# *BILROST!*

Bilrost is an encoding format designed for storing and transmitting structured
data, such as in file formats or network protocols. The encoding is binary, and
unsuitable for reading directly by humans; however, it does have other other
useful properties and advantages. This crate, `bilrost`, is its first
implementation and its first instantiation. Bilrost (as a specification) strives
to provide a superset of the capabilities of protocol buffers while reducing
some of the surface area for mistakes and surprises; `bilrost` (the implementing
library) strives to provide access to all of those capabilities with maximum
convenience.

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

- [Quick start](#getting-started)
    - [Using the derive macros](#deriving-message)
    - [Encoding and decoding](#encoding-and-decoding-messages)
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

*This readme is the result of a lot of work, and we want it to be good! If
anything is unclear or could be improved, please feel free to submit issues or
pull requests!*

## Conceptual overview

Bilrost is an encoding scheme for converting in-memory data structs into plain
byte strings and vice versa. It's generally suitable for both network transport
and data retained over the long-term. Its encoded data is not human-readable,
but it is encoded quite simply. It supports integral and floating point numbers,
strings and byte strings, nested messages, and [recursively](
#writing-recursive-messages) nested messages. All of the above are supported as
optional values, repeated values, sets of unique values, and key/value mappings
where sensible. With appropriate choices of encodings (which determine the
representation), most of these constructs can be nested almost arbitrarily.

Encoded Bilrost data does not include the names of its fields; they are instead
assigned numbers agreed upon in advance by the message schema that specifies it.
This can make the data much more compact than "schemaless" encodings like JSON,
CBOR, etc., without sacrificing its extensibility: new fields can be added, and
old fields removed, without necessarily breaking backwards compatibility with
older versions of the encoding program. In the typical "expedient" decoding
mode, any field not in the message schema is ignored when decoding, so if fields
are added or removed over time the fields that remain in common will still be
mutually intelligible between the two versions of the schema. In this way,
Bilrost is very similar to [protobuf][pb]. See also:
[Design philosophy](#design-philosophy), [Comparisons to other encodings](
#comparisons-to-other-encodings), and the [Encoding specification](#encoding-specification).

Bilrost also has the ability to encode and decode data that is guaranteed to be
canonically represented: see the section on [distinguished decoding](
#distinguished-decoding).

### Design philosophy

Bilrost is designed to be an encoding format that is simple to specify, simple
to implement, simple to port across languages and machines, and easy to use
correctly.

#### Schema-ful encoding

It is designed as a data model that has a schema, though it can of course also
be used to encode representations of "schemaless" data. There are advantages and
disadvantages to this form. The encoded data is significantly smaller, since
repetitive names of fields are replaced with surrogate numbers. At the same
time, it may be less clear what the data means because the inherent
documentation of the fields' names is missing. Schemaless encodings like JSON
can be decoded and accessed dynamically as pure data with far simpler, unified
decoder implementations, whereas encodings like Bilrost and protobuf require a
schema to even be sure of the values.

One argument is that even if fields' names are all specified in the encoding,
they are merely low-information documentation that aids *guessing* or
reverse-engineering. They can help diagnose where *lost* data belongs, or what
*mystery* data means by lightly self-documenting, but the *meaning* of the data
is still determined by the code that emitted it. Data has meaning based on where
it is found, and the documentation of that meaning cannot be fully replaced by
simply including the names of all the fields in the data.

Once that argument is conceded and a project is committed to maintaining schemas
for its encoded data, there are no further distinct disadvantages. Numeric field
tags should not be reused after they are deprecated, but neither should field
names in a schemaless encoding.

Perhaps the biggest caveat is the simultaneous invention problem. If multiple
parties were to implement extensions without communicating with each other they
may choose the same tags, which would cause conflicts in the meaning of those
fields. Sequential numeric tags are *more likely* to be chosen in conflict by
both parties than names would be. The best way to resolve this is to plan ahead
for extensions and encourage potential collaborators to synchronize and choose
allocated tags from some range reserved for extensions, or provide space for
extensions within the schema that have names or UUIDs.

#### Non-coercion of data

Bilrost aims to ensure that when a message is decoded without error, all the
recognized values in its schema will have the exact value they were encoded
with. This means that:

* For boolean fields, 0 represents `false` and 1 represents `true`; if the value
  2 is encountered, this is always an error.
* For numeric fields, out-of-range values are never truncated to fit in a
  smaller numeric type.
* In `bilrost` (this Rust library), floating point values always round trip with
  the *precise* bits of their representation. NaN bits and -0.0 are always
  preserved.
* If an key appears in a mapping multiple times, the whole message is considered
  invalid; likewise for values in sets. There should be no room for alternate
  interpretations of data that keep only the first or last such entry, or that
  discard information about a set with repeated elements.

Bilrost does not enforce these same constraints for unknown field data; if
fields with tags not present in the schema are found in data, it will not be
considered canonical but decoding may succeed. Because those fields are
discarded, they are also not being coerced into different values so the promise
holds.

#### Designed for canonicity

Bilrost is designed to make several classes of non-canonical states
unrepresentable, making detection of non-canonical data far less complex.

The biggest change is that message fields encoded out of order are
unrepresentable; in protobuf this has long been an observed behavior for most
message types, but has never been *promised* for a few reasons that are less
relevant here (and are [discussed below](#differences-from-protobuf)). This
increases the complexity of *encoding* the data *only* when a "oneof" (set of
mutually exclusive fields) has tag numbers that may appear in different places
in the ordering of a message's fields; in practice this is quite rare.

The smaller change is that the [varint representation](
#varints-leb128-bijective-encoding) that makes up the core of the encoding is
designed to guarantee that there can only be a single representation for any
given number. This may be marginally more expensive than traditional
[LEB128][leb128] varints, but not by as much as one might think; rapid decoding
of LEB128 varints is [quite complex][vectorleb128], and the biggest optimization
for most varints is to take a shortcut when the value is small enough to fit in
one byte, the range in which Bilrost's varints encode identically.

[leb128]: https://en.wikipedia.org/wiki/LEB128

[vectorleb128]: https://arxiv.org/pdf/1503.07387.pdf

### Distinguished decoding

It is possible in `bilrost` to derive an extended trait, `DistinguishedMessage`,
which provides a distinguished decoding mode. Decoding in distinguished mode
comes with an additional canonicity check: the decoding result makes it possible
to know whether the decoded message data was canonical. Any message type that
*can* implement distinguished decoding *will* always encode in its fully
canonical form; there is not an alternate encoding mode that is "more
canonical".

Formally, when a message type implements `DistinguishedMessage`, values of
the message type are *bijective* to a subset of all byte strings, each of which
is considered to be a canonical encoding for that message value. Each different
possible byte string decodes in distinguished mode to a message value that is
distinct from the message values decoded from every other such byte string, or
will produce an error or non-canonical result when decoded in this mode. If a
message is successfully and canonically decoded from a byte string in
distinguished mode, is not modified, and is then re-encoded, it will emit the
exact same byte string.

For this reason, `bilrost` will refuse to derive `DistinguishedMessage` if there
are any ignored fields, as they may also participate in the type's equality.

The best proxy of this expectation of an [equivalence relation][equiv] in Rust
is the [`Eq`][eq] trait, which denotes that there is an equivalence relation
between all values of any type that implements it. Therefore, this trait is
required of all field and message types in order to implement distinguished
decoding in `bilrost`.

`bilrost` distinguishes between canonical values of the type in a way that
matches the automatically derived implementation of `Eq` (that is, it matches
based on the `Eq` trait of each constituent field). It is ***strongly
recommended,*** but not required, that the equality traits be derived
automatically. `bilrost` does not directly rely on the implementation of the
type's equality at all; rather, it acts as a contractual guardrail, setting a
minimum expectation.

[equiv]: https://en.wikipedia.org/wiki/Equivalence_relation

[eq]: https://doc.rust-lang.org/std/cmp/trait.Eq.html

Normal ("expedient") decoding may accept other byte strings as valid
encodings of a given value, such as encodings that contain unknown fields or
non-canonically encoded values[^noncanon]. Most of the time, this is what is
desired.

[^noncanon]: "Non-canonical" value encodings in Bilrost principally include
fields that are represented in the encoding even though their value is
considered empty. For message types, such as nested messages, it also includes
the message representation containing fields with unknown tags.

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

#### Canonical order and distinguished representation

Bilrost specifies most of what is required to make these message schemas
portable not just across architectures and programs, but to other programming
languages as well. There is currently one minor caveat: The *sort order* of
values in Bilrost may matter.

In distinguished decoding mode, canonical data must always be represented with
*sets* and *maps* having their items in sorted order. When the item type of a
set (or the key type of a map) is not a simple type with an already-standardized
sorting order (such as an integer or string), the canonical order of the items
depends on that type's implementation, and care must be taken to standardize
that order in addition to the schema of the message's fields when defining
distinguished types.

#### Floating point values and distinguished decoding

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
decoding for third-party wrappers for Rust's floating point types that implement
[`Eq`][eq] and [`Ord`][ord] (such as [`ordered_float`][ordered_float] and
[`decorum`][decorum]) because they still consider some sets of values that have
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

## Using the library

### Getting started

To use `bilrost`, we first add it as a dependency in `Cargo.toml`, either with
`cargo add bilrost` or manually:

```toml
bilrost = "0.1005.0-dev"
```

Then, we derive `bilrost::Message` for our struct type:

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
  derive macros. It's unlikely this should ever be disabled if `bilrost` is used
  normally.
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
bilrost = { version = "0.1005.0-dev", default-features = false, features = ["derive"] }
```

### Derive macros

We can now import and use its traits and derive macros. The main three are:

* [`Message`](#deriving-message): This is the basic working unit. Derive this
  for structs to enable encoding and decoding them to and from binary data.
* [`Enumeration`](#enumerations): This is a derive only, not a trait, which
  implements support for encoding an enum type with `bilrost`. The enum must
  have no fields, and each of its variants will correspond to a different `u32`
  value that will represent it in the encoding.
* [`Oneof`](#oneof-fields): This is a trait and derive macro for enumerations
  representing mutually exclusive fields within a message struct. Each variant
  must have one field, and each variant must have a unique field tag assigned to
  it, *both* within the oneof and within the message of which it is a part.
  Types with `Oneof` derived do not have `bilrost` APIs useful to library users
  except when they are included in a `Message` struct.

#### Deriving `Message`

The `Message` trait can be derived to allow encoding just about any struct as a
Bilrost message, as long as its fields' types are supported.

If not otherwise specified, fields are tagged sequentially in the order they
are specified in the struct, starting with `1`.

Tags can also be explicitly specified. If a field's tag is the only attribute
provided, the number of the tag can be provided with no ceremony as the only
content of the "bilrost" attribute, like `#[bilrost(1)]`. If other attributes
are included, the "tag" attribute must be specified by name; for example, like
`#[bilrost(tag(1), encoding(fixed))]`. The "tag" attribute can also be spelled
`tag = 1` or `tag = "1"`.

We may skip tags which have been reserved, or where there are gaps between
sequentially occurring tag values by specifying the tag number to skip to with
the `tag` attribute on the first field after the gap. The following fields will
be tagged sequentially starting from the next number.

When defining message types for interoperation, or when fields are likely to
be added, removed, or shuffled, it may be good practice to explicitly specify
the tags of all fields in a struct instead, but this is not mandatory.

```rust,
use bilrost::{Enumeration, Message};

#[derive(Clone, PartialEq, Message)]
struct Person {
    #[bilrost(tag = 1)]
    pub id: String, // tag=1
    // NOTE: Old "name" field has been removed
    // pub name: String,
    // given_name has tag 6
    #[bilrost(6)]
    pub given_name: String,
    // family_name has tag 7
    pub family_name: String,
    // formatted_name has tag 8
    pub formatted_name: String,
    // age has tag 3
    #[bilrost(tag = "3")]
    pub age: u32,
    // height has tag 4
    pub height: u32,
    // gender has tag 5
    #[bilrost(enumeration(Gender))]
    pub gender: u32,
    // NOTE: Skip to less commonly occurring fields
    #[bilrost(tag(16))]
    pub name_prefix: String, // has tag 16  (eg. mr/mrs/ms)
    pub name_suffix: String, // has tag 17  (eg. jr/esq)
    pub maiden_name: String, // has tag 18
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Enumeration)]
#[non_exhaustive]
pub enum Gender {
    Unknown = 0,
    Female = 1,
    Male = 2,
    Nonbinary = 3,
}
```

#### Oneof fields

Bilrost messages can have sets of mutually exclusive fields, only one of which
may be present at a time. These are represented by `enum` types where each
variant has one field and is assigned a field tag; the `Oneof` derive macro can
then be used to derive an implementation that allow the oneof to be included in
a message.

<details><summary>Example message with a oneof</summary>

```rust
use bilrost::{Message, Oneof};

#[derive(Oneof)]
enum NameOrUUID {
    #[bilrost(2)]
    Name(String),
    #[bilrost(tag(3), encoding(plainbytes))]
    UUID([u8; 16]),
}

#[derive(Message)]
struct Widget {
    #[bilrost(1)]
    id: u32,
    #[bilrost(oneof(2, 3))]
    label: Option<NameOrUUID>,
    #[bilrost(4)]
    description: String,
}
```

</details>

When the oneof is included in a message, it has to be declared with the "oneof"
attribute, providing a comma-separated list of all its field tags. (This
attribute can also be spelled like `oneof = "2, 3"`.)[^tagranges] It isn't
possible for the derive macro to know what those tag numbers are when it runs
because it can't have access to the definitions of the field's type, but the
list of tags declared in this attribute and the list of tags that the oneof
actually has are statically checked for equality at compile time.

[^tagranges]: The way the full list of tags is specified within the `oneof`
attribute and the `reserved_tags` attribute is the same: the whole list is comma
separated, and each item may be either a single tag number or an inclusive range
from minimum to maximum separated with a dash (like `1-5`). For both
`reserved_tags` and `oneof`, the following are all exactly equivalent:
`1, 2, 3, 4, 5`; `1-5`; `4, 5, 1-3`

The field tags in the oneof must be unique, both within the oneof itself and
within any message containing it. Oneof variants can only contain types that
can be nested (so "unpacked" collections cannot be supported). Mechanically, a
oneof works exactly the same as if there were an `Option<T>` field for each of
its variants, except at most one of them can be `Some`.

In the example above, the `NameOrUUID` oneof must be nested in an `Option` to
enable it to represent the empty state where none of its fields are present. It
is also possible to include *up to one* unit variant in a oneof enum. Any such
variant will be used to represent its empty state.

<details><summary>Example of a oneof with an "empty" variant</summary>

```rust
use bilrost::{Message, Oneof};

#[derive(Oneof)]
enum NameOrUUID {
    #[bilrost(2)]
    Name(String),
    #[bilrost(tag(3), encoding(plainbytes))]
    UUID {
        octets: [u8; 16],
    },
    Neither,
}

#[derive(Message)]
struct Widget {
    #[bilrost(1)]
    id: u32,
    #[bilrost(oneof(2, 3))]
    label: NameOrUUID,
    #[bilrost(4)]
    description: String,
}
```

</details>

When a oneof enum type has the empty variant, it can only be included in a
message directly; when it has none, it can only be included nested within an
`Option`.

#### Encodings

`bilrost` message fields and oneof variants can be annotated with an "encoding"
attribute that specifies which encoding type is used when encoding and decoding
that field's value. `bilrost` provides several standard encodings which can be
used and composed to choose how the field is represented.

```rust,
# use bilrost::Message;
#[derive(Message)]
struct Foo {
    #[bilrost(encoding(general))]
    name: String,
}
```

Encoding attributes can be specified two ways, either in the form shown above or
as a string, like `#[bilrost(encoding = "general")]`. The value of this
attribute specifies a type name, using normal Rust type syntax. The standard
encodings are also available and can be addressed explicitly; there is no
practical reason to do this, but as a demonstration:

```rust,
# use bilrost::Message;
#[derive(Message)]
struct Bar(
    // This is the same type as "general"
    #[bilrost(encoding = "::bilrost::encoding::General")] String,
);

assert_eq!(
    Bar("bar".to_string()).encode_to_vec(),
    b"\x05\x03bar".as_slice()
);
```

Where these encodings' type names are evaluated the standard encodings are made
available as aliases, all-lower-cased to ensure that these aliases are unlikely
to collide with other type names that are in scope. These standard aliases are:

* `general`: the default encoding, suitable for most field types. Delegates
  encoding of collections (vecs and sets) to `unpacked<general>` and mapping
  types to `map<general, general>`.
* `varint`: primitive numeric types and bool, encodes as varint.
* `fixed`: fixed-width four- and eight-byte values for integers, floats, and
  byte arrays. Delegates encoding of collections to `unpacked<fixed>`
* `plainbytes`: encodes byte arrays, `Vec<u8>`, and `Cow<[u8]>` as
  length-delimited values. Delegates encoding of `Vec<Vec<u8>>`
  and `Vec<Cow<[u8]>>` to `unpacked<plainbytes>`
* `unpacked` (`unpacked<E = general>`): : encodes collections with their values
  unpacked as zero or more normally encoded fields, one per value. The fields
  are encoded with the parametrized encoding `E`, which defaults to `general`
* `packed` (`packed<E = general>`): encodes collections with their values packed
  into a single length-delimited value. The values are encoded with the
  parametrized encoding `E`, which defaults to `general`
* `map<KE, VE>`: encodes mappings with their keys (encoded with parametrized
  encoding `KE`) and values (encoded with `VE`) packed alternating into a single
  length-delimited value.

It's possible that more standard encodings may be added in the future, but they
will be similarly lower-cased.

#### Other attributes

There are a few other attributes available inside the "bilrost" attribute:

##### Reserving tags

* **"reserved_tags"**: When placed on the message itself, this declares that the
  given tags and tag ranges[^tagranges] are not used in the field. This has no
  effect other than as a compile-time guard; if a field uses a tag that was
  declared to be reserved the compilation will err.

```rust,compile_fail
# use bilrost::Message;
#[derive(Message)]
#[bilrost(reserved_tags(2, 6-10, 25))]
struct Foo {
    #[bilrost(tag(5), encoding(general))]
    name: String,
    age: int64, // Oops! Uses tag 6! Compile error
}
```

##### Ignoring fields

* **"ignore"**: Must be alone, with no tag or other attribute. This causes the
  field to be ignored by the generated message implementation. If any fields in
  a message are ignored, it must implement `Default` to implement `Message`, so
  there is a value for those fields to take on when they are created from
  encoded data.

  Ignored fields are not currently considered compatible with distinguished
  decoding.

##### Helper methods

* **"enumeration"**: If a field is of type `u32` or `Option<u32>`, this causes
  the message type to have helper methods named after the type that get and set
  its value as the enumeration type specified by this attribute.

##### Writing recursive messages

* **"recurses"**: It is possible to nest messages recursively in `bilrost`. If
  they are, the `Message` traits are currently all always disabled because there
  is an unresolvable circular dependency of a message type on its own traits:

```rust,compile_fail
# use bilrost::Message;
#[derive(Message)]
//       ^^^^^^^ the trait `Encoder<Vec<Tree>>` is not implemented for `General`
struct Tree {
    name: String,
    children: Vec<Tree>,
}
```

Somewhere along the line, we have to break this circular chain of dependencies.
To do that, annotate one of the fields in the chain with the "recurses"
attribute and its type will no longer participate in the `where` clause of the
message implementations, the cycle will be broken, and the message can be used:

```rust,
# use bilrost::Message;
#[derive(Message)]
struct Tree {
    name: String,
    #[bilrost(recurses)]
    children: Vec<Tree>,
}
```

### Distinguished derive macros

There are two derivable companion traits, `DistinguishedMessage`
and `DistinguishedOneof`, that implement the extended traits for distinguished
decoding when possible. Both messages and oneofs must contain only fields that
support distinguished decoding in order to support it themselves. Distinguished
encoding requires `Eq` be implemented for each field, oneof, and message type;
the trait is not used directly, but is trivial to derive for any compatible
type.

### Encoding and decoding messages

There are a variety of methods and associated functions available for encoding
and decoding data in `Message` implementations:

* `encode`: encodes the message into a `&mut bytes::BufMut`
* `encode_length_delimited`: encodes the message prefixed with a length
  delimiter
* `decode`, `decode_length_delimited`: decodes the message likewise, from a
  `bytes::Buf`
* `replace_from`, `replace_from_length_delimited`: like `decode`, but rather
  than returning a `Result` with a new instance of the message, these are
  mutating methods that replace the value in an existing instance. If decoding
  fails, the message will be left with its fields empty.
* `encode_to_vec`, `encode_to_slice`, `encode_length_delimited_to_vec`, ...:
  encodes the message into a new vec or bytes and returns that container
* There are also `encode_dyn`, `replace_from_slice`, and `replace_from_dyn`
  methods for encoding and decoding that do not provide anything the above
  methods do not, but are callable from a trait object.

#### Decoding in distinguished mode

`DistinguishedMessage` has corresponding methods for decoding and replacing
named `decode_distinguished_..` and `replace_distinguished_..`. Instead of
returning `Result<(), DecodeError>` or `Result<Foo, DecodeError>`, these return
`Result<Canonicity, DecodeError>` or `Result<(Foo, Canonicity), DecodeError>`.
`Canonicity` is a simple enum that indicates whether the decoded data was
`Canonical`, `HasExtensions`, or is `NotCanonical`.

The `bilrost::WithCanonicity` trait is made available to unwrap values and
results that have canonicity information:

* `.canonical()`: Converts to an error if not fully canonical, otherwise unwraps
* `.canonical_with_extensions()`: Converts to an error if any *known* fields
  were not canonical, otherwise unwraps
* `.value()`: Always unwraps, discarding the canonicity information.

This trait is implemented for `Canonicity` itself, `(T, Canonicity)`, `Result`
types where the value implements `WithCanonicity` and the error is convertible
to `DecodeErrorKind`, and corresponding references/[`.as_ref()`][resref] types.
The error in the returned result types is `DecodeErrorKind`, which discards any
"detailed-errors" information that would have indicated which field a decode
error occurred in; if that information is needed, check the decoding error
before the canonicity error.

[resref]: https://doc.rust-lang.org/std/result/enum.Result.html#method.as_ref

#### Using `dyn` with object-safe message traits

The `Message` and `DistinguishedMessage` traits are object-safe and can be used
via [trait objects][traitobj]. All of their functionality (except the `decode`
methods for creating a message value from data *ex nihilo*) is available via
object-safe alternatives. Messages can be cleared (reset to empty values);
measured for their encoded byte length; encoded to [`Vec<u8>`][vec],
[`Bytes`][bytes], or into a [`&mut dyn Buf`][bytes]; or decoded (replacing the
value) from [`&[u8]` slice][slice] or a [`&mut dyn BufMut`][bufmut].

[buf]: https://docs.rs/bytes/latest/bytes/buf/trait.Buf.html

[bufmut]: https://docs.rs/bytes/latest/bytes/buf/trait.BufMut.html

[slice]: https://doc.rust-lang.org/std/primitive.slice.html

[traitobj]: https://doc.rust-lang.org/reference/types/trait-object.html

Methods that decode to or from trait object buffers are likely to be less
efficient than their generic, non-object-safe counterparts; it is preferable to
use `encode(..)` rather than `encode_dyn(..)`, and likewise for any other
"`_dyn`" method. Likewise, `replace_from_slice(..)` is equivalent to
`replace_from(..)`, just object safe; the same goes for other "`_slice`"
methods.

### Encoding and decoding example

```rust,
use bilrost::{
    DistinguishedMessage, DistinguishedOneof, Message, Oneof,
    WithCanonicity,
};
use bytes::Bytes;
use std::collections::BTreeMap;

#[derive(Debug, PartialEq, Eq, Oneof, DistinguishedOneof)]
enum PubKeyMaterial {
    Empty,
    #[bilrost(1)]
    Rsa(Bytes),
    #[bilrost(2)]
    ED25519(Bytes),
}

use PubKeyMaterial::*;

#[derive(Debug, PartialEq, Eq, Message, DistinguishedMessage)]
struct PubKey {
    #[bilrost(oneof(1, 2))]
    key: PubKeyMaterial,
    #[bilrost(3)]
    expiry: i64, // See also: `bilrost_types::Timestamp`
}

#[derive(Debug, Default, PartialEq, Eq, Message, DistinguishedMessage)]
struct PubKeyRegistry {
    keys_by_owner: BTreeMap<String, PubKey>,
}

let mut registry = PubKeyRegistry::default();
registry.keys_by_owner.insert(
    "Alice".to_string(),
    PubKey {
        key: ED25519(Bytes::from_static(b"not a secret")),
        expiry: 1600999999,
    },
);
registry.keys_by_owner.insert(
    "Bob".to_string(),
    PubKey {
        key: Rsa(Bytes::from_static(b"pkey")),
        expiry: 1500000001,
    },
);
let encoded = registry.encode_to_vec();

// The binary of this encoded message breaks down as follows:
//
// (The first and only field, containing a map from String to PubKey)
// 05 - field key: tag 0+1 = 1, wire type 1 = length-delimited
//   2c - length: 44 bytes
//     (The key of the first map item, a String value)
//     05 - length: 5 bytes
//       "Alice"
//     (The value of the first map item, a PubKey message)
//     14 - length: 20 bytes
//       (The "ED25519" variant of the PubKeyMaterial oneof)
//       09 - field key: tag 0+2 = 2, wire type 1 = length-delimited
//         (A String value)
//         0c - length: 12 bytes
//           "not a secret"
//       (The "expiry" field of the PubKey message, an i64)
//       04 - field key: tag 2+1 = 3, wire type 0 = varint
//         fec7e9f50a - varint 3201999998, which is +1600999999 in zig-zag
//     (The key of the second map item, a string value)
//     03 - length: 3 bytes
//       "Bob"
//     (The value of the second map item, another PubKey message)
//     0c - length: 12 bytes
//       (The "RSA" variant of the PubKeyMaterial oneof)
//       05 - field key: tag 0+1 = 1, wire type 1 = length-delimited
//         (A String value)
//         04 - length: 4 bytes
//           "pkey"
//       (The "expiry" field of the PubKey message, an i64)
//       08 - field key: tag 1+2 = 3, wire type 0 = varint
//         82bbc0950a - varint 3000000002, which is +1500000001 in zig-zag

assert_eq!(
    encoded,
    b"\x05\x2c\
      \x05Alice\x14\x09\x0cnot a secret\x04\xfe\xc7\xe9\xf5\x0a\
      \x03Bob\x0c\x05\x04pkey\x08\x82\xbb\xc0\x95\x0a"
        .as_slice()
);

let decoded = PubKeyRegistry::decode_distinguished(encoded.as_slice())
    .canonical() // Check that the decoded data was canonical
    .unwrap();
assert_eq!(registry, decoded);
```

### Supported message field types

`bilrost` structs can encode fields with a wide variety of types:

| Encoding             | Value type                                    | Encoded representation | Distinguished |
|----------------------|-----------------------------------------------|------------------------|---------------|
| `general` & `fixed`  | [`f32`][prim]                                 | fixed-size 32 bits     | no            |
| `general` & `fixed`  | [`u32`][prim], [`i32`][prim]                  | fixed-size 32 bits     | yes           |
| `general` & `fixed`  | [`f64`][prim]                                 | fixed-size 64 bits     | no            |
| `general` & `fixed`  | [`u64`][prim], [`i64`][prim]                  | fixed-size 64 bits     | yes           |
| `general` & `varint` | [`u64`][prim], [`u32`][prim], [`u16`][prim]   | varint                 | yes           |
| `general` & `varint` | [`i64`][prim], [`i32`][prim], [`i16`][prim]   | varint                 | yes           |
| `general` & `varint` | [`bool`][prim]                                | varint                 | yes           |
| `general`            | derived [`Enumeration`](#enumerations)[^enum] | varint                 | yes           |
| `general`            | [`String`][str]*                              | length-delimited       | yes           |
| `general`            | impl [`Message`](#derive-macros)[^boxmsg]     | length-delimited       | maybe         |
| `varint`             | [`u8`][prim], [`i8`][prim]                    | varint                 | yes           |
| `plainbytes`         | [`Vec<u8>`][vec]*                             | length-delimited       | yes           |

*Alternative types are available! See below.

[^enum]: `Enumeration` types can be directly included if they have a value that
has a Bilrost representation of zero (represented as exactly the expression `0`
either via a `#[bilrost(0)]` attribute or, absent an attribute, via a normal
discriminant value). Otherwise, enumeration types must always be nested.

[^boxmsg]: `Message` types inside [`Box`][box] still impl `Message`, with a
covering impl; message types [can nest recursively](#writing-recursive-messages)
this way.

Any of these types may be included directly in a `bilrost` message struct. If
that field's value is [empty](#empty-values), no bytes will be emitted when it
is encoded.

In addition to including them directly, these types can also be nested within
several different containers:

| Encoding      | Value type                              | Encoded representation                                                         | Re-nestable | Distinguished      |
|---------------|-----------------------------------------|--------------------------------------------------------------------------------|-------------|--------------------|
| any encoding  | [`Option<T>`][opt]                      | identical; at least some bytes are always encoded if `Some`, nothing if `None` | no          | when `T` is        |
| `unpacked<E>` | [`Vec<T>`][vec], [`BTreeSet<T>`][btset] | the same as encoding `E`, one field per value                                  | no          | when `T` is        |
| `unpacked`    | *                                       | (the same as `unpacked<general>`)                                              | no          | *                  |
| `packed<E>`   | [`Vec<T>`][vec], [`BTreeSet<T>`][btset] | always length-delimited, successively encoded with `E`                         | yes         | when `T` is        |
| `packed`      | *                                       | (the same as `packed<general>`)                                                | yes         | *                  |
| `map<KE, VE>` | [`BTreeMap<K, V>`][btmap]               | always length-delimited, alternately encoded with `KE` and `VE`                | yes         | when `K` & `V` are |
| `general`     | [`Vec<T>`][vec], [`BTreeSet<T>`][btset] | (the same as `unpacked`)                                                       | no          | *                  |
| `general`     | [`BTreeMap`][btmap]                     | (the same as `map<general, general>`)                                          | yes         | *                  |

Many alternative types are also available for both scalar values and containers!

| Value type   | Alternative                               | Supporting encoding | Distinguished | Feature to enable |
|--------------|-------------------------------------------|---------------------|---------------|-------------------|
| `Vec<u8>`    | `Blob`[^blob]                             | `general`           | yes           | (none)            |
| `Vec<u8>`    | [`Cow<[u8]>`][cow]                        | `plainbytes`        | yes           | (none)            |
| `Vec<u8>`    | [`bytes::Bytes`][bytes][^bzcopy]          | `general`           | yes           | (none)            |
| `Vec<u8>`    | [`[u8; N]`][prim][^plainbytearr]          | `plainbytes`        | yes           | (none)            |
| `u32`, `u64` | [`[u8; 4]`][prim], [`[u8; 8]`][prim]      | `fixed`             | yes           | (none)            |
| `String`     | [`Cow<str>`][cow]                         | `general`           | yes           | (none)            |
| `String`     | [`bytestring::ByteString`][bstr][^bzcopy] | `general`           | yes           | "bytestring"      |

[^bzcopy]: When decoding from a `bytes::Bytes` object, both `bytes::Bytes` and
`bytes::ByteString` have a zero-copy optimization and will reference the decoded
buffer rather than copying. (This could also work for any other input type that
has a zero-copy `bytes::Buf::copy_to_bytes()` optimization.)

[^plainbytearr]: Plain byte arrays, as we might expect, only accept one exact
length of data; other lengths are considered invalid values.

[^blob]: `bilrost::Blob` is a transparent wrapper for `Vec<u8>` in that is a
drop-in replacement in most situations and is supported by the default `general`
encoding for maximum ease of use. If nothing but `Vec<u8>` will do,
the `plainbytes` encoding will still encode a plain `Vec<u8>` as its bytes
value.

| Container type | Alternative                                     | Distinguished | Feature to enable |
|----------------|-------------------------------------------------|---------------|-------------------|
| `Vec<T>`       | [`Cow<[T]>`][cow]                               | when `T` is   | (none)            |
| `Vec<T>`       | [`smallvec::SmallVec<[T]>`][smallvec]           | when `T` is   | "smallvec"        |
| `Vec<T>`       | [`thin_vec::ThinVec<[T]>`][thinvec]             | when `T` is   | "thin_vec"        |
| `Vec<T>`       | [`tinyvec::TinyVec<[T]>`][tinyvec]              | when `T` is   | "tinyvec"         |
| `BTreeMap<T>`  | [`HashMap<T>`][hashmap][^hashnoncanon]          | no            | "std" (default)   |
| `BTreeSet<T>`  | [`HashSet<T>`][hashset][^hashnoncanon]          | no            | "std" (default)   |
| `BTreeMap<T>`  | [`hashbrown::HashMap<T>`][hbmap][^hashnoncanon] | no            | "hashbrown"       |
| `BTreeSet<T>`  | [`hashbrown::HashSet<T>`][hbset][^hashnoncanon] | no            | "hashbrown"       |

[box]: https://doc.rust-lang.org/std/boxed/struct.Box.html

[bstr]: https://docs.rs/bytestring/latest/bytestring/struct.ByteString.html

[btmap]: https://doc.rust-lang.org/std/collections/btree_map/struct.BTreeMap.html

[btset]: https://doc.rust-lang.org/std/collections/struct.BTreeSet.html

[bytes]: https://docs.rs/bytes/latest/bytes/struct.Bytes.html

[cow]: https://doc.rust-lang.org/std/borrow/enum.Cow.html

[hashmap]: https://doc.rust-lang.org/std/collections/struct.HashMap.html

[hashset]: https://doc.rust-lang.org/std/collections/struct.HashSet.html

[hbmap]: https://docs.rs/hashbrown/latest/hashbrown/struct.HashMap.html

[hbset]: https://docs.rs/hashbrown/latest/hashbrown/struct.HashSet.html

[opt]: https://doc.rust-lang.org/std/option/enum.Option.html

[prim]: https://doc.rust-lang.org/std/index.html#primitives

[smallvec]: https://docs.rs/smallvec/latest/smallvec/struct.SmallVec.html

[str]: https://doc.rust-lang.org/std/string/struct.String.html

[thinvec]: https://docs.rs/thin-vec/latest/thin_vec/struct.ThinVec.html

[tinyvec]: https://docs.rs/tinyvec/latest/tinyvec/enum.TinyVec.html

[vec]: https://doc.rust-lang.org/std/vec/struct.Vec.html

[^hashnoncanon]: Hash-table-based maps and sets are implemented, but are not
compatible with distinguished encoding or decoding. If distinguished decoding is
required, a container which stores its values in sorted order must be used.

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
   `u32` const expression and match pattern (here with the example value `123`).

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

If the discriminants of an enumeration conflict at all, compilation will fail;
the discriminants must be unique within any given enumeration.

```rust,compile_fail
# use bilrost::Enumeration;
#[derive(Clone, PartialEq, Eq, Enumeration)]
enum Foo {
    A = 1,
    #[bilrost(1)] // error: unreachable pattern
    B = 2,
}
```

For an enumeration type to qualify for direct inclusion as a message field
rather than only as a nested value (within `Option`, `Vec`, etc.), one of the
discriminants must be spelled exactly "0".

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

| Change                                                                                 | Corresponding values                | Backwards compatibility breaks when...                         |
|----------------------------------------------------------------------------------------|-------------------------------------|----------------------------------------------------------------|
| `bool` --> `u8` --> `u16` --> `u32` --> `u64`, all with `general` or `varint` encoding | `true`/`false` becomes 1/0          | value is out of range of the narrower type                     |
| `bool` --> `i8` --> `i16` --> `i32` --> `i64`, all with `general` or `varint` encoding | `true`/`false` becomes -1/0         | value is out of range of the narrower type                     |
| `String` --> `Vec<u8>`                                                                 | string becomes its UTF-8 data       | value contains invalid UTF-8                                   |
| `[u8; N]` with `general` encoding --> `Vec<u8>`                                        | no change                           | data is a different length than the array                      |
| `T` --> `Option<T>`                                                                    | default value of `T` becomes `None` | `Some(default)` is encoded, then decoded in distinguished mode |
| `Option<T>` --> `Vec<T>` (with `unpacked` encoding)                                    | maybe-contained value is identical  | multiple values are in the `Vec`                               |

`Vec<T>` and other list- and set-like collections that contain repeated values
can also be changed between `unpacked` and `packed` encoding, as long as the
inner value type `T` does not have a length-delimited representation. This will
break compatibility with distinguished decoding in both directions whenever the
field is present and not [empty](#empty-values) because it will also change the
encoded representation, but expedient decoding will still work.

## Strengths, Aims, and Advantages

Strengths of Bilrost's encoding include those of protocol buffers:

* Encoded messages are very durable, with greatly extensible forward
  compatibility
* Encoded messages are relatively very compact, and their representation "on the
  wire" is very simple
* The encoding is minimally[^floatbits] platform-dependent; each byte is
  specified, and there are no endianness incompatibility issues
* When decoding, text-string and byte-string data is represented verbatim and
  can be referenced without copying
* Skipping irrelevant, undesired, or unknown-extension data is inexpensive as
  most nested and repeated fields are stored with a length prefix

...as well as more:

* In Bilrost, decoded data means what it says. If a value is decoded, it
  contains all the information that was present in the encoding (no silent
  integer truncation!)
* Bilrost supports distinguished decoding for types where it makes sense, and is
  designed from a protocol level to make invalid values unrepresentable where
  possible
* Bilrost is more compact than protobuf without incurring significant overhead.
  nuances of representation in protobuf that Bilrost cannot represent or has no
  analog for are either permanently deprecated, or all conforming decoders are
  required to discard the difference anyway.
* `bilrost` aims to be as ergonomic as is practical in plain rust, with basic
  annotations and derive macros. It's possible for such a library to be quite
  nice to use!

[^floatbits]: The main area of potential incompatibility is with the
representation of signaling vs. quiet NaN floating point values; see
[`f64::from_bits()`][floatbits].

[floatbits]: https://doc.rust-lang.org/std/primitive.f64.html#method.from_bits

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
previously-encoded field (or zero, if it is the first field) and the tag of the
field the key is part of. Wire-types map to the remainder, and determine the
form and representation of the field value as follows:

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
unsigned 32 bit integer (2^32-1), the encoded message is not valid and must be
rejected.

#### Varints (LEB128-bijective encoding)

Varints are a variable-length encoding of an unsigned 64 bit integer value.
Encoded varints are between one and nine bytes, with lesser numeric values
having shorter representations in the encoding. At the same time, each number in
this range has exactly one possible encoded representation.

1. The final byte of a varint is the first byte that does not have its most
   significant bit set, or the ninth byte, whichever comes first.
2. The value of the encoded varint is the sum of each byte's unsigned integer
   value, multiplied by 128 (shifted left/up by 7 bits) for each byte that
   preceded it.
3. Varints representing values greater than 2^64-1 are invalid.

Several outstanding examples of very similar varint encodings exist:

| Implementation             | Format                         | Limits length?                           | Endianness | Bijective |
|----------------------------|--------------------------------|------------------------------------------|------------|-----------|
| [sqlite][sqlitevarint]     | base 128 with continuation bit | yes (9 bytes)                            | big        | no        |
| [protobuf][protobufvarint] | base 128 with continuation bit | no (10th byte uses only 1 bit)           | little     | no        |
| [git][gitvarint]           | base 128 with continuation bit | no (large values generally not relevant) | big        | yes       |
| bilrost                    | base 128 with continuation bit | yes (9 bytes)                            | little     | yes       |

[gitvarint]: https://git.kernel.org/pub/scm/git/git.git/tree/varint.c?h=v2.43.2

[protobufvarint]: https://protobuf.dev/programming-guides/encoding/#varints

[sqlitevarint]: https://www.sqlite.org/fileformat2.html#varint

##### Mathematics

Bilrost's varint representation is a base 128 [bijective numeration][bn] scheme
with a continuation bit. In such a numbering scheme, each possible values in a
given scheme is greater than each possible value with fewer digits. (Many people
are already unknowingly familiar with bijective numeration via the column names
in spreadsheet software: A, B, ... Y, Z, AA, AB, ...)

[bn]: https://en.wikipedia.org/wiki/Bijective_numeration

Classical bijective numerations have no zero digit, but represent zero with the
empty string. This doesn't work for us because we must always encode at least
one byte to avoid ambiguity. Consider instead:

* A base 128 bijective numeration,
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
    for byte_index, byte_value in enumerate(it):
        assert 0 <= byte_value < 256
        n += byte_value * (128**byte_index)
        if byte_value < 128 or byte_index == 8:
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

*The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD",
"SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL" in this section are to be
interpreted as described in [RFC 2119][rfc2119].*

[rfc2119]: https://www.ietf.org/rfc/rfc2119.txt

In general, whenever a decoded value represents a value that is outside the
domain of the type of the field it is being decoded into (for instance, when the
field type is `u16` but the value is a million, or when the field type is an
enumeration and there is no corresponding variant of the enumeration) the
decoding must be rejected with an error in any decoding mode.

Unsigned integers represented as varints are interpreted exactly. The varint
encoding of the number 10 has the same meaning in `u8`, `u16`, `u32`, and `u64`
field types.

Signed integers represented as varints are always [zig-zag encoded][zigzag],
with the sign of the number denoted in the least significant bit. Thus,
non-negative integers are translated to unsigned for encoding by doubling them,
and negative integers are translated by negating, then doubling, then
subtracting one.

[zigzag]: https://en.wikipedia.org/wiki/Variable-length_quantity#Zigzag_encoding

Booleans use the varint value 0 for `false`, and 1 for `true`.

Unsigned integers encoded in fixed-width must be encoded in little-endian
byte order; signed integers must likewise be encoded in little-endian byte
order, and must have a [two's complement][twos] representation.

[twos]: https://en.wikipedia.org/wiki/Two%27s_complement

Floating point numbers must be encoded in little-endian byte order, and must
have [IEEE 754 binary32/binary64][ieee754] standard representation. Floating
point numbers are encoded as four- and eight-byte fixed-width values.

Arrays, plain byte strings, and collections must be encoded in order, with their
lowest-indexed (first) bytes or items encoded first. For example, the
fixed-width encodings of the `u8` array `[1, 2, 3, 4]` and the 32 bit unsigned
integer `0x04030201` (67305985) are identical.

<details><summary>Demonstration of the above</summary>

```rust
use bilrost::Message;

#[derive(Message)]
struct Foo<T>(#[bilrost(encoding(fixed))] T);

// Both of these messages encode as the bytes `b'\x06\x01\x02\x03\x04'`
assert_eq!(
    Foo(0x04030201u32).encode_to_vec(),
    Foo([1u8, 2, 3, 4]).encode_to_vec(),
);
```

</details>

String values must always be valid UTF-8 text, containing the canonical encoding
for some sequence of Unicode codepoints. Codepoints with over-long encodings and
surrogate codepoints should be rejected with an error in any decoding mode, and
must be considered non-canonical. Bilrost does not impose any restrictions on
the ordering or presence of valid non-surrogate codepoints; it may be desirable
in an application to constrain text to a canonicalized form (such as
[NFC][uninormal]), but that should be considered outside the scope of Bilrost's
responsibilities of *encoding and decoding* and instead part of *validation,*
which is the responsibility of the application.

[uninormal]: https://en.wikipedia.org/wiki/Unicode_equivalence#Normal_forms

Collections of items (such as `Vec<String>`) encoded in the unpacked
representation consist of one field for each item. Collections encoded in the
packed representation consist of a single length-delimited value, containing
each item's value encoded one after the other. In expedient decoding mode,
decoding should succeed when expecting a packed representation but detecting an
unpacked representation, or vice versa (though the encoding must be considered
non-canonical). Detecting this situation is only possible when the values
themselves never have a length-delimited representation, in which case the
wire-type of the field can be used to distinguish the two cases.

Sets (collections of unique values) are encoded and decoded in exactly the same
form as non-unique collections. If a value in a set appears more than once when
decoding, the message must be rejected with an error in any decoding mode. The
items must be in [canonical order](#canonical-ordering) for the encoding to be
considered canonical.

Mappings are represented as a length-delimited value, containing alternately
encoded keys and values for each entry in the mapping. Keys must be distinct,
and if a map is found to have two equivalent keys the message must be rejected
with an error in any decoding mode. In distinguished decoding mode, the entries
in the mapping must be encoded in [canonical order](#canonical-ordering) for the
encoding to be considered canonical.

Any field whose value is [empty](#empty-values) should always be omitted from
the encoding. The presence of any field represented in the encoding with an
empty value must cause the encoding to be considered non-canonical.

Fields whose types do not encode into multiple fields must not occur more than
once. If they do, the message must be rejected with an error in any decoding
mode. This currently includes every type of field not encoded with an unpacked
representation.

Oneofs, sets of mutually exclusive fields, must not have conflicting values
present in the encoding. If they do, the message must be rejected with an error
in any decoding mode.

If a field whose tag that is not known/specified in the message is encountered
in expedient decoding mode, it should be ignored for purposes of decoding.

#### Distinguished constraints

In distinguished decoding mode, in addition to the above constraints on value
ordering in sets and mappings, all values must be represented in exactly the way
they would encode. If an [empty](#empty-values) value is found to be represented
in the encoding, the message is not canonical. (In the case of an optional
field, `Some(0)` is not considered empty, and is distinct from the always-empty
value `None`; this is the purpose of optional fields.)

Also in distinguished mode, if fields whose tags are not in the message's schema
are encountered the encoding can no longer be considered canonical.

#### Empty values

The type of each field of a Bilrost message has an "empty" value, which is never
represented as encoded data on the wire.

| Type                                                  | Empty value                        |
|-------------------------------------------------------|------------------------------------|
| boolean                                               | false                              |
| any integer                                           | 0                                  |
| any floating point number                             | exactly +0.0                       |
| fixed-size byte array                                 | all zeros                          |
| text string, byte string, collection, mapping, or set | containing zero bytes or items     |
| `Enumeration` type                                    | the variant represented by 0       |
| `Message`                                             | each field of the message is empty |
| `Oneof`                                               | `None` or the empty variant        |
| any optional value (`Option<T>`)                      | `None`                             |

The empty byte string is always a valid and canonical encoding of any Bilrost
message type, and represents the value of the message in which every field has
its empty value.

#### Canonical ordering

For supported non-message types, the following orderings are standardized:

| Type                                 | Standard ordering                                                                     |
|--------------------------------------|---------------------------------------------------------------------------------------|
| boolean                              | false, then true                                                                      |
| integer                              | ascending numeric value                                                               |
| text string, byte string, byte array | [lexicographically][lex] ascending, by bytes or UTF-8 bytes[^u8bytes]                 |
| collection (vec, set, etc.)          | lexicographically ascending, by nested values                                         |
| mapping                              | lexicographically ascending, by alternating key-then-value                            |
| floating point number                | [(not specified, nor recommended)](#floating-point-values-and-distinguished-decoding) |
| `Enumeration` types                  | [(not specified)](#canonical-order-and-distinguished-representation)                  |
| `Message` types                      | [(not specified)](#canonical-order-and-distinguished-representation)                  |
| `Option<T>`                          | (not applicable, cannot repeat)                                                       |
| `Oneof` types                        | (not applicable, not a single value, cannot repeat)                                   |

[lex]: https://en.wikipedia.org/wiki/Lexicographic_order

[^u8bytes]: Bytes are considered to be unsigned. The least-valued byte is the
nul byte `0x00`, and the greatest is `0xff`.

This standardization corresponds to the existing definitions of [`Ord`][ord] in
the Rust language for booleans, integers, strings, arrays/slices, ordered sets,
and ordered maps.

## `bilrost` vs. `prost`

`bilrost` is a direct fork of the `prost` crate, though it has been mostly
rewritten since then. Both libraries are designed for largely the same purpose,
but have different capabilities and have strengths in different situations.

[`prost`][p] is an implementation of [Protobuf][pb], and as a consequence it
brings many concerns and heavy tooling of that ecosystem with it, for better and
for worse. Protobuf messages are specified by a dedicated schema file, and the
code that implements those types is then usually automatically generated.
`prost` has tooling to do this via the "protoc" compiler; other implementations
variously do the same thing or reimplement complete parsers for that [DSL][dsl].

[dsl]: https://en.wikipedia.org/wiki/Domain-specific_language

`bilrost` by comparison is an implementation of a new encoding that isn't
compatible with Protobuf. If Protobuf isn't specifically required, consider the
[tradeoffs](#what-bilrost-and-the-library-wont-do) and [comparison](
#differences-from-protobuf) to the Protobuf encoding.

The code generated by `prost-build` is relatively messy and explicit, at least
when compared to handwritten code. This generated code in turn uses derive
macros to generate the more complex parts of the implementation, so the
generated code can in theory be committed and modified too, but it's not
significantly more flexible used this way.

`bilrost` refactors the encoding implementations to use trait-based dispatch
instead of explicit implementations that have to be selected for each field
type. This allows `bilrost` to have very broad type support without requiring
explicit annotations on most fields, and makes it very comfortable and easy to
use without any generated code other than the derive macros. (This same
trait-based dispatch could be back-ported to `prost` to make it easier to use,
but it might be a significant API break.)

`bilrost` has also implemented a couple requested features not yet available in
`prost`:

* message fields can be [ignored via attribute](#ignoring-fields)
* implementations are available for `no_std`-compatible hash maps, vecs that
  inline short values, `ByteString`, etc.
* `Message` and `DistinguishedMessage` traits are object-safe and provide [full
  functionality as trait objects](#using-dyn-with-object-safe-message-traits).
  At time of writing, `prost 0.12.3` has very little functionality exposed in an
  object-safe way; the only object-safe methods compute the encoded length of
  the message and clear its fields.

## Differences from Protobuf

The Bilrost encoding is heavily based upon that of Protobuf, with a small number
of key changes.

* Bilrost supports more types
* Bilrost is slightly more compact
* Bilrost has first-class support for distinguished canonical encoding
* Bilrost removes some mistake-prone choices
* Bilrost does not have a giant ecosystem

<details><summary>In greater detail</summary>

* The varint encoding is different: Bilrost varints are bijective (having only
  one possible representation per value) and have a shorter maximum length, as
  it doesn't make sense to extend the encoding beyond 64 bit integers.

  Despite Protobuf varints being nominally simpler, since they directly
  transpose the bits of the encoding into the final value, it is difficult to
  impossible to realize this simplicity as improved performance in reality;
  almost all of the cost on modern computing hardware is consumed by the
  fact that the values are a variable number of bytes in size.

  Protobuf varints are also subject to zero-extension, because they are not
  bijective. This is a recurring problem whenever attempts are made to guarantee
  canonical representation in Protobuf data, and requires extra care.
* Messages are only representable with their fields in ascending tag order,
  something Protobuf has declined to enforce or guarantee for decades and
  probably won't begin any time soon.

  Compliant Protobuf implementations allow several interesting operations by not
  guaranteeing or enforcing field order:
    * Unknown fields can be preserved as entirely opaque runs of bytes and
      concatenated to a message
    * Concatenating fields to a message has a *merge* semantic: singular fields'
      values are replaced (or merged, if they are messages), and repeated fields
      are appended to. This means that sometimes messages can be blindly
      concatenated with patches that override some of their fields.

  By guaranteeing field order in Bilrost, these (vanishingly rarely used or
  wanted) abilities are lost, but several powerful advantages are gained:
    * It is always trivially obvious when a field occurs more than once in a
      message when it shouldn't. No decisions need to be made or special checks
      performed to handle this case.
    * If desired (it probably isn't), it is even possible to enforce the
      required presence of particular fields in the encoding at run-time without
      maintaining presence data for those fields when decoding.

  Another hidden benefit of the obligate field ordering is that, because field
  tags are encoded as deltas, messages with very large numbers of fields are
  significantly smaller to encode. Protobuf field keys with tags above 15 always
  take multiple bytes to encode; in Bilrost, the only time a field key takes
  more than a single byte is when more than 31 tags have been skipped in a row.
* Fields' tags are less constrained. In Protobuf field tags are restricted to
  the range [1, 2^29-1]; in Bilrost we have made the decision to continue
  numbering them naturally from 1, but to otherwise allow any unsigned 32 bit
  integer as a tag number.
* Protobuf uses three bits in field keys for the wire type, and has six of these
  wire types allocated; two are used as data-less delimiting markers for
  "groups", which are a legacy and long-deprecated method of nesting data within
  messages.

  In nearly twenty years, the Protobuf authors have never found cause to
  populate the final two unallocated wire types, which gives us at least some
  measure of confidence that the four that Bilrost has borrowed are sufficient
  for practical use.

There are also a couple key changes to how values are interpreted in Bilrost,
informed by experience with Protobuf:

* Bilrost representations of signed integers are always zig-zag encoded. In
  Protobuf there are two different modes for signed integers: "int32" is always
  encoded like two's complement, and "sint32" is zig-zag encoded. In practice
  this is a tremendous footgun, because any negative integer always becomes
  *ten bytes* on the wire. Yes, even the 32 bit ones, because they are
  sign-extended all the way to 64 bits in case the field is to be widened in the
  future.
* Learning again from the footguns and mistakes of Protobuf (and C/C++ in
  general), Bilrost also enforces errors when values are out of range. Protobuf
  values will coerce to smaller types by truncation, and any nonzero varint will
  silently convert to the boolean value `true`. This is often surprising,
  bug-prone, and undesirable.
* `bilrost` makes special effort to preserve every bit of floating-point numbers
  when they are encoded and decoded. Whenever possible this should be matched by
  Bilrost libraries for other languages.
* Bilrost is much more permissive of nested values. Length-delimited values are
  permitted to be encoded in a "packed" representation, with warnings to the
  user; this allows nesting vecs within vecs, maps within maps, and more without
  creating explicit sub-message schemas for every single level of nesting.
* Bilrost has first-class mappings. Maps in Protobuf are a construct of unpacked
  repeated values that are nested sub-messages with keys and values in fields
  tagged 1 and 2, a situation whose official field types and APIs came long
  after it was already in production. Protobuf also to this day forbids byte
  strings as map keys, for unclear reasons possibly relating to the usage of
  nul-terminated C-strings as the representation of map keys in some
  implementations.

  Because Bilrost maps are packed into a single length-delimited value, they can
  freely have optional presence or be repeated or nested at will.

</details>

### Distinguished representation on the wire in `bilrost`

Leveraging the changes to varint representation and field order, Bilrost
standardizes easily-distinguishable canonical encodings for many message types.
Zero-extension of varints and unordered fields are the two main things that can
lead Protobuf encodings to vary for the same meaning, and most of what remains
involves enforcing that empty values are never encoded, packed/unpacked
collections have a matching representation, map keys are in sorted order, and
keeping track of whether any unknown fields exist in the encoding.

## Comparisons to other encodings

A very [incomplete][formats] comparison of various alternative encodings we
might consider.

[formats]: https://en.wikipedia.org/wiki/Comparison_of_data-serialization_formats

| Encoding             | Encoding complexity | Schemaless?           | Backwards/forwards compatible? | Human readable? | Canonical encodings?                          | Better than Bilrost                                                                   | Worse than Bilrost                                      |
|----------------------|---------------------|-----------------------|--------------------------------|-----------------|-----------------------------------------------|---------------------------------------------------------------------------------------|---------------------------------------------------------|
| Bilrost              | very low            | schemaful             | yes                            | no              | [yes](#distinguished-decoding)!               | 🌈                                                                                    | 🌈                                                      |
| [Protobuf][pb]       | almost as simple    | schemaful             | yes                            | no              | no                                            | big ecosystem, has a schema DSL                                                       | slightly less compact, more footguns, less type support |
| [ASN.1 DER][asn1]    | quite high          | schemaful             | yes                            | no              | [yes][asn1]                                   | highly standardized & validated canonicity                                            | painful to use & implement                              |
| [Cap'n Proto][capnp] | medium              | schemaful             | yes                            | no              | no                                            | very fast, supports zero-copy style decoding, schema DSL, lots of languages supported | less compact, heavily relies on generated types         |
| [Flatbuffers][flatb] | medium              | schemaful             | yes                            | no              | no                                            | very fast, supports zero-copy style decoding, schema DSL, lots of languages supported | less compact, heavily relies on generated types         |
| [rkyv][rkyv]         | ?                   | fixed to struct       | no                             | no              | ?                                             | extremely fast zero-copy archival encoding                                            | built for a very different purpose                      |
| [bincode][bincode]   | low                 | fixed to struct       | no                             | no              | ?                                             | faster, more compact                                                                  | not compatible when new fields are added                |
| [JSON][json]         | medium-low          | schemaless            | yes                            | yes             | [standardized][jsoncanon], might be supported | near-universal support, readability                                                   | less compact, more lossy, poor fit for many value types |
| [BSON][bson]         | medium              | schemaless            | yes                            | no              | no                                            | it's JSON but compact                                                                 | less compact, not canonical                             |
| [msgpack][msgpack]   | medium              | schemaless            | yes                            | no              | no                                            | it's JSON but compact                                                                 | less compact, not canonical                             |
| [CBOR][cbor]         | medium              | schemaless            | yes                            | no              | [yes][cborcanon]                              | standardized, it's JSON but compact                                                   | less compact                                            |
| [XML][xml]           | high                | philosophers disagree | yes                            | yes             | [apparently yes][xmlcanon]                    | you've heard of it, you know it, it's everywhere                                      | far less compact, an inelegant weapon from a bygone era |

[asn1]: https://www.itu.int/rec/T-REC-X.690/

[bincode]: https://docs.rs/bincode/latest/bincode/

[bson]: https://bsonspec.org/

[capnp]: https://capnproto.org/

[cbor]: https://cbor.io/

[cborcanon]: https://datatracker.ietf.org/doc/html/rfc8949#det-enc

[flatb]: https://flatbuffers.dev/

[json]: https://www.json.org/json-en.html

[jsoncanon]: https://datatracker.ietf.org/doc/html/rfc8785

[msgpack]: https://msgpack.org/index.html

[rkyv]: https://rkyv.org/

[xml]: https://www.w3.org/TR/xml/

[xmlcanon]: https://www.w3.org/TR/xml-c14n11/#XMLCanonicalization

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
thing in protobuf is an onerous task, and one I have almost never seen correctly
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
- Bilrost fields are also associated with a specific encoding, such as `general`
  or `fixed`, which may alter their representation. Purely trait-based dispatch
  will work poorly for this, especially when the values become nested within
  other data structures like maps and `Vec` and encodings may begin to look
  like `map<plainbytes, packed<fixed>>`.
- Bilrost messages must encode their fields in tag order, which may (in the case
  of `oneof` fields) vary depending on their value, and it's not clear how or if
  this could be solved in `serde`.
- Bilrost has both expedient and distinguished decoding modes, and promises that
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
