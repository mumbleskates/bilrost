//! Tools for outputting (human-readable) information about the encoding of the message types used
//! in a program.
//!
//! The general flow goes like this:
//!  * Create a Schema
//!  * Register each of the message types we want to see with that schema. Messages that contain
//!    other messages will register those in turn.
//!      * When a message is registered this way, new message types that haven't been registered
//!        before will have `MessageSchema::register_fields` called, and must describe their
//!        fields into the `FieldSet` provided.
//!  * Finally, once all relevant message types are registered, the schema can be Displayed, which
//!    will output all the collected information.

use alloc::borrow::ToOwned;
use alloc::boxed::Box;
use alloc::collections::btree_map::Entry;
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::fmt::Display;
use alloc::format;
use alloc::rc::Rc;
use alloc::string::String;
use core::any::{Any, TypeId};
use core::cell::RefCell;
use core::fmt::Formatter;

pub fn new_schema() -> impl Schema {
    MessageSet::new()
}

/// Receptacle for a full schema that can record the schemas of many messages.
///
/// This trait is usable from a const reference with interior mutability because when messages
/// register the appearance of their fields' reprs they will sometimes capture references to the
/// whole schema so that they can get the name of a type. We want the overall schema not to be
/// frozen in place as we collect these reprs even as their potential output changes and more types
/// are registered.
pub trait Schema: Clone + Display {
    // TODO: notes for a type, like what it should be called etc.
    /// Visits a specific message type. May shortcut if this method has already been invoked
    /// elsewhere for the same type.
    fn register<M: MessageSchema>(&self, name: &str);
    /// Name for a type that disambiguates where it can be found in the entire schema output.
    fn type_reference<M: MessageSchema>(&self) -> String;
}

/// Receptacle for fields in a single specific message type.
pub trait FieldSet {
    // TODO: notes for the whole type (distinguished?)
    /// Adds a field to the type.
    fn add_field(&mut self, name: &str, tag: u32, repr: Box<dyn Display>);
    /// Registers a oneof on the message by the tags of its mutually exclusive member fields.
    fn add_oneof(&mut self, name: &str, tags: &[u32]);
}

/// A message type that can report the fields in its schema.
pub trait MessageSchema: Any {
    fn register_fields(fields: &mut impl FieldSet, schema: &impl Schema);
}

/// Trait for an encoding E to describe its representation of a type T.
pub trait ValueSchema<E, T> {
    fn repr(schema: &impl Schema) -> Box<dyn Display>;
}

/// A collected internally-complete set of message definitions.
struct MessageSet {
    types: RefCell<BTreeMap<TypeId, MessageInfo>>,
    // TODO: pre-organize the types (or their names) for disambiguation and which index they will
    //  be found at in the final printout
}

impl MessageSet {
    fn new() -> Rc<Self> {
        Self {
            types: Default::default(),
        }
        .into()
    }
}

impl Schema for Rc<MessageSet> {
    fn register<M: MessageSchema>(&self, name: &str) {
        let mut types = self.types.borrow_mut();
        match types.entry(TypeId::of::<M>()) {
            Entry::Vacant(entry) => {
                let field_set = entry.insert(MessageInfo::new(name));
                // TODO: if another message type is registered here we will probably crash due to
                //  the borrow of types. probably there needs to be another refcell around the
                //  MessageInfo so we can insert it first and then modify it without holding a mut
                //  borrow on self.types
                M::register_fields(field_set, self);
            }
            Entry::Occupied(mut entry) => {
                entry.get_mut().add_name(name);
                // type was already registered previously
            }
        }
    }

    fn type_reference<M: MessageSchema>(&self) -> String {
        // TODO: this is a placeholder
        let id = TypeId::of::<M>();
        let name = self
            .types
            .borrow()
            .get(&id)
            .map_or_else(|| "<unnamed>".to_owned(), |info| info.name());
        format!("{name} ({id:?})")
    }
}

impl Display for MessageSet {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        let mut type_indexes = BTreeMap::new();
        for (msg_idx, (type_id, msg_info)) in self.types.borrow().iter().enumerate() {
            type_indexes.insert(*type_id, msg_idx);
            if msg_idx > 0 {
                writeln!(f)?;
            }
            writeln!(f, "[{msg_idx}] {name} {{", name = msg_info.name())?;
            for (tag, field) in &msg_info.fields {
                writeln!(
                    f,
                    "    {tag}: {field_name} ({repr}),",
                    field_name = &field.name,
                    repr = &field.repr,
                )?;
            }
            writeln!(f, "}}")?;
        }
        Ok(())
    }
}

/// Collected information about a specific message type and its fields.
struct MessageInfo {
    names: BTreeSet<String>,
    fields: BTreeMap<u32, FieldInfo>,
    oneofs: BTreeMap<String, BTreeSet<u32>>,
}

impl MessageInfo {
    fn new(name: &str) -> Self {
        Self {
            names: [name.to_owned()].into(),
            fields: Default::default(),
            oneofs: Default::default(),
        }
    }

    fn add_name(&mut self, name: &str) {
        self.names.insert(name.to_owned());
    }

    fn name(&self) -> String {
        match self.names.len() {
            // MSRV: this could be .first()
            1 => format!("{name:?}", name = self.names.iter().next().unwrap()),
            _ => format!("(message known as {names:?})", names = self.names),
        }
    }
}

impl FieldSet for MessageInfo {
    fn add_field(&mut self, name: &str, tag: u32, repr: Box<dyn Display>) {
        if self
            .fields
            .insert(
                tag,
                FieldInfo {
                    name: name.to_owned(),
                    repr,
                },
            )
            .is_some()
        {
            panic!(
                "message {name} registered multiple fields with tag {tag}",
                name = self.name(),
            );
        };
    }

    fn add_oneof(&mut self, oneof_name: &str, tags: &[u32]) {
        let tag_set = tags.into_iter().copied().collect();
        for (existing_oneof_name, existing_set) in &self.oneofs {
            if let Some(conflicting_tag) = existing_set.intersection(&tag_set).next() {
                panic!(
                    "message {message_name} registered conflicting oneof sets: message declared \
                    oneofs which both contain tag {conflicting_tag}: {oneof_name:?}={tag_set:?} \
                    and {existing_oneof_name:?}={existing_set:?}",
                    message_name = self.name(),
                );
            }
        }
        if let Some(conflicting_name) = self.oneofs.insert(oneof_name.to_owned(), tag_set) {
            panic!(
                "message {message_name} registered multiple oneof sets with the name \
                {conflicting_name:?}",
                message_name = self.name(),
            );
        }
    }
}

struct FieldInfo {
    name: String,
    repr: Box<dyn Display>,
}
