//! These are the integration tests for schema functionality. We write them here in a different
//! test crate so that we can depend on the common test-types crate, which enables a bunch of
//! features for type support that we don't want to always have enabled in our other integration
//! tests.
//!
//! ```rust,compile_fail
//! // The assertion in the Schema derive fires either when the call is instantiated.
//! //
//! // We don't derive Message for this to ensure that the const assertions in that impl don't
//! // run either. Currently (rust 1.99) it still works if we make the oneof field's type generic,
//! // but it's allowable for the compiler to monomorphize those constants as soon as the type is
//! // named so we want to be sure that we're at least testing the actual asserts in the Schema
//! // derive and not just failing to compile due to the asserts in the other derive.
//! #[derive(Schema)]
//! struct Foo {
//!     #[bilrost(oneof(1))]
//!     #[allow(dead_code)]
//!     bar: Bar,
//! }
//!
//! #[derive(Oneof, Schema)]
//! enum Bar {
//!     Nothing,
//!     #[bilrost(2)]
//!     Something(u64),
//! }
//!
//! let schema = Schema::new();
//! schema.register::<Foo>();
//! ```

#![cfg(test)]
use bilrost::{Enumeration, Message, Oneof, Schema};
use std::collections::BTreeMap;
use test_types::{
    TestAllTypes, TestDistinguished, TestOneofMessage, TestTypeSupport, TestTypeSupportBorrowable,
    TestTypeSupportDistinguished,
};

#[test]
fn schema_output() {
    #[derive(Message, Schema)]
    struct NestedTypeSupport {
        #[bilrost(encoding(unpacked<packed<map<packed, packed>>>))]
        nested: Vec<Vec<BTreeMap<Vec<String>, [String; 5]>>>,
    }

    let schema = Schema::new();
    schema.register::<TestAllTypes>();
    schema.register::<TestDistinguished>();
    schema.register::<TestTypeSupport>();
    schema.register::<TestTypeSupportBorrowable>();
    schema.register::<TestTypeSupportDistinguished>();
    schema.register::<TestOneofMessage>();
    schema.register::<NestedTypeSupport>();
    schema.register::<()>();

    for schema_output in [format!("{schema}"), format!("{}", schema.with_rust_types())] {
        for expected in [
            "message TestAllTypes {",
            "message () {",
            "enumeration NestedEnum {",
            "message OneofField {",
        ] {
            assert!(
                schema_output.contains(expected),
                "didn't contain {expected}",
            );
        }
        assert!(!schema_output.contains("!!"));
    }
}

#[test]
fn schema_renaming() {
    #[derive(Message, Schema)]
    #[bilrost(name = "XSchemaMessageX")]
    struct XRustMessageX {
        #[bilrost(name = "xschema_fieldx")]
        xrust_fieldx: u64,
        #[bilrost(oneof(2, 3), name = "xschema_oneof_fieldx")]
        xrust_oneof_fieldx: XRustOneofX,
        #[bilrost(name = "xschema_enumeration_fieldx")]
        xrust_enumeration_fieldx: XRustEnumerationX,
    }

    #[derive(Message, Oneof, Schema)]
    #[bilrost(name = "XSchemaOneofX")]
    enum XRustOneofX {
        Empty,
        #[bilrost(tag(2), name = "XSchemaVariantX")]
        XRustVariantX(u64),
        #[bilrost(tag(3), message, name = "XSchemaMessageVariantX")]
        XRustMessageVariantX {
            #[bilrost(name = "xschema_variant_fieldx")]
            xrust_variant_fieldx: u64,
        },
    }

    #[derive(PartialEq, Eq, Enumeration)]
    #[bilrost(name = "XSchemaEnumerationX")]
    enum XRustEnumerationX {
        #[bilrost(name = "XSchemaEnumerationValX")]
        XRustEnumerationValX = 0,
    }

    let schema = Schema::new();
    schema.register::<XRustMessageX>();
    schema.register::<XRustOneofX>();

    let plain = format!("{schema}");
    let with_rust = format!("{}", schema.with_rust_types());

    for always_expected in [
        "XSchemaMessageX",
        "xschema_fieldx",
        "xschema_oneof_fieldx",
        "xschema_enumeration_fieldx",
        "XSchemaOneofX",
        "XSchemaVariantX",
        "XSchemaMessageVariantX",
        "xschema_variant_fieldx",
        "XSchemaEnumerationX",
        "XSchemaEnumerationValX",
    ] {
        assert!(plain.contains(always_expected));
        assert!(with_rust.contains(always_expected));
    }
    for only_in_rust in ["XRustMessageX", "XRustOneofX", "XRustEnumerationX"] {
        assert!(!plain.contains(only_in_rust));
        assert!(with_rust.contains(only_in_rust));
    }
    for variation in [&plain, &with_rust] {
        assert!(!variation.contains("!!"));
    }

    // Keeping an assertion on the full schema content for now
    assert_eq!(
        plain,
        "\
[1] enumeration XSchemaEnumerationX {
    0: XSchemaEnumerationValX,
}

[2] message XSchemaMessageX {
    1: xschema_fieldx (varint, unsigned),
    2: xschema_oneof_fieldx variant XSchemaVariantX (varint, unsigned),
    3: xschema_oneof_fieldx variant XSchemaMessageVariantX (delimited message XSchemaOneofX::XSchemaMessageVariantX [4]),
    4: xschema_enumeration_fieldx (varint, unsigned; one of enumeration XSchemaEnumerationX [1]),
}

[3] message XSchemaOneofX {
    2: variant XSchemaVariantX (varint, unsigned),
    3: variant XSchemaMessageVariantX (delimited message XSchemaOneofX::XSchemaMessageVariantX [4]),
}

[4] message XSchemaOneofX::XSchemaMessageVariantX {
    1: xschema_variant_fieldx (varint, unsigned),
}
"
    )
}
