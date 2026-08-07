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
#[bilrost(reserved_tags(166-299, 320-1000, 1013-1999, 2013..))]
pub struct TestAllTypes {
    /// Singular
    #[bilrost(tag(130), encoding(varint))]
    pub sint8: i8,
    #[bilrost(131)]
    pub sint16: i16,
    #[bilrost(1)]
    pub sint32: i32,
    #[bilrost(2)]
    pub sint64: i64,
    #[bilrost(132)]
    pub sintsize: isize,
    #[bilrost(tag(133), encoding(varint))]
    pub uint8: u8,
    #[bilrost(134)]
    pub uint16: u16,
    #[bilrost(3)]
    pub uint32: u32,
    #[bilrost(4)]
    pub uint64: u64,
    #[bilrost(135)]
    pub uintsize: usize,
    #[bilrost(tag(5), encoding(fixed))]
    pub ufixed32: u32,
    #[bilrost(tag(6), encoding(fixed))]
    pub ufixed64: u64,
    #[bilrost(tag(7), encoding(fixed))]
    pub sfixed32: i32,
    #[bilrost(tag(8), encoding(fixed))]
    pub sfixed64: i64,
    #[bilrost(9)]
    pub float32: f32,
    #[bilrost(10)]
    pub float64: f64,
    #[bilrost(11)]
    pub bool: bool,
    #[bilrost(12)]
    pub string: String,
    #[bilrost(115)]
    pub bytestring: bytestring::ByteString,
    #[bilrost(tag(13), encoding((general, general, fixed)))]
    pub tuple: (u64, String, u32),
    #[bilrost(tag(14), encoding(plainbytes))]
    pub bytes: Vec<u8>,
    #[bilrost(15)]
    pub blob: Blob,
    #[bilrost(116)]
    pub bytes_bytes: bytes::Bytes,
    #[bilrost(16)]
    pub core_duration: core::time::Duration,
    #[bilrost(17)]
    pub core_systemtime: std::time::SystemTime,
    #[bilrost(117)]
    pub range_varint: std::ops::Range<u64>,
    #[bilrost(118)]
    pub range_string: std::ops::Range<String>,
    #[bilrost(tag(119), encoding((fixed, fixed)))]
    pub range_fixed: std::ops::Range<u32>,
    #[bilrost(120)]
    pub range_inclusive_varint: std::ops::RangeInclusive<u64>,
    #[bilrost(121)]
    pub range_inclusive_string: std::ops::RangeInclusive<String>,
    #[bilrost(tag(122), encoding((fixed, fixed)))]
    pub range_inclusive_fixed: std::ops::RangeInclusive<u32>,
    #[bilrost(18)]
    pub direct_message: test_message::NestedMessage,
    #[bilrost(19)]
    pub boxed_message: Box<test_message::NestedMessage>,
    #[bilrost(tag(20), enumeration(test_message::NestedEnum))]
    pub helped_enum: u32,
    #[bilrost(21)]
    pub direct_enum: test_message::NestedEnum,
    #[bilrost(22)]
    pub map_varint_varint: BTreeMap<i32, i32>,
    #[bilrost(tag(23), encoding(map<fixed, fixed>))]
    pub map_ufixed32_ufixed32: BTreeMap<u32, u32>,
    #[bilrost(tag(24), encoding(map<fixed, fixed>))]
    pub map_ufixed64_ufixed64: BTreeMap<u64, u64>,
    #[bilrost(tag(25), encoding(map<fixed, fixed>))]
    pub map_fixed32_fixed64: BTreeMap<i32, f64>,
    #[bilrost(26)]
    pub map_bool_bool: BTreeMap<bool, bool>,
    #[bilrost(27)]
    pub map_string_string: BTreeMap<String, String>,
    #[bilrost(28)]
    pub map_string_bytes: BTreeMap<String, Blob>,
    #[bilrost(29)]
    pub map_string_nested_message: BTreeMap<String, test_message::NestedMessage>,
    #[bilrost(30)]
    pub map_string_nested_enum: BTreeMap<String, test_message::NestedEnum>,
    /// Optional
    #[bilrost(tag(136), encoding(varint))]
    pub optional_sint8: Option<i8>,
    #[bilrost(137)]
    pub optional_sint16: Option<i16>,
    #[bilrost(31)]
    pub optional_sint32: Option<i32>,
    #[bilrost(32)]
    pub optional_sint64: Option<i64>,
    #[bilrost(138)]
    pub optional_sintsize: Option<isize>,
    #[bilrost(tag(139), encoding(varint))]
    pub optional_uint8: Option<u8>,
    #[bilrost(140)]
    pub optional_uint16: Option<u16>,
    #[bilrost(33)]
    pub optional_uint32: Option<u32>,
    #[bilrost(34)]
    pub optional_uint64: Option<u64>,
    #[bilrost(141)]
    pub optional_uintsize: Option<usize>,
    #[bilrost(tag(35), encoding(fixed))]
    pub optional_ufixed32: Option<u32>,
    #[bilrost(tag(36), encoding(fixed))]
    pub optional_ufixed64: Option<u64>,
    #[bilrost(tag(37), encoding(fixed))]
    pub optional_sfixed32: Option<i32>,
    #[bilrost(tag(38), encoding(fixed))]
    pub optional_sfixed64: Option<i64>,
    #[bilrost(142)]
    pub optional_nonzerosint8: Option<core::num::NonZeroI8>,
    #[bilrost(143)]
    pub optional_nonzerosint16: Option<core::num::NonZeroI16>,
    #[bilrost(144)]
    pub optional_nonzerosint32: Option<core::num::NonZeroI32>,
    #[bilrost(145)]
    pub optional_nonzerosint64: Option<core::num::NonZeroI64>,
    #[bilrost(146)]
    pub optional_nonzerosintsize: Option<core::num::NonZeroIsize>,
    #[bilrost(147)]
    pub optional_nonzerouint8: Option<core::num::NonZeroU8>,
    #[bilrost(148)]
    pub optional_nonzerouint16: Option<core::num::NonZeroU16>,
    #[bilrost(149)]
    pub optional_nonzerouint32: Option<core::num::NonZeroU32>,
    #[bilrost(150)]
    pub optional_nonzerouint64: Option<core::num::NonZeroU64>,
    #[bilrost(151)]
    pub optional_nonzerouintsize: Option<core::num::NonZeroUsize>,
    #[bilrost(tag(152), encoding(fixed))]
    pub optional_nonzeroufixed32: Option<core::num::NonZeroU32>,
    #[bilrost(tag(153), encoding(fixed))]
    pub optional_nonzeroufixed64: Option<core::num::NonZeroU64>,
    #[bilrost(tag(154), encoding(fixed))]
    pub optional_nonzerosfixed32: Option<core::num::NonZeroI32>,
    #[bilrost(tag(155), encoding(fixed))]
    pub optional_nonzerosfixed64: Option<core::num::NonZeroI64>,
    #[bilrost(39)]
    pub optional_float32: Option<f32>,
    #[bilrost(40)]
    pub optional_float64: Option<f64>,
    #[bilrost(41)]
    pub optional_bool: Option<bool>,
    #[bilrost(42)]
    pub optional_string: Option<String>,
    #[bilrost(tag(43), encoding((general, general, fixed)))]
    pub optional_tuple: Option<(u64, String, u32)>,
    #[bilrost(tag(44), encoding(plainbytes))]
    pub optional_bytes: Option<Vec<u8>>,
    #[bilrost(45)]
    pub optional_blob: Option<Blob>,
    #[bilrost(123)]
    pub optional_range_varint: Option<std::ops::Range<u64>>,
    #[bilrost(124)]
    pub optional_range_string: Option<std::ops::Range<String>>,
    #[bilrost(tag(125), encoding((fixed, fixed)))]
    pub optional_range_fixed: Option<std::ops::Range<u32>>,
    #[bilrost(126)]
    pub optional_range_inclusive_varint: Option<std::ops::RangeInclusive<u64>>,
    #[bilrost(127)]
    pub optional_range_inclusive_string: Option<std::ops::RangeInclusive<String>>,
    #[bilrost(tag(128), encoding((fixed, fixed)))]
    pub optional_range_inclusive_fixed: Option<std::ops::RangeInclusive<u32>>,
    #[bilrost(46)]
    pub optional_message: Option<test_message::NestedMessage>,
    #[bilrost(47)]
    pub optional_boxed_message: Option<Box<test_message::NestedMessage>>,
    #[bilrost(tag(48), enumeration(test_message::NestedEnum))]
    pub optional_helped_enum: Option<u32>,
    #[bilrost(49)]
    pub optional_enum: Option<test_message::NestedEnum>,
    #[bilrost(50)]
    pub optional_map_fixed32_fixed64: Option<BTreeMap<i32, f64>>,
    #[bilrost(51)]
    pub optional_map_bool_bool: Option<BTreeMap<bool, bool>>,
    #[bilrost(52)]
    pub optional_map_string_string: Option<BTreeMap<String, String>>,
    #[bilrost(53)]
    pub optional_map_string_bytes: Option<BTreeMap<String, Blob>>,
    #[bilrost(54)]
    pub optional_map_string_nested_message: Option<BTreeMap<String, test_message::NestedMessage>>,
    #[bilrost(55)]
    pub optional_map_string_nested_enum: Option<BTreeMap<String, test_message::NestedEnum>>,
    /// Unpacked
    #[bilrost(56)]
    pub unpacked_sint32: Vec<i32>,
    #[bilrost(57)]
    pub unpacked_sint64: Vec<i64>,
    #[bilrost(58)]
    pub unpacked_uint32: Vec<u32>,
    #[bilrost(59)]
    pub unpacked_uint64: Vec<u64>,
    #[bilrost(156)]
    pub unpacked_nonzerouint32: Vec<core::num::NonZeroU32>,
    #[bilrost(tag(60), encoding(unpacked<fixed>))]
    pub unpacked_ufixed32: Vec<u32>,
    #[bilrost(tag(61), encoding(unpacked<fixed>))]
    pub unpacked_ufixed64: Vec<u64>,
    #[bilrost(tag(62), encoding(unpacked<fixed>))]
    pub unpacked_sfixed32: Vec<i32>,
    #[bilrost(tag(63), encoding(unpacked<fixed>))]
    pub unpacked_sfixed64: Vec<i64>,
    #[bilrost(tag(157), encoding(unpacked<fixed>))]
    pub unpacked_nonzeroufixed32: Vec<core::num::NonZeroU32>,
    #[bilrost(64)]
    pub unpacked_float32: Vec<f32>,
    #[bilrost(65)]
    pub unpacked_float64: Vec<f64>,
    #[bilrost(66)]
    pub unpacked_bool: Vec<bool>,
    #[bilrost(67)]
    pub unpacked_string: Vec<String>,
    #[bilrost(tag(68), encoding(unpacked<(general, general, fixed)>))]
    pub unpacked_tuple: Vec<(u64, String, u32)>,
    #[bilrost(tag(69), encoding(unpacked<plainbytes>))]
    pub unpacked_bytes: Vec<Vec<u8>>,
    #[bilrost(70)]
    pub unpacked_blob: Vec<Blob>,
    #[bilrost(71)]
    pub unpacked_nested_message: Vec<test_message::NestedMessage>,
    #[bilrost(tag(72), encoding(unpacked))]
    pub unpacked_varint_arr: [u64; 3],
    #[bilrost(tag(73), encoding(unpacked<fixed>))]
    pub unpacked_fixed_arr: [u32; 3],
    #[bilrost(tag(74), encoding(unpacked))]
    pub unpacked_varint_arrayvec: ArrayVec<[u64; 3]>,
    #[bilrost(tag(75), encoding(unpacked<fixed>))]
    pub unpacked_fixed_arrayvec: ArrayVec<[u32; 3]>,
    /// Packed
    #[bilrost(tag(76), encoding(packed))]
    pub packed_uint32: Vec<u32>,
    #[bilrost(tag(77), encoding(packed))]
    pub packed_uint64: Vec<u64>,
    #[bilrost(tag(158), encoding(packed))]
    pub packed_nonzerouint32: Vec<core::num::NonZeroU32>,
    #[bilrost(tag(78), encoding(packed<fixed>))]
    pub packed_ufixed32: Vec<u32>,
    #[bilrost(tag(79), encoding(packed<fixed>))]
    pub packed_ufixed64: Vec<u64>,
    #[bilrost(tag(159), encoding(packed<fixed>))]
    pub packed_nonzeroufixed32: Vec<core::num::NonZeroU32>,
    #[bilrost(tag(80), encoding(packed))]
    pub packed_bool: Vec<bool>,
    #[bilrost(tag(81), encoding(packed))]
    pub packed_string: Vec<String>,
    #[bilrost(tag(82), encoding(packed<(general, general, fixed)>))]
    pub packed_tuple: Vec<(u64, String, u32)>,
    #[bilrost(tag(83), encoding(packed))]
    pub packed_nested_enum: Vec<test_message::NestedEnum>,
    #[bilrost(tag(84), encoding(packed))]
    pub packed_varint_arr: [u64; 3],
    #[bilrost(tag(85), encoding(packed<fixed>))]
    pub packed_fixed_arr: [u32; 3],
    #[bilrost(tag(86), encoding(packed))]
    pub packed_varint_arrayvec: ArrayVec<[u64; 3]>,
    #[bilrost(tag(87), encoding(packed<fixed>))]
    pub packed_fixed_arrayvec: ArrayVec<[u32; 3]>,
    /// Set, unpacked
    #[bilrost(88)]
    pub unpacked_set_uint32: BTreeSet<u32>,
    #[bilrost(89)]
    pub unpacked_set_uint64: BTreeSet<u64>,
    #[bilrost(tag(160), encoding(packed))]
    pub unpacked_set_nonzerouint32: BTreeSet<core::num::NonZeroU32>,
    #[bilrost(tag(90), encoding(unpacked<fixed>))]
    pub unpacked_set_ufixed32: BTreeSet<u32>,
    #[bilrost(tag(91), encoding(unpacked<fixed>))]
    pub unpacked_set_ufixed64: BTreeSet<u64>,
    #[bilrost(tag(161), encoding(packed<fixed>))]
    pub unpacked_set_nonzeroufixed32: BTreeSet<core::num::NonZeroU32>,
    #[bilrost(92)]
    pub unpacked_set_bool: BTreeSet<bool>,
    #[bilrost(93)]
    pub unpacked_set_string: BTreeSet<String>,
    #[bilrost(94)]
    pub unpacked_set_blob: BTreeSet<Blob>,
    #[bilrost(95)]
    pub unpacked_set_enum: BTreeSet<test_message::NestedEnum>,
    #[bilrost(96)]
    pub unpacked_set_map: BTreeSet<BTreeMap<bool, bool>>,
    /// Set, packed
    #[bilrost(tag(97), encoding(packed))]
    pub packed_set_uint32: BTreeSet<u32>,
    #[bilrost(tag(98), encoding(packed))]
    pub packed_set_uint64: BTreeSet<u64>,
    #[bilrost(tag(162), encoding(packed))]
    pub packed_set_nonzerouint32: BTreeSet<core::num::NonZeroU32>,
    #[bilrost(tag(99), encoding(packed<fixed>))]
    pub packed_set_ufixed32: BTreeSet<u32>,
    #[bilrost(tag(100), encoding(packed<fixed>))]
    pub packed_set_ufixed64: BTreeSet<u64>,
    #[bilrost(tag(163), encoding(packed<fixed>))]
    pub packed_set_nonzeroufixed32: BTreeSet<core::num::NonZeroU32>,
    #[bilrost(tag(101), encoding(packed))]
    pub packed_set_bool: BTreeSet<bool>,
    #[bilrost(tag(102), encoding(packed))]
    pub packed_set_string: BTreeSet<String>,
    #[bilrost(tag(103), encoding(packed))]
    pub packed_set_blob: BTreeSet<Blob>,
    #[bilrost(tag(104), encoding(packed))]
    pub packed_set_enum: BTreeSet<test_message::NestedEnum>,
    #[bilrost(tag(105), encoding(packed))]
    pub packed_set_map: BTreeSet<BTreeMap<bool, bool>>,
    /// Set, packed & optional
    #[bilrost(tag(106), encoding(packed))]
    pub optional_packed_set_uint32: Option<BTreeSet<u32>>,
    #[bilrost(tag(107), encoding(packed))]
    pub optional_packed_set_uint64: Option<BTreeSet<u64>>,
    #[bilrost(tag(164), encoding(packed))]
    pub optional_packed_set_nonzerouint32: Option<BTreeSet<core::num::NonZeroU32>>,
    #[bilrost(tag(108), encoding(packed<fixed>))]
    pub optional_packed_set_ufixed32: Option<BTreeSet<u32>>,
    #[bilrost(tag(109), encoding(packed<fixed>))]
    pub optional_packed_set_ufixed64: Option<BTreeSet<u64>>,
    #[bilrost(tag(165), encoding(packed<fixed>))]
    pub optional_packed_set_nonzeroufixed32: Option<BTreeSet<core::num::NonZeroU32>>,
    #[bilrost(tag(110), encoding(packed))]
    pub optional_packed_set_bool: Option<BTreeSet<bool>>,
    #[bilrost(tag(111), encoding(packed))]
    pub optional_packed_set_string: Option<BTreeSet<String>>,
    #[bilrost(tag(112), encoding(packed<plainbytes>))]
    pub optional_packed_set_bytes: Option<BTreeSet<Vec<u8>>>,
    #[bilrost(113)]
    pub optional_map_set_enum_f32: Option<BTreeMap<BTreeSet<test_message::NestedEnum>, f32>>,
    /// Recursive message
    #[bilrost(tag(114), recurses)]
    pub recursive_message: Option<Box<TestAllTypes>>,
    /// Well-known types
    #[bilrost(301)]
    pub direct_duration: bilrost_types::Duration,
    #[bilrost(302)]
    pub direct_timestamp: bilrost_types::Timestamp,
    #[bilrost(303)]
    pub direct_struct: bilrost_types::StructValue,
    #[bilrost(304)]
    pub direct_value: bilrost_types::Value,
    #[bilrost(305)]
    pub optional_duration: Option<bilrost_types::Duration>,
    #[bilrost(306)]
    pub optional_timestamp: Option<bilrost_types::Timestamp>,
    #[bilrost(307)]
    pub optional_struct: Option<bilrost_types::StructValue>,
    #[bilrost(308)]
    pub optional_value: Option<bilrost_types::Value>,
    #[bilrost(309)]
    pub unpacked_duration: Vec<bilrost_types::Duration>,
    #[bilrost(310)]
    pub unpacked_timestamp: Vec<bilrost_types::Timestamp>,
    #[bilrost(311)]
    pub unpacked_struct: Vec<bilrost_types::StructValue>,
    #[bilrost(312)]
    pub unpacked_value: Vec<bilrost_types::Value>,
    #[bilrost(313)]
    pub unpacked_list_value: Vec<bilrost_types::ListValue>,
    #[bilrost(tag(314), encoding(packed))]
    pub packed_duration: Vec<bilrost_types::Duration>,
    #[bilrost(tag(315), encoding(packed))]
    pub packed_timestamp: Vec<bilrost_types::Timestamp>,
    #[bilrost(tag(316), encoding(packed))]
    pub packed_struct: Vec<bilrost_types::StructValue>,
    #[bilrost(tag(317), encoding(packed))]
    pub packed_value: Vec<bilrost_types::Value>,
    #[bilrost(tag(318), encoding(packed))]
    pub packed_list_value: Vec<bilrost_types::ListValue>,
    /// Oneofs
    #[bilrost(319)]
    pub oneof_as_submessage: test_message::OneofField,
    #[bilrost(oneof(1001-1012))]
    pub nonempty_oneof_field: Option<test_message::NonEmptyOneofField>,
    #[bilrost(oneof(2001-2012))]
    pub oneof_field: test_message::OneofField,
}

