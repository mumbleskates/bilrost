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
use core::ops::{Deref, DerefMut};

/// Common trait for interior mutability
trait BorrowGuard<T> {
    type ReadGuard<'a>: Deref<Target = T>
    where
        Self: 'a,
        T: 'a;
    type WriteGuard<'a>: DerefMut<Target = T>
    where
        Self: 'a,
        T: 'a;

    fn get_guarded(&self) -> Self::WriteGuard<'_>;
    fn read_guarded(&self) -> Self::ReadGuard<'_>;
}

#[cfg(feature = "threadsafe-schema")]
mod guard {
    pub(super) use spin::RwLock as Guard;

    impl<T> super::BorrowGuard<T> for Guard<T> {
        type ReadGuard<'a>
            = spin::RwLockReadGuard<'a, T>
        where
            T: 'a;
        type WriteGuard<'a>
            = spin::RwLockWriteGuard<'a, T>
        where
            T: 'a;

        fn read_guarded(&self) -> Self::ReadGuard<'_> {
            self.read()
        }

        fn get_guarded(&self) -> Self::WriteGuard<'_> {
            self.try_write().unwrap()
            //self.write()
        }
    }
}

#[cfg(not(feature = "threadsafe-schema"))]
mod guard {
    pub(super) use core::cell::RefCell as Guard;

    impl<T> super::BorrowGuard<T> for Guard<T> {
        type ReadGuard<'a>
            = core::cell::Ref<'a, T>
        where
            T: 'a;
        type WriteGuard<'a>
            = core::cell::RefMut<'a, T>
        where
            T: 'a;

        fn read_guarded(&self) -> Self::ReadGuard<'_> {
            self.borrow()
        }

        fn get_guarded(&self) -> Self::WriteGuard<'_> {
            self.borrow_mut()
        }
    }
}

use guard::Guard;

pub trait PopulateSchema {
    /// Registers a specific message type. May shortcut if this method has already been invoked
    /// for the same type.
    fn register_message<M: Any + ?Sized>(&self, name: &str, fields: impl Fn(&mut MessageFields));

    /// Registers a specific type as an enumeration.
    fn register_enumeration<E: Any + ?Sized>(&self, name: &str, fields: impl Fn(&mut EnumInfo));

    /// Registers the message variants of a oneof enum. A oneof may have several variants that each
    /// encode as messages, and they should each be registered on the `OneofMessages` value
    /// provided to the `variants` closure.
    fn register_oneof_messages<T: Any + ?Sized>(
        &self,
        name: &str,
        variants: impl Fn(&mut OneofMessages),
    );

    /// Registers that the type W wraps the message type M and should be treated as equivalent.
    fn register_message_wrapper<W: Any + ?Sized, M: Any + ?Sized>(&self);

    /// Name for a type that disambiguates where it can be found in the entire schema output. The
    /// output of this function may differ as more types are added to the schema, so this should
    /// only be called when the whole schema is being rendered; see `make_lazy_repr`.
    fn type_reference<M: Any + ?Sized>(&self) -> String;

    /// Name for a sub-type (message variant of a oneof enum) that disambiguates where it can be
    /// found in the entire schema output. The output of this function may differ as more types are
    /// added to the schema, so this should only be called when the whole schema is being rendered;
    /// see `make_lazy_repr`.
    fn subtype_reference<M: Any + ?Sized, const TAG: u32>(&self) -> String;

    /// Returns a lazily-evaluated repr using the given closure. The closure won't be invoked until
    /// the schema is displayed, so `schema.type_reference::<T>()` can correctly specify the name
    /// of the type `T`.
    fn make_lazy_repr<A, D>(&self, a: A) -> Box<dyn Display>
    where
        A: 'static + Fn(&Schema) -> D,
        D: Display;
}

/// A collected internally-complete set of message definitions.
#[derive(Clone)]
pub struct Schema(Arc<MessageSet>);

#[derive(Default)]
struct MessageSet {
    types: Guard<BTreeMap<TypeId, Arc<Guard<TypeInfo>>>>,
    subtypes: Guard<BTreeMap<TypeId, Arc<Guard<OneofMessages>>>>,
    type_index: Guard<BTreeMap<(TypeId, Option<u32>), usize>>,
    message_wrappers: Guard<BTreeMap<TypeId, TypeId>>,
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
        Self(MessageSet::default().into())
    }

    fn wrapped_type_id(&self, type_id: TypeId) -> TypeId {
        let mut effective_id = type_id;
        let wrappers = self.0.message_wrappers.read_guarded();
        while let Some(&wrapped_id) = wrappers.get(&effective_id) {
            effective_id = wrapped_id;
        }
        effective_id
    }
}

