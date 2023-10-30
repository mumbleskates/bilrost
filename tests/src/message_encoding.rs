use bilrost::alloc::{borrow::ToOwned, string::String, vec, vec::Vec};
use bilrost::bytes::Bytes;
use bilrost::{Enumeration, Message, Oneof};

use crate::check_message;
use crate::check_serialize_equivalent;

#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, Message)]
pub struct RepeatedFloats {
    #[bilrost(float, tag = "11")]
    pub single_float: f32,
    #[bilrost(float, repeated, packed = "true", tag = "41")]
    pub repeated_float: Vec<f32>,
}

#[test]
fn check_repeated_floats() {
    check_message(&RepeatedFloats {
        single_float: 0.0,
        repeated_float: vec![
            0.1,
            340282300000000000000000000000000000000.0,
            0.000000000000000000000000000000000000011754944,
        ],
    });
}

#[test]
fn check_scalar_types() {
    check_message(&ScalarTypes::default());
}

/// A protobuf message which contains all scalar types.
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, Message)]
pub struct ScalarTypes {
    #[bilrost(int32, tag = "001")]
    pub int32: i32,
    #[bilrost(int64, tag = "002")]
    pub int64: i64,
    #[bilrost(uint32, tag = "003")]
    pub uint32: u32,
    #[bilrost(uint64, tag = "004")]
    pub uint64: u64,
    #[bilrost(sint32, tag = "005")]
    pub sint32: i32,
    #[bilrost(sint64, tag = "006")]
    pub sint64: i64,
    #[bilrost(fixed32, tag = "007")]
    pub fixed32: u32,
    #[bilrost(fixed64, tag = "008")]
    pub fixed64: u64,
    #[bilrost(sfixed32, tag = "009")]
    pub sfixed32: i32,
    #[bilrost(sfixed64, tag = "010")]
    pub sfixed64: i64,
    #[bilrost(float, tag = "011")]
    pub float: f32,
    #[bilrost(double, tag = "012")]
    pub double: f64,
    #[bilrost(bool, tag = "013")]
    pub _bool: bool,
    #[bilrost(string, tag = "014")]
    pub string: String,
    #[bilrost(bytes = "vec", tag = "015")]
    pub bytes_vec: Vec<u8>,
    #[bilrost(bytes = "bytes", tag = "016")]
    pub bytes_buf: Bytes,

    #[bilrost(int32, required, tag = "101")]
    pub required_int32: i32,
    #[bilrost(int64, required, tag = "102")]
    pub required_int64: i64,
    #[bilrost(uint32, required, tag = "103")]
    pub required_uint32: u32,
    #[bilrost(uint64, required, tag = "104")]
    pub required_uint64: u64,
    #[bilrost(sint32, required, tag = "105")]
    pub required_sint32: i32,
    #[bilrost(sint64, required, tag = "106")]
    pub required_sint64: i64,
    #[bilrost(fixed32, required, tag = "107")]
    pub required_fixed32: u32,
    #[bilrost(fixed64, required, tag = "108")]
    pub required_fixed64: u64,
    #[bilrost(sfixed32, required, tag = "109")]
    pub required_sfixed32: i32,
    #[bilrost(sfixed64, required, tag = "110")]
    pub required_sfixed64: i64,
    #[bilrost(float, required, tag = "111")]
    pub required_float: f32,
    #[bilrost(double, required, tag = "112")]
    pub required_double: f64,
    #[bilrost(bool, required, tag = "113")]
    pub required_bool: bool,
    #[bilrost(string, required, tag = "114")]
    pub required_string: String,
    #[bilrost(bytes = "vec", required, tag = "115")]
    pub required_bytes_vec: Vec<u8>,
    #[bilrost(bytes = "bytes", required, tag = "116")]
    pub required_bytes_buf: Bytes,

