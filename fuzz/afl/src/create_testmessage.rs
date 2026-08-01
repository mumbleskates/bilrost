use bilrost::encoding::EmptyState;
use bilrost::Message;
use test_types::{test_distinguished, test_message, TestAllTypes, TestDistinguished};

fn main() {
    let msg = TestAllTypes {
        optional_sint32: Some(42),
        sfixed64: 9983748923,
        bool: true,
        recursive_message: Some(Box::new(TestAllTypes {
            unpacked_sint32: vec![1, 2, 99, 50, -5],
            ..EmptyState::empty()
        })),
        packed_fixed_arr: [60, 70, 80],
        packed_varint_arrayvec: [100, 200].into_iter().collect(),
        optional_packed_set_uint32: Some(vec![1, 2, 1, 2].into_iter().collect()),
        unpacked_float32: vec![-1.0, 10.10, 1.337, f32::NAN],
        oneof_as_submessage: test_message::OneofField::OneofEnum(test_message::NestedEnum::Baz),
        nonempty_oneof_field: Some(test_message::NonEmptyOneofField::OneofString(
            "foo".to_owned(),
        )),
        ..EmptyState::empty()
    };
    let distinguished_msg = TestDistinguished {
        optional_ufixed32: Some(42),
        ufixed64: 9983748923,
        bool: true,
        optional_message: Some(test_distinguished::NestedMessage {
            a: 123,
            corecursive: Some(Box::new(TestDistinguished {
                unpacked_fixed_arr: [1, 2, 999],
                ..EmptyState::empty()
            })),
            ..EmptyState::empty()
        }),
        packed_fixed_arr: [60, 70, 80],
        packed_varint_arrayvec: [100, 200].into_iter().collect(),
        optional_packed_set_uint32: Some(vec![1, 2, 1, 2].into_iter().collect()),
        unpacked_varint: vec![1, 100, 10000, 55555],
        oneof_as_submessage: test_distinguished::OneofField::OneofEnum(
            test_distinguished::NestedEnum::Baz,
        ),
        nonempty_oneof_field: Some(test_distinguished::NonEmptyOneofField::OneofBytes(
            b"foo".to_vec(),
        )),
        ..EmptyState::empty()
    };
    std::fs::write("in/all_types.bilrost", msg.encode_to_vec()).unwrap();
    std::fs::write(
        "in/distinguished.bilrost",
        distinguished_msg.encode_to_vec(),
    )
    .unwrap();
}
