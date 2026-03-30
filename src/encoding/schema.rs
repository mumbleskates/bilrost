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
use alloc::string::String;
use alloc::sync::Arc;
use core::any::{Any, TypeId};
use core::fmt::Formatter;
use core::ops::DerefMut;

/// Common trait for interior mutability
trait BorrowGuard<T> {
    type Borrowed<'a>: DerefMut<Target = T>
    where
        Self: 'a,
        T: 'a;

    fn new(t: T) -> Self;

    fn get_guarded(&self) -> Self::Borrowed<'_>;
}

#[cfg(feature = "threadsafe-schema")]
mod guard {
    pub(super) use spin::Mutex as Guard;

    impl<T> super::BorrowGuard<T> for Guard<T> {
        type Borrowed<'a>
            = spin::MutexGuard<'a, T>
        where
            T: 'a;

        fn new(t: T) -> Self {
            Guard::new(t)
        }

        fn get_guarded(&self) -> Self::Borrowed<'_> {
            self.lock()
        }
    }
}

#[cfg(not(feature = "threadsafe-schema"))]
mod guard {
    pub(super) use core::cell::RefCell as Guard;

    impl<T> super::BorrowGuard<T> for Guard<T> {
        type Borrowed<'a>
            = core::cell::RefMut<'a, T>
        where
            T: 'a;

        fn new(t: T) -> Self {
            Guard::new(t)
        }

        fn get_guarded(&self) -> Self::Borrowed<'_> {
            self.borrow_mut()
        }
    }
}

use guard::Guard;

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
///
/// This trait is always implemented on the unit type `()`.
pub trait ValueSchema<E, T: ?Sized> {
    /// Returns the representation of the field. This may register other message types with the
    /// schema and the returned value may use the schema to look up the name of those other message
    /// types when displaying.
    fn repr(schema: &impl Schema) -> Box<dyn Display>;
}

pub fn repr<E, T>(schema: &impl Schema) -> Box<dyn Display>
where
    (): ValueSchema<E, T>,
{
    <() as ValueSchema<E, T>>::repr(schema)
}

/// A collected internally-complete set of message definitions.
struct MessageSet {
    types: Guard<BTreeMap<TypeId, Arc<Guard<MessageInfo>>>>,
    // TODO: pre-organize the types (or their names) for disambiguation and which index they will
    //  be found at in the final printout
}

impl MessageSet {
    fn new() -> Arc<Self> {
        Self {
            types: Default::default(),
        }
        .into()
    }
}

impl Schema for Arc<MessageSet> {
    fn register<M: MessageSchema>(&self, name: &str) {
        let mut types = self.types.get_guarded();
        let field_set = match types.entry(TypeId::of::<M>()) {
            Entry::Vacant(entry) => entry
                .insert(Arc::new(Guard::new(MessageInfo::new(name))))
                .clone(),
            Entry::Occupied(mut entry) => {
                entry.get_mut().get_guarded().add_name(name);
                return; // type was already registered previously
            }
        };
        drop(types);
        M::register_fields(field_set.get_guarded().deref_mut(), self);
    }

    fn type_reference<M: MessageSchema>(&self) -> String {
        // TODO: this is a placeholder, we want to use the type's ordinal after they're organized
        let id = TypeId::of::<M>();
        let name = self
            .types
            .get_guarded()
            .get(&id)
            .map_or_else(|| "<unnamed>".to_owned(), |info| info.get_guarded().name());
        format!("{name} ({id:?})")
    }
}

impl Display for MessageSet {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        let mut type_indexes = BTreeMap::new();
        for (msg_idx, (type_id, msg_info)) in self.types.get_guarded().iter().enumerate() {
            type_indexes.insert(*type_id, msg_idx);
            if msg_idx > 0 {
                writeln!(f)?;
            }
            let msg_info = msg_info.get_guarded();
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