    #[bilrost(int32, optional, tag = "201")]
    pub optional_int32: Option<i32>,
    #[bilrost(int64, optional, tag = "202")]
    pub optional_int64: Option<i64>,
    #[bilrost(uint32, optional, tag = "203")]
    pub optional_uint32: Option<u32>,
    #[bilrost(uint64, optional, tag = "204")]
    pub optional_uint64: Option<u64>,
    #[bilrost(sint32, optional, tag = "205")]
    pub optional_sint32: Option<i32>,
    #[bilrost(sint64, optional, tag = "206")]
    pub optional_sint64: Option<i64>,

    #[bilrost(fixed32, optional, tag = "207")]
    pub optional_fixed32: Option<u32>,
    #[bilrost(fixed64, optional, tag = "208")]
    pub optional_fixed64: Option<u64>,
    #[bilrost(sfixed32, optional, tag = "209")]
    pub optional_sfixed32: Option<i32>,
    #[bilrost(sfixed64, optional, tag = "210")]
    pub optional_sfixed64: Option<i64>,
    #[bilrost(float, optional, tag = "211")]
    pub optional_float: Option<f32>,
    #[bilrost(double, optional, tag = "212")]
    pub optional_double: Option<f64>,
    #[bilrost(bool, optional, tag = "213")]
    pub optional_bool: Option<bool>,
    #[bilrost(string, optional, tag = "214")]
    pub optional_string: Option<String>,
    #[bilrost(bytes = "vec", optional, tag = "215")]
    pub optional_bytes_vec: Option<Vec<u8>>,
    #[bilrost(bytes = "bytes", optional, tag = "216")]
    pub optional_bytes_buf: Option<Bytes>,

    #[bilrost(int32, repeated, packed = "false", tag = "301")]
    pub repeated_int32: Vec<i32>,
    #[bilrost(int64, repeated, packed = "false", tag = "302")]
    pub repeated_int64: Vec<i64>,
    #[bilrost(uint32, repeated, packed = "false", tag = "303")]
    pub repeated_uint32: Vec<u32>,
    #[bilrost(uint64, repeated, packed = "false", tag = "304")]
    pub repeated_uint64: Vec<u64>,
    #[bilrost(sint32, repeated, packed = "false", tag = "305")]
    pub repeated_sint32: Vec<i32>,
    #[bilrost(sint64, repeated, packed = "false", tag = "306")]
    pub repeated_sint64: Vec<i64>,
    #[bilrost(fixed32, repeated, packed = "false", tag = "307")]
    pub repeated_fixed32: Vec<u32>,
    #[bilrost(fixed64, repeated, packed = "false", tag = "308")]
    pub repeated_fixed64: Vec<u64>,
    #[bilrost(sfixed32, repeated, packed = "false", tag = "309")]
    pub repeated_sfixed32: Vec<i32>,
    #[bilrost(sfixed64, repeated, packed = "false", tag = "310")]
    pub repeated_sfixed64: Vec<i64>,
    #[bilrost(float, repeated, packed = "false", tag = "311")]
    pub repeated_float: Vec<f32>,
    #[bilrost(double, repeated, packed = "false", tag = "312")]
    pub repeated_double: Vec<f64>,
    #[bilrost(bool, repeated, packed = "false", tag = "313")]
    pub repeated_bool: Vec<bool>,
    #[bilrost(string, repeated, packed = "false", tag = "315")]
    pub repeated_string: Vec<String>,
    #[bilrost(bytes = "vec", repeated, packed = "false", tag = "316")]
    pub repeated_bytes_vec: Vec<Vec<u8>>,
    #[bilrost(bytes = "bytes", repeated, packed = "false", tag = "317")]
    pub repeated_bytes_buf: Vec<Bytes>,

    #[bilrost(int32, repeated, tag = "401")]
    pub packed_int32: Vec<i32>,
    #[bilrost(int64, repeated, tag = "402")]
    pub packed_int64: Vec<i64>,
    #[bilrost(uint32, repeated, tag = "403")]
    pub packed_uint32: Vec<u32>,
    #[bilrost(uint64, repeated, tag = "404")]
    pub packed_uint64: Vec<u64>,
    #[bilrost(sint32, repeated, tag = "405")]
    pub packed_sint32: Vec<i32>,
    #[bilrost(sint64, repeated, tag = "406")]
    pub packed_sint64: Vec<i64>,
    #[bilrost(fixed32, repeated, tag = "407")]
    pub packed_fixed32: Vec<u32>,

