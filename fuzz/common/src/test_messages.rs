use bilrost::{Blob, Enumeration, Message, Oneof};
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use tinyvec::ArrayVec;

/// This proto includes every type of field in both singular and repeated
/// forms.
///
/// Also, crucially, all messages and enums in this file are eventually
/// submessages of this message.  So for example, a fuzz test of TestAllTypes
/// could trigger bugs that occur in any message type in this file.  We verify
/// this stays true in a unit test.
#[derive(Clone, Debug, PartialEq, Message)]
pub struct TestAllTypes {
    /// Singular
    pub sint32: i32,
    pub sint64: i64,
    pub uint32: u32,
    pub uint64: u64,
    #[bilrost(encoding(fixed))]
    pub ufixed32: u32,
    #[bilrost(encoding(fixed))]
    pub ufixed64: u64,
    #[bilrost(encoding(fixed))]
    pub sfixed32: i32,
    #[bilrost(encoding(fixed))]
    pub sfixed64: i64,
    pub float32: f32,
    pub float64: f64,
    pub bool: bool,
    pub string: String,
    #[bilrost(encoding((general, general, fixed)))]
    pub tuple: (u64, String, u32),
    #[bilrost(encoding(plainbytes))]
    pub bytes: Vec<u8>,
    pub blob: Blob,
    pub core_duration: core::time::Duration,
    pub core_systemtime: std::time::SystemTime,
    pub direct_message: test_message::NestedMessage,
    pub boxed_message: Box<test_message::NestedMessage>,
    #[bilrost(enumeration(test_message::NestedEnum))]
    pub helped_enum: u32,
    pub direct_enum: test_message::NestedEnum,
    pub map_varint_varint: BTreeMap<i32, i32>,
    #[bilrost(encoding(map<fixed, fixed>))]
    pub map_ufixed32_ufixed32: BTreeMap<u32, u32>,
    #[bilrost(encoding(map<fixed, fixed>))]
    pub map_ufixed64_ufixed64: BTreeMap<u64, u64>,
    #[bilrost(encoding(map<fixed, fixed>))]
    pub map_fixed32_fixed64: BTreeMap<i32, f64>,
    pub map_bool_bool: BTreeMap<bool, bool>,
    pub map_string_string: BTreeMap<String, String>,
    pub map_string_bytes: BTreeMap<String, Blob>,
    pub map_string_nested_message: BTreeMap<String, test_message::NestedMessage>,
    pub map_string_nested_enum: BTreeMap<String, test_message::NestedEnum>,
    /// Optional
    pub optional_sint32: Option<i32>,
    pub optional_sint64: Option<i64>,
    pub optional_uint32: Option<u32>,
    pub optional_uint64: Option<u64>,
    #[bilrost(encoding(fixed))]
    pub optional_ufixed32: Option<u32>,
    #[bilrost(encoding(fixed))]
    pub optional_ufixed64: Option<u64>,
    #[bilrost(encoding(fixed))]
    pub optional_sfixed32: Option<i32>,
    #[bilrost(encoding(fixed))]
    pub optional_sfixed64: Option<i64>,
    pub optional_float32: Option<f32>,
    pub optional_float64: Option<f64>,
    pub optional_bool: Option<bool>,
    pub optional_string: Option<String>,
    #[bilrost(encoding((general, general, fixed)))]
    pub optional_tuple: Option<(u64, String, u32)>,
    #[bilrost(encoding(plainbytes))]
    pub optional_bytes: Option<Vec<u8>>,
    pub optional_blob: Option<Blob>,
    pub optional_message: Option<test_message::NestedMessage>,
    pub optional_boxed_message: Option<Box<test_message::NestedMessage>>,
    #[bilrost(enumeration(test_message::NestedEnum))]
    pub optional_helped_enum: Option<u32>,
    pub optional_enum: Option<test_message::NestedEnum>,
    pub optional_map_fixed32_fixed64: Option<BTreeMap<i32, f64>>,
    pub optional_map_bool_bool: Option<BTreeMap<bool, bool>>,
    pub optional_map_string_string: Option<BTreeMap<String, String>>,
    pub optional_map_string_bytes: Option<BTreeMap<String, Blob>>,
    pub optional_map_string_nested_message: Option<BTreeMap<String, test_message::NestedMessage>>,
    pub optional_map_string_nested_enum: Option<BTreeMap<String, test_message::NestedEnum>>,
    /// Unpacked
    pub unpacked_sint32: Vec<i32>,
    pub unpacked_sint64: Vec<i64>,
    pub unpacked_uint32: Vec<u32>,
    pub unpacked_uint64: Vec<u64>,
    #[bilrost(encoding(fixed))]
    pub unpacked_ufixed32: Vec<u32>,
    #[bilrost(encoding(fixed))]
    pub unpacked_ufixed64: Vec<u64>,
    #[bilrost(encoding(fixed))]
    pub unpacked_sfixed32: Vec<i32>,
    #[bilrost(encoding(fixed))]
    pub unpacked_sfixed64: Vec<i64>,
    pub unpacked_float32: Vec<f32>,
    pub unpacked_float64: Vec<f64>,
    pub unpacked_bool: Vec<bool>,
    pub unpacked_string: Vec<String>,
    #[bilrost(encoding(unpacked<(general, general, fixed)>))]
    pub unpacked_tuple: Vec<(u64, String, u32)>,
    #[bilrost(encoding(unpacked<plainbytes>))]
    pub unpacked_bytes: Vec<Vec<u8>>,
    pub unpacked_blob: Vec<Blob>,
    pub unpacked_nested_message: Vec<test_message::NestedMessage>,
    #[bilrost(encoding(unpacked))]
    pub unpacked_varint_arr: [u64; 3],
    #[bilrost(encoding(unpacked<fixed>))]
    pub unpacked_fixed_arr: [u32; 3],
    #[bilrost(encoding(unpacked))]
    pub unpacked_varint_arrayvec: ArrayVec<[u64; 3]>,
    #[bilrost(encoding(unpacked<fixed>))]
    pub unpacked_fixed_arrayvec: ArrayVec<[u32; 3]>,
    /// Packed
    #[bilrost(encoding(packed))]
    pub packed_uint32: Vec<u32>,
    #[bilrost(encoding(packed))]
    pub packed_uint64: Vec<u64>,
    #[bilrost(encoding(packed<fixed>))]
    pub packed_ufixed32: Vec<u32>,
    #[bilrost(encoding(packed<fixed>))]
    pub packed_ufixed64: Vec<u64>,
    #[bilrost(encoding(packed))]
    pub packed_bool: Vec<bool>,
    #[bilrost(encoding(packed))]
    pub packed_string: Vec<String>,
    #[bilrost(encoding(packed<(general, general, fixed)>))]
    pub packed_tuple: Vec<(u64, String, u32)>,
    #[bilrost(encoding(packed))]
    pub packed_nested_enum: Vec<test_message::NestedEnum>,
    #[bilrost(encoding(packed))]
    pub packed_varint_arr: [u64; 3],
    #[bilrost(encoding(packed<fixed>))]
    pub packed_fixed_arr: [u32; 3],
    #[bilrost(encoding(packed))]
    pub packed_varint_arrayvec: ArrayVec<[u64; 3]>,
    #[bilrost(encoding(packed<fixed>))]
    pub packed_fixed_arrayvec: ArrayVec<[u32; 3]>,
    /// Set, unpacked
    pub unpacked_set_uint32: BTreeSet<u32>,
    pub unpacked_set_uint64: BTreeSet<u64>,
    #[bilrost(encoding(unpacked<fixed>))]
    pub unpacked_set_ufixed32: BTreeSet<u32>,
    #[bilrost(encoding(unpacked<fixed>))]
    pub unpacked_set_ufixed64: BTreeSet<u64>,
    pub unpacked_set_bool: BTreeSet<bool>,
    pub unpacked_set_string: BTreeSet<String>,
    pub unpacked_set_blob: BTreeSet<Blob>,
    pub unpacked_set_enum: BTreeSet<test_message::NestedEnum>,
    pub unpacked_set_map: BTreeSet<BTreeMap<bool, bool>>,
    /// Set, packed
    #[bilrost(encoding(packed))]
    pub packed_set_uint32: BTreeSet<u32>,
    #[bilrost(encoding(packed))]
    pub packed_set_uint64: BTreeSet<u64>,
    #[bilrost(encoding(packed<fixed>))]
    pub packed_set_ufixed32: BTreeSet<u32>,
    #[bilrost(encoding(packed<fixed>))]
    pub packed_set_ufixed64: BTreeSet<u64>,
    #[bilrost(encoding(packed))]
    pub packed_set_bool: BTreeSet<bool>,
    #[bilrost(encoding(packed))]
    pub packed_set_string: BTreeSet<String>,
    #[bilrost(encoding(packed))]
    pub packed_set_blob: BTreeSet<Blob>,
    #[bilrost(encoding(packed))]
    pub packed_set_enum: BTreeSet<test_message::NestedEnum>,
    #[bilrost(encoding(packed))]
    pub packed_set_map: BTreeSet<BTreeMap<bool, bool>>,
    /// Set, packed & optional
    #[bilrost(encoding(packed))]
    pub optional_packed_set_uint32: Option<BTreeSet<u32>>,
    #[bilrost(encoding(packed))]
    pub optional_packed_set_uint64: Option<BTreeSet<u64>>,
    #[bilrost(encoding(packed<fixed>))]
    pub optional_packed_set_ufixed32: Option<BTreeSet<u32>>,
    #[bilrost(encoding(packed<fixed>))]
    pub optional_packed_set_ufixed64: Option<BTreeSet<u64>>,
    #[bilrost(encoding(packed))]
    pub optional_packed_set_bool: Option<BTreeSet<bool>>,
    #[bilrost(encoding(packed))]
    pub optional_packed_set_string: Option<BTreeSet<String>>,
    #[bilrost(encoding(packed<plainbytes>))]
    pub optional_packed_set_bytes: Option<BTreeSet<Vec<u8>>>,
    #[bilrost(encoding(map<packed, general>))]
    pub optional_map_set_enum_f32: Option<BTreeMap<BTreeSet<test_message::NestedEnum>, f32>>,
    /// Recursive message
    #[bilrost(recurses)]
    pub recursive_message: Option<Box<TestAllTypes>>,
    /// Well-known types
    #[bilrost(tag = 301)]
    pub direct_duration: bilrost_types::Duration,
    pub direct_timestamp: bilrost_types::Timestamp,
    pub direct_struct: bilrost_types::StructValue,
    pub direct_value: bilrost_types::Value,
    pub optional_duration: Option<bilrost_types::Duration>,
    pub optional_timestamp: Option<bilrost_types::Timestamp>,
    pub optional_struct: Option<bilrost_types::StructValue>,
    pub optional_value: Option<bilrost_types::Value>,
    pub unpacked_duration: Vec<bilrost_types::Duration>,
    pub unpacked_timestamp: Vec<bilrost_types::Timestamp>,
    pub unpacked_struct: Vec<bilrost_types::StructValue>,
    pub unpacked_value: Vec<bilrost_types::Value>,
    pub unpacked_list_value: Vec<bilrost_types::ListValue>,
    #[bilrost(encoding(packed))]
    pub packed_duration: Vec<bilrost_types::Duration>,
    #[bilrost(encoding(packed))]
    pub packed_timestamp: Vec<bilrost_types::Timestamp>,
    #[bilrost(encoding(packed))]
    pub packed_struct: Vec<bilrost_types::StructValue>,
    #[bilrost(encoding(packed))]
    pub packed_value: Vec<bilrost_types::Value>,
    #[bilrost(encoding(packed))]
    pub packed_list_value: Vec<bilrost_types::ListValue>,
    /// Oneofs
    pub oneof_as_submessage: test_message::OneofField,
    #[bilrost(oneof(1001-1009))]
    pub nonempty_oneof_field: Option<test_message::NonEmptyOneofField>,
    #[bilrost(oneof(2001-2009))]
    pub oneof_field: test_message::OneofField,
}

