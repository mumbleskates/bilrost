//! Tests for skipping the default Debug implementation.

use crate::custom_debug::{msg, AnEnum, Msg};
use crate::message_encoding::BasicEnumeration;
use bilrost::alloc::{format, string::String};
use std::fmt;

/// A special case with a tuple struct
#[test]
fn tuple_struct_custom_debug() {
    #[derive(Clone, PartialEq, bilrost::Message)]
    #[bilrost(skip_debug)]
    struct NewType(#[bilrost(enumeration = "BasicEnumeration", tag = "5")] i32);
    impl fmt::Debug for NewType {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("NewType(custom_debug)")
        }
    }
    assert_eq!(
        format!("{:?}", NewType(BasicEnumeration::TWO as i32)),
        "NewType(custom_debug)"
    );
    assert_eq!(format!("{:?}", NewType(42)), "NewType(custom_debug)");
}

#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, bilrost::Oneof)]
#[bilrost(skip_debug)]
pub enum OneofWithEnumCustomDebug {
    #[bilrost(int32, tag = "8")]
    Int(i32),
    #[bilrost(string, tag = "9")]
    String(String),
    #[bilrost(enumeration = "BasicEnumeration", tag = "10")]
    Enumeration(i32),
}

#[derive(Clone, PartialEq, bilrost::Message)]
#[bilrost(skip_debug)]
struct MessageWithOneofCustomDebug {
    #[bilrost(oneof = "OneofWithEnumCustomDebug", tags = "8, 9, 10")]
    of: Option<OneofWithEnumCustomDebug>,
}

impl fmt::Debug for MessageWithOneofCustomDebug {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("MessageWithOneofCustomDebug {..}")
    }
}

/// Enumerations inside oneofs
#[test]
fn oneof_with_enum_custom_debug() {
    let msg = MessageWithOneofCustomDebug {
        of: Some(OneofWithEnumCustomDebug::Enumeration(
            BasicEnumeration::TWO as i32,
        )),
    };
    assert_eq!(format!("{:?}", msg), "MessageWithOneofCustomDebug {..}");
}

/// Generated protobufs
#[test]
fn test_proto_msg_custom_debug() {
    let msg = Msg {
        a: 0,
        b: "".to_string(),
        c: Some(msg::C::D(AnEnum::A as i32)),
    };
    assert_eq!(format!("{:?}", msg), "Msg {..}");
}