    #[bilrost(fixed64, repeated, tag = "408")]
    pub packed_fixed64: Vec<u64>,
    #[bilrost(sfixed32, repeated, tag = "409")]
    pub packed_sfixed32: Vec<i32>,
    #[bilrost(sfixed64, repeated, tag = "410")]
    pub packed_sfixed64: Vec<i64>,
    #[bilrost(float, repeated, tag = "411")]
    pub packed_float: Vec<f32>,
    #[bilrost(double, repeated, tag = "412")]
    pub packed_double: Vec<f64>,
    #[bilrost(bool, repeated, tag = "413")]
    pub packed_bool: Vec<bool>,
    #[bilrost(string, repeated, tag = "415")]
    pub packed_string: Vec<String>,
    #[bilrost(bytes = "vec", repeated, tag = "416")]
    pub packed_bytes_vec: Vec<Vec<u8>>,
    #[bilrost(bytes = "bytes", repeated, tag = "417")]
    pub packed_bytes_buf: Vec<Bytes>,
}

#[test]
fn check_tags_inferred() {
    check_message(&TagsInferred::default());
    check_serialize_equivalent(&TagsInferred::default(), &TagsQualified::default());

    let tags_inferred = TagsInferred {
        one: true,
        two: Some(42),
        three: vec![0.0, 1.0, 1.0],
        skip_to_nine: "nine".to_owned(),
        ten: 0,
        eleven: ::alloc::collections::BTreeMap::new(),
        back_to_five: vec![1, 0, 1],
        six: Basic::default(),
    };
    check_message(&tags_inferred);

    let tags_qualified = TagsQualified {
        one: true,
        two: Some(42),
        three: vec![0.0, 1.0, 1.0],
        five: vec![1, 0, 1],
        six: Basic::default(),
        nine: "nine".to_owned(),
        ten: 0,
        eleven: ::alloc::collections::BTreeMap::new(),
    };
    check_serialize_equivalent(&tags_inferred, &tags_qualified);
}

#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, Message)]
pub struct TagsInferred {
    #[bilrost(bool)]
    pub one: bool,
    #[bilrost(int32, optional)]
    pub two: Option<i32>,
    #[bilrost(float, repeated)]
    pub three: Vec<f32>,

    #[bilrost(tag = "9", string, required)]
    pub skip_to_nine: String,
    #[bilrost(enumeration = "BasicEnumeration", default = "ONE")]
    pub ten: i32,
    #[bilrost(btree_map = "string, string")]
    pub eleven: ::alloc::collections::BTreeMap<String, String>,

    #[bilrost(tag = "5", bytes)]
    pub back_to_five: Vec<u8>,
    #[bilrost(message, required)]
    pub six: Basic,
}

#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, Message)]
pub struct TagsQualified {
    #[bilrost(tag = "1", bool)]
    pub one: bool,
    #[bilrost(tag = "2", int32, optional)]
    pub two: Option<i32>,
    #[bilrost(tag = "3", float, repeated)]
    pub three: Vec<f32>,

    #[bilrost(tag = "5", bytes)]
    pub five: Vec<u8>,
    #[bilrost(tag = "6", message, required)]
    pub six: Basic,

    #[bilrost(tag = "9", string, required)]
    pub nine: String,
    #[bilrost(tag = "10", enumeration = "BasicEnumeration", default = "ONE")]
    pub ten: i32,
    #[bilrost(tag = "11", btree_map = "string, string")]
    pub eleven: ::alloc::collections::BTreeMap<String, String>,
}

/// A bilrost message with default value.
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, Message)]
pub struct DefaultValues {
    #[bilrost(int32, tag = "1", default = "42")]
    pub int32: i32,

    #[bilrost(int32, optional, tag = "2", default = "88")]
    pub optional_int32: Option<i32>,

    #[bilrost(string, tag = "3", default = "forty two")]
    pub string: String,