impl PopulateSchema for Schema {
    fn register_message<M: Any + ?Sized>(&self, name: &str, fields: impl Fn(&mut MessageFields)) {
        let ty_id = TypeId::of::<M>();
        // First check by a read-only lock whether the type is already registered
        if self.0.types.read_guarded().contains_key(&ty_id) {
            return;
        }
        let info = match self.0.types.get_guarded().entry(ty_id) {
            Entry::Vacant(entry) => entry
                .insert(Arc::new(Guard::new(TypeInfo::Message(MessageFields::new(
                    name,
                )))))
                .clone(),
            Entry::Occupied(_) => return, // already registered by a race
        };

        // Only after we've inserted the message info into the map do we populate its fields. This
        // ensures that we are no longer holding the lock on `types` so other types can be
        // recursively registered.
        let mut info_ref = info.get_guarded();
        let TypeInfo::Message(msg) = info_ref.deref_mut() else {
            unreachable!();
        };
        fields(msg)
    }

    fn register_enumeration<E: Any + ?Sized>(&self, name: &str, fields: impl Fn(&mut EnumInfo)) {
        let ty_id = TypeId::of::<E>();
        // First check by a read-only lock whether the type is already registered
        if self.0.types.read_guarded().contains_key(&ty_id) {
            return;
        }
        let info = match self.0.types.get_guarded().entry(ty_id) {
            Entry::Vacant(entry) => entry
                .insert(Arc::new(Guard::new(TypeInfo::Enum(EnumInfo::new(name)))))
                .clone(),
            Entry::Occupied(_) => return, // already registered
        };

        // Only after we've inserted the enum info into the map do we populate its fields. This
        // ensures that we are no longer holding the lock on `types` so other types can be
        // recursively registered.
        let mut info_ref = info.get_guarded();
        let TypeInfo::Enum(enum_info) = info_ref.deref_mut() else {
            unreachable!();
        };
        fields(enum_info)
    }

    fn register_oneof_messages<T: Any + ?Sized>(
        &self,
        name: &str,
        variants: impl Fn(&mut OneofMessages),
    ) {
        // First check by a read-only lock whether the type is already registered
        if self
            .0
            .subtypes
            .read_guarded()
            .contains_key(&TypeId::of::<T>())
        {
            return;
        }
        let info = match self.0.subtypes.get_guarded().entry(TypeId::of::<T>()) {
            Entry::Vacant(entry) => entry
                .insert(Arc::new(Guard::new(OneofMessages::new(name))))
                .clone(),
            Entry::Occupied(_) => return, // already registered
        };

        // Only after we've inserted the oneof info into the map do we populate its variants. This
        // ensures that we are no longer holding the lock on `types` so other types can be
        // recursively registered.
        let mut info_ref = info.get_guarded();
        variants(info_ref.deref_mut())
    }

    fn register_message_wrapper<W: Any + ?Sized, M: Any + ?Sized>(&self) {
        let wrapper_type_id = TypeId::of::<W>();
        let referenced_type_id = TypeId::of::<M>();
        // Dereference what "M" is already declared to wrap
        let root_type_id = self.wrapped_type_id(referenced_type_id);

        // if "M" already wraps "W", don't do anything. This way we can never create infinite loops
        if root_type_id == wrapper_type_id {
            return;
        }

        self.0
            .message_wrappers
            .get_guarded()
            .insert(wrapper_type_id, referenced_type_id);
    }

    fn type_reference<M: Any + ?Sized>(&self) -> String {
        let effective_id = self.wrapped_type_id(TypeId::of::<M>());
        let types = self.0.types.read_guarded();
        let Some(type_info) = types.get(&effective_id) else {
            return format!(
                "<!! type {ty_name:?} is not registered as a message !!>",
                ty_name = type_name::<M>(),
            );
        };
        let type_info = type_info.read_guarded();
        let name = type_info.name();
        if let Some(ordinal) = self.0.type_index.read_guarded().get(&(effective_id, None)) {
            format!("{name} [{ordinal}]")
        } else {
            format!("{name} <!! no ordinal for {effective_id:?} !!>")
        }
    }

