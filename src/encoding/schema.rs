//! Tools for outputting (human-readable) information about the encoding of the message types used
//! in a program.
//!
//! The general flow goes like this:
//!  * Create a Schema
//!  * Register each of the message types we want to see with that schema.
//!      * When a message is registered this way, new message types that haven't been registered
//!        before will have `MessageSchema::register_fields` called, and must describe their
//!        fields into the `FieldSet` provided.
//!      * Messages that contain other messages will register those in turn.
//!  * Finally, once all relevant message types are registered, the schema can be Displayed, which
//!    will output all the collected information.

use alloc::borrow::ToOwned;
use alloc::boxed::Box;
use alloc::collections::btree_map::Entry;
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;
use core::any::{Any, TypeId};
use core::fmt::{Display, Formatter};
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

/// A collected internally-complete set of message definitions.
#[derive(Clone)]
pub struct Schema(Arc<MessageSet>);

struct MessageSet {
    types: Guard<BTreeMap<TypeId, Arc<Guard<MessageInfo>>>>,
    // TODO: pre-organize the types (or their names) for disambiguation and which index they will
    //  be found at in the final printout
}

/// Receptacle for a full schema that can record the schemas of many messages.
///
/// This trait is usable from a const reference with interior mutability because when messages
/// register the appearance of their fields' reprs they will sometimes capture references to the
/// whole schema so that they can get the name of a type. We want the overall schema not to be
/// frozen in place as we collect these reprs even as their potential output changes and more types
/// are registered.
impl Schema {
    pub fn new() -> Self {
        Self(
            MessageSet {
                types: Default::default(),
            }
            .into(),
        )
    }

    /// Visits a specific message type. May shortcut if this method has already been invoked
    /// elsewhere for the same type.
    // TODO: this should probably take no name and there should be another method for naming it
    pub fn register<M: MessageSchema + ?Sized>(&self) {
        self.register_with_alias::<M>("")
    }

    pub fn register_with_alias<M: MessageSchema + ?Sized>(&self, name: &str) {
        let mut types = self.0.types.get_guarded();
        let field_set = match types.entry(TypeId::of::<M>()) {
            Entry::Vacant(entry) => entry
                .insert(Arc::new(Guard::new(MessageInfo::new(name))))
                .clone(),
            Entry::Occupied(mut entry) => {
                if name != "" {
                    entry.get_mut().get_guarded().add_name(name);
                }
                return; // type was already registered previously
            }
        };
        drop(types);
        M::register_fields(field_set.get_guarded().deref_mut(), self);
    }

    /// Name for a type that disambiguates where it can be found in the entire schema output. The
    /// output of this function may differ as more types are added to the schema, so this should
    /// only be called when the whole schema is being rendered; see `make_lazy_repr`.
    pub fn type_reference<M: MessageSchema + ?Sized>(&self) -> String {
        // TODO: this is a placeholder, we want to use the type's ordinal after they're organized
        let id = TypeId::of::<M>();
        let name = self
            .0
            .types
            .get_guarded()
            .get(&id)
            .map_or_else(|| "<unnamed>".to_owned(), |info| info.get_guarded().name());
        format!("{name} ({id:?})")
    }

    /// Returns a lazily-evaluated repr using the given closure. The closure won't be invoked until
    /// the schema is displayed, so `schema.type_reference::<T>()` can correctly specify the name
    /// of the type `T`.
    pub fn make_lazy_repr<A>(&self, a: A) -> Box<dyn Display>
    where
        A: 'static + Fn(&Schema, &mut core::fmt::Formatter<'_>) -> core::fmt::Result,
    {
        struct LazyRepr<A> {
            schema: Schema,
            func: A,
        }

        impl<A> Display for LazyRepr<A>
        where
            A: 'static + Fn(&Schema, &mut core::fmt::Formatter<'_>) -> core::fmt::Result,
        {
            fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
                (self.func)(&self.schema, f)
            }
        }

        Box::new(LazyRepr {
            schema: self.clone(),
            func: a,
        })
    }
}

/// Receptacle for fields in a single specific message type.
pub trait FieldSet {
    /// Adds a name for the whole message.
    fn add_name(&mut self, message_name: &str);
    // TODO: notes for the whole type (distinguished?)
    /// Adds a field to the type.
    fn add_field(&mut self, name: &str, tag: u32, repr: Box<dyn Display>);
    /// Registers a oneof on the message by the tags of its mutually exclusive member fields.
    fn add_oneof(&mut self, name: &str, tags: &[u32]);
}

/// A message type that can report the fields in its schema.
pub trait MessageSchema: Any {
    fn register_fields(fields: &mut impl FieldSet, schema: &Schema);
}

/// Possibly an easier way to register types with a schema. This just calls the corresponding
/// method on the schema object, but doesn't require a turbofish to spell.
pub trait Registerable: MessageSchema {
    fn register(schema: &Schema) {
        schema.register::<Self>();
    }

    fn register_with_alias(schema: &Schema, name: &str) {
        schema.register_with_alias::<Self>(name);
    }
}

impl<T: MessageSchema + ?Sized> Registerable for T {}

/// Trait for an encoding E to describe its representation of a type T.
///
/// This trait is always implemented on the unit type `()`.
pub trait ValueSchema<E, T: ?Sized> {
    /// Returns the representation of the field. This may register other message types with the
    /// schema and the returned value may use the schema to look up the name of those other message
    /// types when displaying.
    fn repr(schema: &Schema) -> Box<dyn Display>;
}

impl Display for Schema {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        let mut type_indexes = BTreeMap::new();
        for (msg_idx, (type_id, msg_info)) in self.0.types.get_guarded().iter().enumerate() {
            type_indexes.insert(*type_id, msg_idx);
            // TODO: build the indexes in the schema itself and use them in `type_reference`
            if msg_idx > 0 {
                writeln!(f)?;
            }
            let msg_info = msg_info.get_guarded();
            writeln!(f, "[{msg_idx}] {name} {{", name = msg_info.name())?;
            if msg_info.fields.is_empty() {
                writeln!(f, "    // empty")?;
            } else {
                for (tag, field) in &msg_info.fields {
                    writeln!(
                        f,
                        "    {tag}: {field_name} ({repr}),",
                        field_name = &field.name,
                        repr = &field.repr,
                    )?;
                }
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

    fn name(&self) -> String {
        match self.names.len() {
            // MSRV: this could be .first()
            1 => format!("{name:?}", name = self.names.iter().next().unwrap()),
            _ => format!("(message known as {names:?})", names = self.names),
        }
    }
}

impl FieldSet for MessageInfo {
    fn add_name(&mut self, name: &str) {
        self.names.insert(name.to_owned());
    }

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