    #[bilrost(bytes = "vec", tag = "7", default = "b\"foo\\x00bar\"")]
    pub bytes_vec: Vec<u8>,

    #[bilrost(bytes = "bytes", tag = "8", default = "b\"foo\\x00bar\"")]
    pub bytes_buf: Bytes,

    #[bilrost(enumeration = "BasicEnumeration", tag = "4", default = "ONE")]
    pub enumeration: i32,

    #[bilrost(enumeration = "BasicEnumeration", optional, tag = "5", default = "TWO")]
    pub optional_enumeration: Option<i32>,

    #[bilrost(enumeration = "BasicEnumeration", repeated, tag = "6")]
    pub repeated_enumeration: Vec<i32>,
}

#[test]
fn check_default_values() {
    let default = DefaultValues::default();
    assert_eq!(default.int32, 42);
    assert_eq!(default.optional_int32, None);
    assert_eq!(&default.string, "forty two");
    assert_eq!(&default.bytes_vec.as_ref(), b"foo\0bar");
    assert_eq!(&default.bytes_buf.as_ref(), b"foo\0bar");
    assert_eq!(default.enumeration, BasicEnumeration::ONE as i32);
    assert_eq!(default.optional_enumeration, None);
    assert_eq!(&default.repeated_enumeration, &[]);
    assert_eq!(0, default.encoded_len());
}

/// A protobuf enum.
#[allow(clippy::upper_case_acronyms, clippy::derive_partial_eq_without_eq)]
#[derive(Clone, Copy, Debug, PartialEq, Enumeration)]
pub enum BasicEnumeration {
    ZERO = 0,
    ONE = 1,
    TWO = 2,
    THREE = 3,
}

#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, Message)]
pub struct Basic {
    #[bilrost(int32, tag = "1")]
    pub int32: i32,

    #[bilrost(bool, repeated, packed = "false", tag = "2")]
    pub bools: Vec<bool>,

    #[bilrost(string, tag = "3")]
    pub string: String,

    #[bilrost(string, optional, tag = "4")]
    pub optional_string: Option<String>,

    #[bilrost(enumeration = "BasicEnumeration", tag = "5")]
    pub enumeration: i32,

    #[bilrost(map = "int32, enumeration(BasicEnumeration)", tag = "6")]
    #[cfg(feature = "std")]
    pub enumeration_map: ::std::collections::HashMap<i32, i32>,

    #[bilrost(hash_map = "string, string", tag = "7")]
    #[cfg(feature = "std")]
    pub string_map: ::std::collections::HashMap<String, String>,

    #[bilrost(btree_map = "int32, enumeration(BasicEnumeration)", tag = "10")]
    pub enumeration_btree_map: bilrost::alloc::collections::BTreeMap<i32, i32>,

    #[bilrost(btree_map = "string, string", tag = "11")]
    pub string_btree_map: bilrost::alloc::collections::BTreeMap<String, String>,

    #[bilrost(oneof = "BasicOneof", tags = "8, 9")]
    pub oneof: Option<BasicOneof>,

    #[bilrost(map = "string, bytes", tag = "12")]
    #[cfg(feature = "std")]
    pub bytes_map: ::std::collections::HashMap<String, Vec<u8>>,
}

#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, Message)]
pub struct Compound {
    #[bilrost(message, optional, tag = "1")]
    pub optional_message: Option<Basic>,

    #[bilrost(message, required, tag = "2")]
    pub required_message: Basic,

    #[bilrost(message, repeated, tag = "3")]
    pub repeated_message: Vec<Basic>,

    #[bilrost(map = "sint32, message", tag = "4")]
    #[cfg(feature = "std")]
    pub message_map: ::std::collections::HashMap<i32, Basic>,

    #[bilrost(btree_map = "sint32, message", tag = "5")]
    pub message_btree_map: bilrost::alloc::collections::BTreeMap<i32, Basic>,
}

#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, Oneof)]
pub enum BasicOneof {
    #[bilrost(int32, tag = "8")]
    Int(i32),
    #[bilrost(string, tag = "9")]
    String(String),
}