    fn subtype_reference<M: Any + ?Sized, const TAG: u32>(&self) -> String {
        // TODO: this is a placeholder, we want to use the type's ordinal after they're organized
        let id = self.wrapped_type_id(TypeId::of::<M>());
        let subtypes = self.0.subtypes.read_guarded();
        let Some(oneof_info) = subtypes.get(&id) else {
            return format!(
                "<!! type {ty_name:?} is not registered as a oneof with subtypes !!>",
                ty_name = type_name::<M>(),
            );
        };
        let oneof_info = oneof_info.read_guarded();
        let Some(message) = oneof_info.variants.get(&TAG) else {
            return format!(
                "<!! type {ty_name:?} does not have a registered variant with tag {TAG} !!>",
                ty_name = type_name::<M>()
            );
        };
        let name = &oneof_info.oneof_name;
        let variant_name = &message.message_name;
        if let Some(ordinal) = self.0.type_index.read_guarded().get(&(id, Some(TAG))) {
            format!("{name}::{variant_name} [{ordinal}]")
        } else {
            format!("{name}::{variant_name} <!! no ordinal for {id:?} !!>")
        }
    }

    fn make_lazy_repr<A, D>(&self, a: A) -> Box<dyn Display>
    where
        A: 'static + Fn(&Schema) -> D,
        D: Display,
    {
        struct LazyRepr<A> {
            schema: Schema,
            func: A,
        }

        impl<A, D: Display> Display for LazyRepr<A>
        where
            A: 'static + Fn(&Schema) -> D,
        {
            fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
                (self.func)(&self.schema).fmt(f)
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
        let types = self.0.types.read_guarded();
        let subtypes = self.0.subtypes.read_guarded();

        #[derive(PartialEq, Eq, PartialOrd, Ord)]
        struct TypeEntry {
            type_name: String,
            subtype_name: Option<String>,
            type_id: TypeId,
            subtype_tag: Option<u32>,
        }

        let mut ordered = BTreeSet::new();
        for (&type_id, info) in types.iter() {
            let info = info.read_guarded();
            ordered.insert(TypeEntry {
                type_name: info.name().to_owned(),
                subtype_name: None,
                type_id,
                subtype_tag: None,
            });
        }
        for (&type_id, info) in subtypes.iter() {
            let info = info.read_guarded();
            for (&subtype_tag, subinfo) in info.variants.iter() {
                ordered.insert(TypeEntry {
                    type_name: info.oneof_name.clone(),
                    subtype_name: Some(subinfo.message_name.clone()),
                    type_id,
                    subtype_tag: Some(subtype_tag),
                });
            }
        }
        // Update the index so that `type_reference` and `subtype_reference` will show the correct
        // numbers during the render
        {
            let mut index = self.0.type_index.get_guarded();

            *index = ordered
                .iter()
                .map(|entry| (entry.type_id, entry.subtype_tag))
                .zip(1..)
                .collect();
        }

        let mut first_print = true;
        for (
            TypeEntry {
                type_id,
                subtype_tag,
                ..
            },
            ordinal,
        ) in ordered.iter().zip(1..)
        {
            if first_print {
                first_print = false;
            } else {
                writeln!(f, "")?;
            }
            match subtype_tag {
                None => {
                    write!(
                        f,
                        "[{ordinal}] {type_info}",
                        type_info = types.get(type_id).unwrap().read_guarded(),
                    )?;
                }
                Some(subtype_tag) => {
                    write!(f, "[{ordinal}] ")?;
                    subtypes
                        .get(type_id)
                        .unwrap()
                        .read_guarded()
                        .display_variant(f, *subtype_tag)?;
                }
            }
        }
        Ok(())
    }
}

enum TypeInfo {
    Message(MessageFields),
    Enum(EnumInfo),
}

impl TypeInfo {
    fn name(&self) -> &str {
        match self {
            TypeInfo::Message(message_fields) => &message_fields.message_name,
            TypeInfo::Enum(enum_info) => &enum_info.enum_name,
        }
    }
}

impl Display for TypeInfo {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        match self {
            TypeInfo::Message(ty_msg) => {
                ty_msg.display(f, None)?;
            }
            TypeInfo::Enum(ty_enum) => {
                writeln!(f, "enumeration {name} {{", name = ty_enum.enum_name)?;
                for (value, name) in &ty_enum.values {
                    writeln!(f, "    {value}: {name},")?;
                }
                writeln!(f, "}}")?;
            }
        }
        Ok(())
    }
}