/// Nested message and enum types in `TestAllTypes`.
pub mod test_message {
    use super::*;

    #[derive(Clone, Debug, PartialEq, Message)]
    pub struct NestedMessage {
        #[bilrost(1)]
        pub a: i32,
        #[bilrost(tag(2), recurses)]
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
        #[bilrost(tag = 1010, message)]
        OneofBracedMessage {
            #[bilrost(1)]
            x: u64,
            #[bilrost(2)]
            y: u64,
            #[bilrost(3)]
            z: u64,
        },
        #[bilrost(tag = 1011, message)]
        OneofTupleMessage(u64, u64, u64),
        #[bilrost(tag = 1012, message)]
        OneofUnit,
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
        #[bilrost(tag = 2010, message)]
        OneofBracedMessage {
            #[bilrost(1)]
            x: u64,
            #[bilrost(2)]
            y: u64,
            #[bilrost(3)]
            z: u64,
        },
        #[bilrost(tag = 2011, message)]
        OneofTupleMessage(u64, u64, u64),
        #[bilrost(tag = 2012, message)]
        OneofUnit,
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Message)]
#[bilrost(distinguished, reserved_tags(87-100, 110-199, 210..))]
pub struct TestDistinguished {
    /// Singular
    #[bilrost(tag(1), encoding(varint))]
    pub uint8: u8,
    #[bilrost(2)]
    pub uint16: u16,
    #[bilrost(3)]
    pub uint32: u32,
    #[bilrost(4)]
    pub uint64: u64,
    #[bilrost(tag(5), encoding(fixed))]
    pub ufixed32: u32,
    #[bilrost(tag(6), encoding(fixed))]
    pub ufixed64: u64,
    #[bilrost(79)]
    pub usize: usize,
    #[bilrost(80)]
    pub optional_nonzerouint8: Option<core::num::NonZeroU8>,
    #[bilrost(81)]
    pub optional_nonzerouint16: Option<core::num::NonZeroU16>,
    #[bilrost(82)]
    pub optional_nonzerouint32: Option<core::num::NonZeroU32>,
    #[bilrost(83)]
    pub optional_nonzerouint64: Option<core::num::NonZeroU64>,
    #[bilrost(tag(84), encoding(fixed))]
    pub optional_nonzeroufixed32: Option<core::num::NonZeroU32>,
    #[bilrost(tag(85), encoding(fixed))]
    pub optional_nonzeroufixed64: Option<core::num::NonZeroU64>,
    #[bilrost(86)]
    pub optional_nonzerousize: Option<core::num::NonZeroUsize>,
    #[bilrost(7)]
    pub bool: bool,
    #[bilrost(8)]
    pub string: String,
    #[bilrost(tag(9), encoding(plainbytes))]
    pub bytes: Vec<u8>,
    #[bilrost(64)]
    pub blob: Blob,
    #[bilrost(65)]
    pub bytes_bytes: bytes::Bytes,
    #[bilrost(66)]
    pub bytestring: bytestring::ByteString,
    #[bilrost(tag(10), encoding((general, general, fixed)))]
    pub tuple: (u64, String, u32),
    #[bilrost(11)]
    pub core_duration: core::time::Duration,
    #[bilrost(67)]
    pub range_varint: std::ops::Range<u64>,
    #[bilrost(68)]
    pub range_string: std::ops::Range<String>,
    #[bilrost(tag(69), encoding((fixed, fixed)))]
    pub range_fixed: std::ops::Range<u32>,
    #[bilrost(70)]
    pub range_inclusive_varint: std::ops::RangeInclusive<u64>,
    #[bilrost(71)]
    pub range_inclusive_string: std::ops::RangeInclusive<String>,
    #[bilrost(tag(72), encoding((fixed, fixed)))]
    pub range_inclusive_fixed: std::ops::RangeInclusive<u32>,
    #[bilrost(12)]
    pub direct_message: test_distinguished::NestedMessage,
    #[bilrost(13)]
    pub direct_enum: test_distinguished::NestedEnum,
    #[bilrost(14)]
    pub map_varint_varint: BTreeMap<i32, i32>,
    #[bilrost(tag(15), encoding(map<fixed, fixed>))]
    pub map_ufixed32_ufixed32: BTreeMap<i32, i32>,
    #[bilrost(16)]
    pub map_bool_bool: BTreeMap<bool, bool>,
    #[bilrost(17)]
    pub map_u32_nested_message: BTreeMap<u32, test_distinguished::NestedMessage>,
    #[bilrost(18)]
    pub map_u32_nested_enum: BTreeMap<u32, test_distinguished::NestedEnum>,
    /// Optional
    #[bilrost(19)]
    pub optional_uint64: Option<u64>,
    #[bilrost(tag(20), encoding(fixed))]
    pub optional_ufixed32: Option<u32>,
    #[bilrost(21)]
    pub optional_bool: Option<bool>,
    #[bilrost(tag(22), encoding(plainbytes))]
    pub optional_bytes: Option<Vec<u8>>,
    #[bilrost(tag(23), encoding((general, general, fixed)))]
    pub optional_tuple: Option<(u64, String, u32)>,
    #[bilrost(73)]
    pub optional_range_varint: Option<std::ops::Range<u64>>,
    #[bilrost(74)]
    pub optional_range_string: Option<std::ops::Range<String>>,
    #[bilrost(tag(75), encoding((fixed, fixed)))]
    pub optional_range_fixed: Option<std::ops::Range<u32>>,
    #[bilrost(76)]
    pub optional_range_inclusive_varint: Option<std::ops::RangeInclusive<u64>>,
    #[bilrost(77)]
    pub optional_range_inclusive_string: Option<std::ops::RangeInclusive<String>>,
    #[bilrost(tag(78), encoding((fixed, fixed)))]
    pub optional_range_inclusive_fixed: Option<std::ops::RangeInclusive<u32>>,
    #[bilrost(24)]
    pub optional_message: Option<test_distinguished::NestedMessage>,
    #[bilrost(25)]
    pub optional_boxed_message: Option<Box<test_distinguished::NestedMessage>>,
    #[bilrost(26)]
    pub optional_enum: Option<test_distinguished::NestedEnum>,
    #[bilrost(27)]
    pub optional_map_bool_bool: Option<BTreeMap<bool, bool>>,
    /// Unpacked
    #[bilrost(28)]
    pub unpacked_varint: Vec<u16>,
    #[bilrost(tag(29), encoding(unpacked<fixed>))]
    pub unpacked_fixed: Vec<u32>,
    #[bilrost(30)]
    pub unpacked_bool: Vec<bool>,
    #[bilrost(tag(31), encoding(plainbytes))]
    pub unpacked_string: Vec<Vec<u8>>,
    #[bilrost(32)]
    pub unpacked_nested_message: Vec<test_distinguished::NestedMessage>,
    #[bilrost(tag(33), encoding(unpacked))]
    pub unpacked_varint_arr: [u64; 3],
    #[bilrost(tag(34), encoding(unpacked<fixed>))]
    pub unpacked_fixed_arr: [u32; 3],
    #[bilrost(tag(35), encoding(unpacked<plainbytes>))]
    pub unpacked_bytes_arr: [Vec<u8>; 3],
    #[bilrost(tag(36), encoding(unpacked))]
    pub unpacked_varint_arrayvec: ArrayVec<[u64; 3]>,
    #[bilrost(tag(37), encoding(unpacked<fixed>))]
    pub unpacked_fixed_arrayvec: ArrayVec<[u32; 3]>,
    #[bilrost(tag(38), encoding(unpacked<plainbytes>))]
    pub unpacked_bytes_arrayvec: ArrayVec<[Vec<u8>; 3]>,
    /// Packed
    #[bilrost(tag(39), encoding(packed))]
    pub packed_uint32: Vec<u32>,
    #[bilrost(tag(40), encoding(packed<fixed>))]
    pub packed_ufixed32: Vec<u32>,
    #[bilrost(tag(41), encoding(packed))]
    pub packed_bool: Vec<bool>,
    #[bilrost(tag(42), encoding(packed<plainbytes>))]
    pub packed_bytes: Vec<Vec<u8>>,
    #[bilrost(tag(43), encoding(packed<(general, general, fixed)>))]
    pub packed_tuple: Vec<(u64, String, u32)>,
    #[bilrost(tag(44), encoding(packed))]
    pub packed_nested_enum: Vec<test_distinguished::NestedEnum>,
    #[bilrost(tag(45), encoding(packed))]
    pub packed_varint_arr: [u64; 3],
    #[bilrost(tag(46), encoding(packed<fixed>))]
    pub packed_fixed_arr: [u32; 3],
    #[bilrost(tag(47), encoding(packed))]
    pub packed_varint_arrayvec: ArrayVec<[u64; 3]>,
    #[bilrost(tag(48), encoding(packed<fixed>))]
    pub packed_fixed_arrayvec: ArrayVec<[u32; 3]>,
    #[bilrost(tag(49), encoding(packed<plainbytes>))]
    pub packed_bytes_arrayvec: ArrayVec<[Vec<u8>; 3]>,
    /// Set, unpacked
    #[bilrost(50)]
    pub unpacked_set_uint32: BTreeSet<u32>,
    #[bilrost(tag(51), encoding(unpacked<fixed>))]
    pub unpacked_set_ufixed32: BTreeSet<u32>,
    #[bilrost(52)]
    pub unpacked_set_bool: BTreeSet<bool>,
    #[bilrost(tag(53), encoding(unpacked<plainbytes>))]
    pub unpacked_set_bytes: BTreeSet<Vec<u8>>,
    #[bilrost(54)]
    pub unpacked_set_map: BTreeSet<BTreeMap<bool, bool>>,
    /// Set, packed
    #[bilrost(tag(55), encoding(packed))]
    pub packed_set_uint32: BTreeSet<u32>,
    #[bilrost(tag(56), encoding(packed<fixed>))]
    pub packed_set_ufixed32: BTreeSet<u32>,
    #[bilrost(tag(57), encoding(packed))]
    pub packed_set_bool: BTreeSet<bool>,
    #[bilrost(tag(58), encoding(packed<plainbytes>))]
    pub packed_set_blob: BTreeSet<Vec<u8>>,
    #[bilrost(tag(59), encoding(packed))]
    pub packed_set_enum: BTreeSet<test_distinguished::NestedEnum>,
    #[bilrost(tag(60), encoding(packed))]
    pub packed_set_map: BTreeSet<BTreeMap<bool, bool>>,
    /// Set, packed & optional
    #[bilrost(tag(61), encoding(packed))]
    pub optional_packed_set_uint32: Option<BTreeSet<u32>>,
    #[bilrost(tag(62), encoding(packed))]
    pub optional_packed_set_map: Option<BTreeSet<BTreeMap<bool, bool>>>,
    /// Oneofs
    #[bilrost(63)]
    pub oneof_as_submessage: test_distinguished::OneofField,
    #[bilrost(oneof(101-109))]
    pub nonempty_oneof_field: Option<test_distinguished::NonEmptyOneofField>,
    #[bilrost(oneof(201-209))]
    pub oneof_field: test_distinguished::OneofField,
}

