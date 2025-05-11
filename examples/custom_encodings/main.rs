use bilrost::{Message, OwnedMessage};
use std::sync::Arc;

/// `arc_encoding::ArcEncoding` implements a custom encoding from outside the `bilrost` crate, for
/// the type `Arc<T>` which is also not owned by us. All this encoding does is directly pass through
/// all encoding traits directly from the `T` inside the arc; if it is to be modified, we first call
/// `Arc::make_mut` to ensure that we have a unique copy.
///
/// The `bilrost` crate itself will likely never provide this *specific* amenity for `Arc<T>` or
/// `Rc<T>` in particular because there are risks and pitfalls around reference cycles that may
/// cause `bilrost` to crash the program when encoding. It's still very useful for the careful user
/// though, and serves as an excellent example of how something like this can be implemented.
mod arc_encoding;
use arc_encoding::ArcEncoding as arced;

fn main() {
    #[derive(Debug, Default, Message)]
    struct Plain {
        #[bilrost(tag(1), encoding(varint))]
        scalar: Option<u64>,
        #[bilrost(tag(2))]
        name: String,
        #[bilrost(tag(3), recurses)]
        tree_children: Vec<Box<Self>>,
    }

    #[derive(Clone, Debug, Message)]
    struct DemoCustom {
        #[bilrost(tag(1), encoding(arced<varint>))] // Any field can be wrapped in Arc
        scalar: Option<Arc<u64>>,
        #[bilrost(tag(2))]
        name: String,
        #[bilrost(tag(3), encoding(unpacked<arced<general>>), recurses)]
        tree_children: Vec<Arc<Self>>,
    }

    let input = Plain {
        scalar: Some(123),
        name: "root".to_owned(),
        tree_children: vec![
            Box::new(Plain {
                scalar: None,
                name: "left".to_owned(),
                tree_children: vec![],
            }),
            Box::new(Plain {
                scalar: None,
                name: "right".to_owned(),
                tree_children: vec![],
            }),
            Box::new(Plain {
                scalar: Some(999),
                name: "some numbers".to_owned(),
                tree_children: vec![
                    Box::new(Plain {
                        scalar: Some(55),
                        name: "some fives".to_owned(),
                        tree_children: vec![],
                    }),
                    Box::new(Plain {
                        scalar: Some(555555),
                        name: "additional fives".to_owned(),
                        tree_children: vec![],
                    }),
                ],
            }),
        ],
    };
    println!("input: {:#?}", input);
    let encoded = input.encode_to_vec();
    let output = DemoCustom::decode(encoded.as_slice()).expect("should decode");
    println!("output: {:#?}", output);
    assert_eq!(encoded, output.encode_to_vec());
}
