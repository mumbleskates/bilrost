# *BILROST!*

`bilrost` is a [Protocol Buffers](https://developers.google.com/protocol-buffers/)-alike
fork for the [Rust Language](https://www.rust-lang.org/). It is a direct fork of
[`prost`](https://github.com/tokio-rs/prost). Like `prost`, `bilrost` can
generate simple, idiomatic Rust code with `derive` macros that serializes and
deserializes data similar to protocol buffers but in a slightly different,
incompatible way:

* All varints (including tag fields and lengths) use
  [bijective numeration](https://en.wikipedia.org/wiki/Bijective_numeration),
  which cannot be length-extended with trailing zeros the way protobuf varints
  can (and are slightly more compact, especially at the 64bit limit where they
  take up 9 bytes instead of 10).
* "Groups" are completely omitted as a wire-type, reducing the number of wire
  types from 6 to 4.
* These four wire types are packed into 2 bits instead of 3.
* Fields are obligated to be encoded in order, and fields' tag numbers are
  encoded as the difference from the previous field. This means that
  out-of-order fields cannot be represented in the encoding, and messages with
  many different fields consume relatively fewer bytes encoding their tags. The
  only time a field's tag is more than 1 byte is when more than 31 tag ids have
  been skipped.
* Field tags can be any `u32` value, without restriction.
* Signed varint types that aren't encoded with their sign bit in the LSB ("zig
  zag encoding") are omitted. There are no "gotcha" integer types whose negative
  values always take up the maximum amount of space.
* Enumerations are unsigned `u32`-typed, not `i32`.

Compared to other Protocol Buffers implementations, `bilrost`

* Generates simple, idiomatic, and readable Rust types by taking advantage of
  Rust `derive` attributes.
* Allows existing Rust types to be serialized and deserialized by adding
  attributes.
* Uses the [`bytes::{Buf, BufMut}`](https://github.com/carllerche/bytes)
  abstractions for serialization instead of `std::io::{Read, Write}`.
* Preserves unknown enum values during deserialization.
* Does not include support for runtime reflection or message descriptors.

### Messages

Given a simple message declaration:

```protobuf,ignore
// Sample message.
message Foo {
}
```

`bilrost` will generate the following Rust struct:

```rust,ignore
/// Sample message.
#[derive(Clone, Debug, PartialEq, Message)]
pub struct Foo {
}
```

### Fields

Fields in Protobuf messages are translated into Rust as public struct fields of the
corresponding type.

#### Scalar Values

Scalar value types are converted as follows:

| Protobuf Type | Rust Type |
| --- | --- |
| `float32` | `f32` |
| `float64` | `f64` |
| `uint32` | `u32` |
| `uint64` | `u64` |
| `sint32` | `i32` |
| `sint64` | `i64` |
| `ufixed32` | `u32` |
| `ufixed64` | `u64` |
| `sfixed32` | `i32` |
| `sfixed64` | `i64` |
| `bool` | `bool` |
| `string` | `String` |
| `bytes` | `Vec<u8>` |

#### Enumerations

All `.proto` enumeration types convert to the Rust `i32` type. Additionally,
each enumeration type gets a corresponding Rust `enum` type. For example, this
`proto` enum:

```protobuf,ignore
enum PhoneType {
  MOBILE = 0;
  HOME = 1;
  WORK = 2;
}
```

gets this corresponding Rust enum [^1]:

```rust,ignore
pub enum PhoneType {
    Mobile = 0,
    Home = 1,
    Work = 2,
}
```

[^1]: Annotations have been elided for clarity. See below for a full example.

You can convert a `PhoneType` value to an `i32` by doing:

```rust,ignore
PhoneType::Mobile as i32
```

The `#[derive(::bilrost::Enumeration)]` annotation added to the generated
`PhoneType` adds these associated functions to the type:

```rust,ignore
impl PhoneType {
    pub fn is_valid(value: i32) -> bool { ... }
    #[deprecated]
    pub fn from_i32(value: i32) -> Option<PhoneType> { ... }
}
```

It also adds an `impl TryFrom<i32> for PhoneType`, so you can convert an `i32` to its corresponding `PhoneType` value by doing,
for example:

```rust,ignore
let phone_type = 2i32;

match PhoneType::try_from(phone_type) {
    Ok(PhoneType::Mobile) => ...,
    Ok(PhoneType::Home) => ...,
    Ok(PhoneType::Work) => ...,
    Err(_) => ...,
}
```

Additionally, wherever a `proto` enum is used as a field in a `Message`, the
message will have 'accessor' methods to get/set the value of the field as the
Rust enum type. For instance, this proto `PhoneNumber` message that has a field
named `type` of type `PhoneType`:

```protobuf,ignore
message PhoneNumber {
  string number = 1;
  PhoneType type = 2;
}
```

will become the following Rust type [^2] with methods `type` and `set_type`:

```rust,ignore
pub struct PhoneNumber {
    pub number: String,
    pub r#type: i32, // the `r#` is needed because `type` is a Rust keyword
}

impl PhoneNumber {
    pub fn r#type(&self) -> PhoneType { ... }
    pub fn set_type(&mut self, value: PhoneType) { ... }
}
```

Note that the getter methods will return the Rust enum's default value if the
field has an invalid `i32` value.

The `enum` type isn't used directly as a field, because the Protobuf spec
mandates that enumerations values are 'open', and decoding unrecognized
enumeration values must be possible.

[^2]: Annotations have been elided for clarity. See below for a full example.

#### Field Modifiers

Protobuf scalar value and enumeration message fields can have a modifier
depending on the Protobuf version. Modifiers change the corresponding type of
the Rust field:

| `.proto` Version | Modifier | Rust Type |
| --- | --- | --- |
| `proto2` | `optional` | `Option<T>` |
| `proto2` | `required` | `T` |
| `proto3` | default | `T` for scalar types, `Option<T>` otherwise |
| `proto3` | `optional` | `Option<T>` |
| `proto2`/`proto3` | `repeated` | `Vec<T>` |

Note that in `proto3` the default representation for all user-defined message
types is `Option<T>`, and for scalar types just `T` (during decoding, a missing
value is populated by `T::default()`). If you need a witness of the presence of
a scalar type `T`, use the `optional` modifier to enforce an `Option<T>`
representation in the generated Rust struct.

#### Map Fields

Map fields are converted to a Rust `HashMap` with key and value type converted
from the Protobuf key and value types.

#### Message Fields

Message fields are converted to the corresponding struct type. The table of
field modifiers above applies to message fields, except that `proto3` message
fields without a modifier (the default) will be wrapped in an `Option`.
Typically message fields are unboxed. `bilrost` will automatically box a message
field if the field type and the parent type are recursively nested in order to
avoid an infinite sized struct.

#### Oneof Fields

Oneof fields convert to a Rust enum. Protobuf `oneof`s types are not named, so
`bilrost` uses the name of the `oneof` field for the resulting Rust enum, and
defines the enum in a module under the struct. For example, a `proto3` message
such as:

```protobuf,ignore
message Foo {
  oneof widget {
    int32 quux = 1;
    string bar = 2;
  }
}
```

generates the following Rust[^3]:

```rust,ignore
pub struct Foo {
    pub widget: Option<foo::Widget>,
}
pub mod foo {
    pub enum Widget {
        Quux(i32),
        Bar(String),
    }
}
```

`oneof` fields are always wrapped in an `Option`.

[^3]: Annotations have been elided for clarity. See below for a full example.

### Generated Code Example

Example `.proto` file:

```protobuf,ignore
syntax = "proto3";
package tutorial;

message Person {
  string name = 1;
  int32 id = 2;  // Unique ID number for this person.
  string email = 3;

  enum PhoneType {
    MOBILE = 0;
    HOME = 1;
    WORK = 2;
  }

  message PhoneNumber {
    string number = 1;
    PhoneType type = 2;
  }

  repeated PhoneNumber phones = 4;
}

// Our address book file is just one of these.
message AddressBook {
  repeated Person people = 1;
}
```

and the generated Rust code (`tutorial.rs`):

```rust,ignore
#[derive(Clone, PartialEq, ::bilrost::Message)]
pub struct Person {
    #[bilrost(string, tag="1")]
    pub name: ::bilrost::alloc::string::String,
    /// Unique ID number for this person.
    #[bilrost(sint32, tag="2")]
    pub id: i32,
    #[bilrost(string, tag="3")]
    pub email: ::bilrost::alloc::string::String,
    #[bilrost(message, repeated, tag="4")]
    pub phones: ::bilrost::alloc::vec::Vec<person::PhoneNumber>,
}
/// Nested message and enum types in `Person`.
pub mod person {
    #[derive(Clone, PartialEq, ::bilrost::Message)]
    pub struct PhoneNumber {
        #[bilrost(string, tag="1")]
        pub number: ::bilrost::alloc::string::String,
        #[bilrost(enumeration="PhoneType", tag="2")]
        pub r#type: i32,
    }
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::bilrost::Enumeration)]
    #[repr(i32)]
    pub enum PhoneType {
        Mobile = 0,
        Home = 1,
        Work = 2,
    }
}
/// Our address book file is just one of these.
#[derive(Clone, PartialEq, ::bilrost::Message)]
pub struct AddressBook {
    #[bilrost(message, repeated, tag="1")]
    pub people: ::bilrost::alloc::vec::Vec<Person>,
}
```

## Accessing the `protoc` `FileDescriptorSet`

The `bilrost_build::Config::file_descriptor_set_path` option can be used to emit a file descriptor set
during the build & code generation step. When used in conjunction with the `std::include_bytes`
macro and the `bilrost_types::FileDescriptorSet` type, applications and libraries using Bilrost can
implement introspection capabilities requiring details from the original `.proto` files.

## Using `bilrost` in a `no_std` Crate

`bilrost` is compatible with `no_std` crates. To enable `no_std` support, disable
the `std` features in `bilrost` and `bilrost-types`:

```ignore
[dependencies]
bilrost = { version = "0.6", default-features = false, features = ["bilrost-derive"] }
# Only necessary if using Protobuf well-known types:
bilrost-types = { version = "0.6", default-features = false }
```

Additionally, configure `bilrost-build` to output `BTreeMap`s instead of `HashMap`s
for all Protobuf `map` fields in your `build.rs`:

```rust,ignore
let mut config = bilrost_build::Config::new();
config.btree_map(&["."]);
```

When using edition 2015, it may be necessary to add an `extern crate core;`
directive to the crate which includes `bilrost`-generated code.

## Serializing Existing Types

`bilrost` uses a custom derive macro to handle encoding and decoding types, which
means that if your existing Rust type is compatible with Protobuf types, you can
serialize and deserialize it by adding the appropriate derive and field
annotations.

Currently the best documentation on adding annotations is to look at the
generated code examples above.

### Tag Inference for Existing Types

Bilrost automatically infers tags for the struct.

Fields are tagged sequentially in the order they
are specified, starting with `1`.

You may skip tags which have been reserved, or where there are gaps between
sequentially occurring tag values by specifying the tag number to skip to with
the `tag` attribute on the first field after the gap. The following fields will
be tagged sequentially starting from the next number.

```rust,ignore
use bilrost;
use bilrost::{Enumeration, Message};

#[derive(Clone, PartialEq, Message)]
struct Person {
    #[bilrost(string, tag = "1")]
    pub id: String, // tag=1
    // NOTE: Old "name" field has been removed
    // pub name: String, // tag=2 (Removed)
    #[bilrost(string, tag = "6")]
    pub given_name: String, // tag=6
    #[bilrost(string)]
    pub family_name: String, // tag=7
    #[bilrost(string)]
    pub formatted_name: String, // tag=8
    #[bilrost(uint32, tag = "3")]
    pub age: u32, // tag=3
    #[bilrost(uint32)]
    pub height: u32, // tag=4
    #[bilrost(enumeration = "Gender")]
    pub gender: i32, // tag=5
    // NOTE: Skip to less commonly occurring fields
    #[bilrost(string, tag = "16")]
    pub name_prefix: String, // tag=16  (eg. mr/mrs/ms)
    #[bilrost(string)]
    pub name_suffix: String, // tag=17  (eg. jr/esq)
    #[bilrost(string)]
    pub maiden_name: String, // tag=18
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Enumeration)]
pub enum Gender {
    Unknown = 0,
    Female = 1,
    Male = 2,
}
```

## Nix

The bilrost project maintains flakes support for local development. Once you have
nix and nix flakes setup you can just run `nix develop` to get a shell
configured with the required dependencies to compile the whole project.


## FAQ

1. **Could `bilrost` be implemented as a serializer for [Serde](https://serde.rs/)?**

  Probably not, however I would like to hear from a Serde expert on the matter.
  There are two complications with trying to serialize Protobuf messages with
  Serde:

  - Protobuf fields require a numbered tag, and currently there appears to be no
    mechanism suitable for this in `serde`.
  - The mapping of Protobuf type to Rust type is not 1-to-1. As a result,
    trait-based approaches to dispatching don't work very well. Example: four
    different Protobuf field types correspond to a Rust `Vec<i32>`: `repeated
    sint32`, `repeated sfixed32`, and their packed counterparts.

  But it is possible to place `serde` derive tags onto the generated types, so
  the same structure can support both `bilrost` and `Serde`.

2. **I get errors when trying to run `cargo test` on MacOS**

  If the errors are about missing `autoreconf` or similar, you can probably fix
  them by running

  ```ignore
  brew install automake
  brew install libtool
  ```

## License

`bilrost` is distributed under the terms of the Apache License (Version 2.0).

See [LICENSE](https://github.com/mumbleskates/bilrost/blob/master/LICENSE) for details.

Copyright 2022 Dan Burkert & Tokio Contributors