/// Nested message and enum types in `TestAllTypes`.
pub mod test_message {
    use super::*;

    #[derive(Clone, Debug, PartialEq, Message)]
    pub struct NestedMessage {
        pub a: i32,
        #[bilrost(recurses)]
        pub corecursive: Option<Box<TestAllTypes>>,
    }
    #[derive(Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Enumeration)]
    #[repr(u32)]
    pub enum NestedEnum {
        #[default]
        Foo = 0,
        Bar = 1,
        Baz = 2,
        Max = u32::MAX,
    }
    #[derive(Clone, Debug, PartialEq, Oneof)]
    pub enum NonEmptyOneofField {
        #[bilrost(tag = 1001)]
        OneofUint32(u32),
        #[bilrost(tag = 1002)]
        OneofNestedMessage(Box<NestedMessage>),
        #[bilrost(tag = 1003)]
        OneofString(String),
        #[bilrost(tag = 1004, encoding(plainbytes))]
        OneofBytes(Vec<u8>),
        #[bilrost(tag = 1005)]
        OneofBool(bool),
        #[bilrost(tag = 1006)]
        OneofUint64(u64),
        #[bilrost(tag = 1007)]
        OneofFloat(f32),
        #[bilrost(tag = 1008)]
        OneofDouble(f64),
        #[bilrost(tag = 1009)]
        OneofEnum(NestedEnum),
    }

