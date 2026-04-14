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
use core::any::{type_name, Any, TypeId};
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
    types: Guard<BTreeMap<TypeId, Arc<Guard<TypeInfo>>>>,
    // TODO: pre-organize the types (or their names) for disambiguation and which index they will
    //  be found at in the final printout
    // TODO: registry of type equivalence and maybe alternate names
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

    /// Registers a specific message type. May shortcut if this method has already been invoked
    /// for the same type.
    pub fn register_message<M: Any + ?Sized>(
        &self,
        name: &str,
        fields: impl Fn(&mut MessageFields),
    ) {
        let info = match self.0.types.get_guarded().entry(type_of::<M>()) {
            Entry::Vacant(entry) => entry
                .insert(Arc::new(Guard::new(TypeInfo {
                    name: name.to_owned(),
                    alternate_names: Default::default(),
                    details: TypeDetails::Message(MessageFields::new()),
                })))
                .clone(),
            Entry::Occupied(entry) => return, // already registered
        };

        // Only after we've inserted the message info into the map do we populate its fields. This
        // ensures that we are no longer holding the lock on `types` so other types can be
        // recursively registered.
        let TypeInfo {
            details: TypeDetails::Message(msg),
            ..
        } = info.get_guarded().deref_mut()
        else {
            unreachable!();
        };
        fields(msg)
    }

    pub fn register_enumeration<E: ?Sized>(&self, name: &str, fields: impl Fn(&mut EnumFields)) {
        let info = match self.0.types.get_guarded().entry(type_of::<M>()) {
            Entry::Vacant(entry) => entry
                .insert(Arc::new(Guard::new(TypeInfo {
                    name: name.to_owned(),
                    alternate_names: Default::default(),
                    details: TypeDetails::Enum(EnumInfo::new()),
                })))
                .clone(),
            Entry::Occupied(entry) => return, // already registered
        };

        // Only after we've inserted the enum info into the map do we populate its fields. This
        // ensures that we are no longer holding the lock on `types` so other types can be
        // recursively registered.
        let TypeInfo {
            details: TypeDetails::Enum(enum_info),
            ..
        } = info.get_guarded().deref_mut()
        else {
            unreachable!();
        };
        fields(enum_info)
    }

    /// Registers the message variants of a oneof enum. A oneof may have several variants that each
    /// encode as messages, and they should each be registered on the `OneofMessages` value
    /// provided to the `variants` closure.
    pub fn register_oneof_messages<T: Any + ?Sized>(
        &self,
        name: &str,
        variants: impl Fn(&mut OneofMessages),
    ) {
        let info = match self.0.types.get_guarded().entry(type_of::<M>()) {
            Entry::Vacant(entry) => entry
                .insert(Arc::new(Guard::new(TypeInfo {
                    name: name.to_owned(),
                    alternate_names: Default::default(),
                    details: TypeDetails::Oneof(OneofMessages::new()),
                })))
                .clone(),
            Entry::Occupied(entry) => return, // already registered
        };

        // Only after we've inserted the oneof info into the map do we populate its variants. This
        // ensures that we are no longer holding the lock on `types` so other types can be
        // recursively registered.
        let TypeInfo {
            details: TypeDetails::Oneof(submsgs),
            ..
        } = info.get_guarded().deref_mut()
        else {
            unreachable!();
        };
        variants(submsgs)
    }

    // TODO: way to register alternate names; the TypeInfo needs to be restructured so alternate
    //  names can be added by &const without needing to look at the details object. Maybe these
    //  go in the same place as the "equivalent types" thing we need to do for boxed messages

    /// Name for a type that disambiguates where it can be found in the entire schema output. The
    /// output of this function may differ as more types are added to the schema, so this should
    /// only be called when the whole schema is being rendered; see `make_lazy_repr`.
    pub fn type_reference<M: Any + ?Sized>(&self) -> String {
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

    /// Name for a sub-type (message variant of a oneof enum) that disambiguates where it can be
    /// found in the entire schema output. The output of this function may differ as more types are
    /// added to the schema, so this should only be called when the whole schema is being rendered;
    /// see `make_lazy_repr`.
    pub fn subtype_reference<M: Any + ?Sized, const Tag: u32>(&self) -> String {
        // TODO: this is a placeholder, we want to use the type's ordinal after they're organized
        let id = TypeId::of::<M>();
        let name = self.0.types.get_guarded().get(&id).map_or_else(
            || "<unnamed>".to_owned(),
            |info| {
                let info = info.get_guarded();
                let type_name = &info.name;
                let TypeDetails::Oneof(variants) = info.get_guarded().details else {
                    panic!("type {name:?} is not registered as a oneof with subtypes");
                };
                let (variant_name, _) = variants
                    .variants
                    .get(&Tag)
                    .expect("type {name:?} does not have a registered variant with tag {Tag}");
                format!("{type_name}::{variant_name}")
            },
        );
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

struct TypeInfo {
    name: String,
    alternate_names: BTreeSet<String>,
    details: TypeDetails,
}

impl TypeInfo {
    fn name(&self) -> String {
        match self.alternate_names.len() {
            0 => format!("{name:?}", name = self.name),
            _ => format!(
                "{name:?} (aka {alts:?})",
                name = self.name,
                alts = self.alternate_names,
            ),
        }
    }
}

enum TypeDetails {
    Message(MessageFields),
    Enum(EnumInfo),
    Oneof(OneofMessages),
}

/// Collected information about a specific message type and its fields.
pub struct MessageFields {
    fields: BTreeMap<u32, FieldInfo>,
    oneofs: BTreeMap<String, BTreeSet<u32>>,
}

impl MessageFields {
    fn new() -> Self {
        Self {
            fields: Default::default(),
            oneofs: Default::default(),
        }
    }

    pub fn add_field(&mut self, name: &str, tag: u32, repr: Box<dyn Display>) {
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

    pub fn add_oneof(&mut self, oneof_name: &str, tags: &[u32]) {
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

struct EnumInfo {
    values: BTreeMap<u32, String>,
}

impl EnumInfo {
    fn new() -> Self {
        Self {
            values: Default::default(),
        }
    }

    /// Add a value to the enumeration.
    pub fn add_value(&mut self, name: &str, value: u32) {
        self.values.insert(value, name.to_owned());
    }
}

struct OneofMessages {
    variants: BTreeMap<u32, (String, MessageFields)>,
}

impl OneofMessages {
    fn new() -> Self {
        Self {
            variants: Default::default(),
        }
    }

    /// Adds a message variant to the oneof.
    pub fn add_message_variant(
        &mut self,
        tag: u32,
        name: &str,
        fields: impl Fn(&mut MessageFields),
    ) {
        let Entry::Vacant(entry) = self.variants.entry(tag) else {
            panic!("multiple variants added with the tag {tag}");
        };
        let (_, msg) = entry.insert((name.to_owned(), MessageFields::new()));
        fields(msg);
    }
}