/// Collected information about a specific message type and its fields.
pub struct MessageFields {
    message_name: String,
    fields: BTreeMap<u32, FieldInfo>,
    oneofs: BTreeMap<String, BTreeSet<u32>>,
}

impl MessageFields {
    fn new(name: &str) -> Self {
        Self {
            message_name: name.to_owned(),
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
            panic!("message {name} registered multiple fields with tag {tag}");
        };
    }

    pub fn add_oneof(&mut self, oneof_name: &str, tags: &[u32]) {
        let tag_set = tags.into_iter().copied().collect();
        for (existing_oneof_name, existing_set) in &self.oneofs {
            if let Some(conflicting_tag) = existing_set.intersection(&tag_set).next() {
                panic!(
                    "message registered conflicting oneof sets: message declared oneofs which \
                    both contain tag {conflicting_tag}: {oneof_name:?}={tag_set:?} and \
                    {existing_oneof_name:?}={existing_set:?}",
                );
            }
        }
        if let Some(conflicting_name) = self.oneofs.insert(oneof_name.to_owned(), tag_set) {
            panic!("message registered multiple oneof sets with the name {conflicting_name:?}");
        }
    }

    fn display(&self, f: &mut Formatter<'_>, oneof_name: Option<&str>) -> core::fmt::Result {
        // TODO: display info about oneofs in the message
        write!(f, "message ")?;
        if let Some(oneof_name) = oneof_name {
            write!(f, "{oneof_name}::")?;
        }
        writeln!(f, "{name} {{", name = self.message_name)?;
        if self.fields.is_empty() {
            writeln!(f, "    // empty")?;
        } else {
            for (tag, field) in &self.fields {
                writeln!(
                    f,
                    "    {tag}: {field_name} ({repr}),",
                    field_name = field.name,
                    repr = field.repr,
                )?;
            }
        }
        writeln!(f, "}}")?;
        Ok(())
    }
}

struct FieldInfo {
    name: String,
    repr: Box<dyn Display>,
}

pub struct EnumInfo {
    enum_name: String,
    values: BTreeMap<u32, String>,
}

impl EnumInfo {
    fn new(name: &str) -> Self {
        Self {
            enum_name: name.to_owned(),
            values: Default::default(),
        }
    }

    /// Add a value to the enumeration.
    pub fn add_value(&mut self, name: &str, value: u32) {
        self.values.insert(value, name.to_owned());
    }
}

pub struct OneofMessages {
    oneof_name: String,
    variants: BTreeMap<u32, MessageFields>,
}

impl OneofMessages {
    fn new(name: &str) -> Self {
        Self {
            oneof_name: name.to_owned(),
            variants: Default::default(),
        }
    }

    /// Adds a message variant to the oneof.
    pub fn add_message_variant(
        &mut self,
        name: &str,
        tag: u32,
        fields: impl Fn(&mut MessageFields),
    ) {
        let Entry::Vacant(entry) = self.variants.entry(tag) else {
            panic!("multiple variants added with the tag {tag}");
        };
        let msg = entry.insert(MessageFields::new(name));
        fields(msg);
    }

    fn display_variant(&self, f: &mut Formatter<'_>, tag: u32) -> core::fmt::Result {
        self.variants
            .get(&tag)
            .expect("tried to display a nonexistent variant")
            .display(f, Some(&self.oneof_name))
    }
}

/// Representation of a value type T when encoded by the encoding E.
pub trait ValueRepr<E, T: ?Sized> {
    fn repr(schema: &Schema) -> Box<dyn Display>;
}

/// Representation of a field type T when encoded by the encoding E.
pub trait FieldRepr<E, T: ?Sized> {
    fn repr(schema: &Schema) -> Box<dyn Display>;
}

/// Ability of a message to register its fields with a schema.
pub trait RegisterFields {
    fn register(schema: &Schema);
}

/// Ability of a oneof to register its fields inline with the outer struct's fields.
pub trait AddOneofFields {
    fn add_fields(schema: &Schema, fields: &mut MessageFields, field_name: Option<&str>);
}