pub mod test_distinguished {
    use super::*;

    #[derive(Clone, Debug, PartialEq, Eq, Message)]
    #[bilrost(distinguished)]
    pub struct NestedMessage {
        #[bilrost(1)]
        pub a: u64,
        #[bilrost(tag(2), recurses)]
        pub corecursive: Option<Box<TestDistinguished>>,
        #[bilrost(oneof(201-209))]
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
        #[bilrost(tag = 107, message)]
        OneofBracedMessage {
            #[bilrost(1)]
            x: u64,
            #[bilrost(2)]
            y: u64,
            #[bilrost(3)]
            z: u64,
        },
        #[bilrost(tag = 108, message)]
        OneofTupleMessage(u64, u64, u64),
        #[bilrost(tag = 109, message)]
        OneofUnit,
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
        #[bilrost(tag = 207, message)]
        OneofBracedMessage {
            #[bilrost(1)]
            x: u64,
            #[bilrost(2)]
            y: u64,
            #[bilrost(3)]
            z: u64,
        },
        #[bilrost(tag = 208, message)]
        OneofTupleMessage(u64, u64, u64),
        #[bilrost(tag = 209, message)]
        OneofUnit,
    }
}

#[derive(Debug, PartialEq, Message)]
pub struct TestTypeSupport<'a> {
    #[bilrost(1)]
    core_duration: core::time::Duration,
    #[bilrost(2)]
    chrono_naive_date: chrono::NaiveDate,
    #[bilrost(3)]
    chrono_naive_time: chrono::NaiveTime,
    #[bilrost(4)]
    chrono_naive_date_time: chrono::NaiveDateTime,
    #[bilrost(5)]
    chrono_fixed_offset: chrono::FixedOffset,
    #[bilrost(6)]
    chrono_date_time_utc: chrono::DateTime<chrono::Utc>,
    #[bilrost(7)]
    chrono_date_time_fixed: chrono::DateTime<chrono::FixedOffset>,
    #[bilrost(8)]
    chrono_time_delta: chrono::TimeDelta,
    #[bilrost(9)]
    time_date: time::Date,
    #[bilrost(10)]
    time_time: time::Time,
    #[bilrost(11)]
    time_primitivedatetime: time::PrimitiveDateTime,
    #[bilrost(12)]
    time_utcoffset: time::UtcOffset,
    #[bilrost(13)]
    time_offsetdatetime: time::OffsetDateTime,
    #[bilrost(14)]
    time_duration: time::Duration,
    #[bilrost(15)]
    std_systemtime: std::time::SystemTime,
    #[bilrost(16)]
    smol_str: smol_str::SmolStr,
    #[bilrost(17)]
    bstr: bstr::BString,
    #[bilrost(18)]
    bstr_cow: Cow<'a, bstr::BStr>,
}