    #[derive(Clone, Debug, PartialEq, Oneof, Message)]
    pub enum OneofField {
        Empty,
        #[bilrost(tag = 2001)]
        OneofUint32(u32),
        #[bilrost(tag = 2002)]
        OneofNestedMessage(Box<NestedMessage>),
        #[bilrost(tag = 2003)]
        OneofString(String),
        #[bilrost(tag = 2004, encoding(plainbytes))]
        OneofBytes(Vec<u8>),
        #[bilrost(tag = 2005)]
        OneofBool(bool),
        #[bilrost(tag = 2006)]
        OneofUint64(u64),
        #[bilrost(tag = 2007)]
        OneofFloat(f32),
        #[bilrost(tag = 2008)]
        OneofDouble(f64),
        #[bilrost(tag = 2009)]
        OneofEnum(NestedEnum),
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Message)]
#[bilrost(distinguished)]
pub struct TestDistinguished {
    /// Singular
    #[bilrost(encoding(varint))]
    pub uint8: u8,
    pub uint16: u16,
    pub uint32: u32,
    pub uint64: u64,
    #[bilrost(encoding(fixed))]
    pub ufixed32: u32,
    #[bilrost(encoding(fixed))]
    pub ufixed64: u64,
    pub bool: bool,
    pub string: String,
    #[bilrost(encoding(plainbytes))]
    pub bytes: Vec<u8>,
    #[bilrost(encoding((general, general, fixed)))]
    pub tuple: (u64, String, u32),
    pub core_duration: core::time::Duration,
    pub direct_message: test_distinguished::NestedMessage,
    pub direct_enum: test_distinguished::NestedEnum,
    pub map_varint_varint: BTreeMap<i32, i32>,
    #[bilrost(encoding(map<fixed, fixed>))]
    pub map_ufixed32_ufixed32: BTreeMap<i32, i32>,
    pub map_bool_bool: BTreeMap<bool, bool>,
    pub map_u32_nested_message: BTreeMap<u32, test_distinguished::NestedMessage>,
    pub map_u32_nested_enum: BTreeMap<u32, test_distinguished::NestedEnum>,
    /// Optional
    pub optional_uint64: Option<u64>,
    #[bilrost(encoding(fixed))]
    pub optional_ufixed32: Option<u32>,
    pub optional_bool: Option<bool>,
    #[bilrost(encoding(plainbytes))]
    pub optional_bytes: Option<Vec<u8>>,
    #[bilrost(encoding((general, general, fixed)))]
    pub optional_tuple: Option<(u64, String, u32)>,
    pub optional_message: Option<test_distinguished::NestedMessage>,
    pub optional_boxed_message: Option<Box<test_distinguished::NestedMessage>>,
    pub optional_enum: Option<test_distinguished::NestedEnum>,
    pub optional_map_bool_bool: Option<BTreeMap<bool, bool>>,
    /// Unpacked
    pub unpacked_varint: Vec<u16>,
    #[bilrost(encoding(fixed))]
    pub unpacked_fixed: Vec<u32>,
    pub unpacked_bool: Vec<bool>,
    #[bilrost(encoding(plainbytes))]
    pub unpacked_string: Vec<Vec<u8>>,
    pub unpacked_nested_message: Vec<test_distinguished::NestedMessage>,
    #[bilrost(encoding(unpacked))]
    pub unpacked_varint_arr: [u64; 3],
    #[bilrost(encoding(unpacked<fixed>))]
    pub unpacked_fixed_arr: [u32; 3],
    #[bilrost(encoding(unpacked<plainbytes>))]
    pub unpacked_bytes_arr: [Vec<u8>; 3],
    #[bilrost(encoding(unpacked))]
    pub unpacked_varint_arrayvec: ArrayVec<[u64; 3]>,
    #[bilrost(encoding(unpacked<fixed>))]
    pub unpacked_fixed_arrayvec: ArrayVec<[u32; 3]>,
    #[bilrost(encoding(unpacked<plainbytes>))]
    pub unpacked_bytes_arrayvec: ArrayVec<[Vec<u8>; 3]>,
    /// Packed
    #[bilrost(encoding(packed))]
    pub packed_uint32: Vec<u32>,
    #[bilrost(encoding(packed<fixed>))]
    pub packed_ufixed32: Vec<u32>,
    #[bilrost(encoding(packed))]
    pub packed_bool: Vec<bool>,
    #[bilrost(encoding(packed<plainbytes>))]
    pub packed_bytes: Vec<Vec<u8>>,
    #[bilrost(encoding(packed<(general, general, fixed)>))]
    pub packed_tuple: Vec<(u64, String, u32)>,
    #[bilrost(encoding(packed))]
    pub packed_nested_enum: Vec<test_distinguished::NestedEnum>,
    #[bilrost(encoding(packed))]
    pub packed_varint_arr: [u64; 3],
    #[bilrost(encoding(packed<fixed>))]
    pub packed_fixed_arr: [u32; 3],
    #[bilrost(encoding(packed))]
    pub packed_varint_arrayvec: ArrayVec<[u64; 3]>,
    #[bilrost(encoding(packed<fixed>))]
    pub packed_fixed_arrayvec: ArrayVec<[u32; 3]>,
    #[bilrost(encoding(packed<plainbytes>))]
    pub packed_bytes_arrayvec: ArrayVec<[Vec<u8>; 3]>,
    /// Set, unpacked
    pub unpacked_set_uint32: BTreeSet<u32>,
    #[bilrost(encoding(unpacked<fixed>))]
    pub unpacked_set_ufixed32: BTreeSet<u32>,
    pub unpacked_set_bool: BTreeSet<bool>,
    #[bilrost(encoding(unpacked<plainbytes>))]
    pub unpacked_set_bytes: BTreeSet<Vec<u8>>,
    pub unpacked_set_enum: BTreeSet<test_distinguished::NestedEnum>,
    pub unpacked_set_map: BTreeSet<BTreeMap<bool, bool>>,
    /// Set, packed
    #[bilrost(encoding(packed))]
    pub packed_set_uint32: BTreeSet<u32>,
    #[bilrost(encoding(packed<fixed>))]
    pub packed_set_ufixed32: BTreeSet<u32>,
    #[bilrost(encoding(packed))]
    pub packed_set_bool: BTreeSet<bool>,
    #[bilrost(encoding(packed<plainbytes>))]
    pub packed_set_blob: BTreeSet<Vec<u8>>,
    #[bilrost(encoding(packed))]
    pub packed_set_enum: BTreeSet<test_distinguished::NestedEnum>,
    #[bilrost(encoding(packed))]
    pub packed_set_map: BTreeSet<BTreeMap<bool, bool>>,
    /// Set, packed & optional
    #[bilrost(encoding(packed))]
    pub optional_packed_set_uint32: Option<BTreeSet<u32>>,
    #[bilrost(encoding(packed))]
    pub optional_packed_set_map: Option<BTreeSet<BTreeMap<bool, bool>>>,
    /// Oneofs
    pub oneof_as_submessage: test_distinguished::OneofField,
    #[bilrost(oneof(101-106))]
    pub nonempty_oneof_field: Option<test_distinguished::NonEmptyOneofField>,
    #[bilrost(oneof(201-206))]
    pub oneof_field: test_distinguished::OneofField,
}

