//! This little crate is here to import bilrost from a different path and test that the
//! `#[bilrost(crate)]` attributes work properly. We make sure that the core library is unavailable
//! at `::bilrost` where it is normally present and then run the derive macros with a couple
//! different alternative paths to make sure everything compiles and runs.

use tsorlib::{Enumeration, Message, Oneof, Schema};

mod alternative {
    pub use ::tsorlib as bilrost;
}

#[derive(Debug, PartialEq, Message, Schema)]
#[bilrost(crate = "tsorlib")]
struct TestMessage {
    x: u64,
    y: String,
    #[bilrost(oneof(4, 5))]
    either_or: EitherOr,
    abc: Abc,
}

#[derive(Debug, PartialEq, Oneof, Message, Schema)]
#[bilrost(crate = "::tsorlib")]
enum EitherOr {
    #[bilrost(4)]
    Either(String),
    #[bilrost(tag(5), message)]
    Or { yeah: bool, nah: bool },
    #[bilrost(empty)]
    Nevermind,
}

#[derive(Debug, PartialEq, Eq, Enumeration)]
#[bilrost(crate(alternative::bilrost))]
enum Abc {
    A = 0,
    B = 3,
    #[bilrost(2)]
    C,
}

#[test]
fn encode_decode_and_schema() {
    use tsorlib::OwnedMessage;

    let msg = TestMessage {
        x: 1,
        y: "okay".to_owned(),
        either_or: EitherOr::Or {
            yeah: true,
            nah: false,
        },
        abc: Abc::B,
    };
    let encoded = msg.encode_fast();
    assert_eq!(Ok(msg), TestMessage::decode(encoded));

    let schema = Schema::new();
    schema.register::<TestMessage>();
    let schema = schema.to_string();
    assert!(schema.contains("enumeration Abc {"));
    assert!(!schema.contains("!!"));
}