#[derive(Debug, PartialEq, Eq, Message)]
#[bilrost(distinguished)]
pub struct TestTypeSupportDistinguished<'a> {
    #[bilrost(1)]
    core_duration: core::time::Duration,
    #[bilrost(2)]
    chrono_naive_date: chrono::NaiveDate,
    #[bilrost(3)]
    chrono_naive_time: chrono::NaiveTime,
    #[bilrost(4)]
    chrono_naive_date_time: chrono::NaiveDateTime,
    #[bilrost(5)]
    chrono_fixed_offset: chrono::FixedOffset,
    #[bilrost(6)]
    chrono_date_time_utc: chrono::DateTime<chrono::Utc>,
    #[bilrost(7)]
    chrono_date_time_fixed: chrono::DateTime<chrono::FixedOffset>,
    #[bilrost(8)]
    chrono_time_delta: chrono::TimeDelta,
    #[bilrost(9)]
    time_date: time::Date,
    #[bilrost(10)]
    time_time: time::Time,
    #[bilrost(11)]
    time_primitivedatetime: time::PrimitiveDateTime,
    #[bilrost(12)]
    time_utcoffset: time::UtcOffset,
    #[bilrost(13)]
    time_offsetdatetime: time::OffsetDateTime,
    #[bilrost(14)]
    time_duration: time::Duration,
    #[bilrost(15)]
    smol_str: smol_str::SmolStr,
    #[bilrost(16)]
    bstr: bstr::BString,
    #[bilrost(17)]
    bstr_cow: Cow<'a, bstr::BStr>,
}