pub mod test_distinguished {
    use super::*;

    #[derive(Clone, Debug, PartialEq, Eq, Message)]
    #[bilrost(distinguished)]
    pub struct NestedMessage {
        pub a: u64,
        #[bilrost(recurses)]
        pub corecursive: Option<Box<TestDistinguished>>,
        #[bilrost(oneof(201-206))]
        pub oneof_field: OneofField,
    }

    pub use test_message::NestedEnum;

    #[derive(Clone, Debug, PartialEq, Eq, Oneof)]
    #[bilrost(distinguished)]
    pub enum NonEmptyOneofField {
        #[bilrost(tag = 101)]
        OneofUint32(u32),
        #[bilrost(tag = 102)]
        OneofNestedMessage(Box<NestedMessage>),
        #[bilrost(tag = 103, encoding(plainbytes))]
        OneofBytes(Vec<u8>),
        #[bilrost(tag = 104)]
        OneofBool(bool),
        #[bilrost(tag = 105)]
        OneofUint64(u64),
        #[bilrost(tag = 106)]
        OneofEnum(NestedEnum),
    }

    #[derive(Clone, Debug, PartialEq, Eq, Oneof, Message)]
    #[bilrost(distinguished)]
    pub enum OneofField {
        Empty,
        #[bilrost(tag = 201)]
        OneofUint32(u32),
        #[bilrost(tag = 202, recurses)]
        OneofNestedMessage(Box<NestedMessage>),
        #[bilrost(tag = 203, encoding(plainbytes))]
        OneofBytes(Vec<u8>),
        #[bilrost(tag = 204)]
        OneofBool(bool),
        #[bilrost(tag = 205)]
        OneofUint64(u64),
        #[bilrost(tag = 206)]
        OneofEnum(NestedEnum),
    }
}

