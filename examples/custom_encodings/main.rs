use bilrost::{Message, OwnedMessage};
use std::sync::Arc;

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
        #[bilrost(tag(1), encoding(arced<varint>))]
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
    let output = DemoCustom::decode(input.encode_to_vec().as_slice()).expect("should decode");
    println!("output: {:#?}", output);
}