#[derive(Debug, PartialEq, Eq, Message)]
#[bilrost(distinguished)]
pub struct TestTypeSupportBorrowable<'a> {
    #[bilrost(1)]
    str: Cow<'a, str>,
    #[bilrost(tag(2), encoding(plainbytes))]
    bytes: Cow<'a, [u8]>,
    #[bilrost(3)]
    bstr: Cow<'a, bstr::BStr>,
    #[bilrost(tag(4), encoding(plainbytes))]
    small_array: Cow<'a, [u8; 1]>,
    #[bilrost(tag(5), encoding(plainbytes))]
    bigger_array: Cow<'a, [u8; 16]>,
}

#[derive(Debug, PartialEq, Eq, Oneof, Message)]
#[bilrost(distinguished)]
pub enum TestOneofMessage<'a> {
    Empty,
    #[bilrost(1)]
    Varint(u64),
    #[bilrost(tag(2), encoding(plainbytes))]
    Delimited(&'a [u8]),
    #[bilrost(tag(3), encoding(fixed))]
    Fixed4(u32),
    #[bilrost(tag(4), encoding(fixed))]
    Fixed8(u64),
}

#[derive(Debug, PartialEq, Eq, Message)]
#[bilrost(distinguished)]
pub struct TestOneofMessageMock<'a> {
    #[bilrost(1)]
    pub varint: Option<u64>,
    #[bilrost(tag(2), encoding(plainbytes))]
    pub delimited: Option<&'a [u8]>,
    #[bilrost(tag(3), encoding(fixed))]
    pub fixed4: Option<u32>,
    #[bilrost(tag(4), encoding(fixed))]
    pub fixed8: Option<u64>,
}