#[derive(Debug, PartialEq, Message)]
pub struct TestTypeSupport {
    core_duration: core::time::Duration,
    chrono_naive_date: chrono::NaiveDate,
    chrono_naive_time: chrono::NaiveTime,
    chrono_naive_date_time: chrono::NaiveDateTime,
    chrono_fixed_offset: chrono::FixedOffset,
    chrono_date_time_utc: chrono::DateTime<chrono::Utc>,
    chrono_date_time_fixed: chrono::DateTime<chrono::FixedOffset>,
    chrono_time_delta: chrono::TimeDelta,
    time_date: time::Date,
    time_time: time::Time,
    time_primitivedatetime: time::PrimitiveDateTime,
    time_utcoffset: time::UtcOffset,
    time_offsetdatetime: time::OffsetDateTime,
    time_duration: time::Duration,

    std_systemtime: std::time::SystemTime,
}

#[derive(Debug, PartialEq, Eq, Message)]
#[bilrost(distinguished)]
pub struct TestTypeSupportDistinguished {
    core_duration: core::time::Duration,
    chrono_naive_date: chrono::NaiveDate,
    chrono_naive_time: chrono::NaiveTime,
    chrono_naive_date_time: chrono::NaiveDateTime,
    chrono_fixed_offset: chrono::FixedOffset,
    chrono_date_time_utc: chrono::DateTime<chrono::Utc>,
    chrono_date_time_fixed: chrono::DateTime<chrono::FixedOffset>,
    chrono_time_delta: chrono::TimeDelta,
    time_date: time::Date,
    time_time: time::Time,
    time_primitivedatetime: time::PrimitiveDateTime,
    time_utcoffset: time::UtcOffset,
    time_offsetdatetime: time::OffsetDateTime,
    time_duration: time::Duration,
}

#[derive(Debug, PartialEq, Eq, Message)]
#[bilrost(distinguished)]
pub struct TestTypeSupportBorrowable<'a> {
    str: Cow<'a, str>,
    #[bilrost(encoding(plainbytes))]
    bytes: Cow<'a, [u8]>,
    bstr: Cow<'a, bstr::BStr>,
    #[bilrost(encoding(plainbytes))]
    small_array: Cow<'a, [u8; 1]>,
    bigger_array: Cow<'a, [u8; 16]>,
}
