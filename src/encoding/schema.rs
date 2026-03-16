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
use core::marker::PhantomData;

/// Receptacle for a full schema that can record the schemas of many messages.
///
/// This trait is usable from a const reference with interior mutability because when messages
/// register the appearance of their fields' reprs they will sometimes capture references to the
/// whole schema so that they can get the name of a type. We want the overall schema not to be
/// frozen in place as we collect these reprs even as their potential output changes and more types
/// are registered.
pub trait SchemaSet: Clone {
    // TODO: notes for a type, like what it should be called etc.
    /// Visits a specific message type. May shortcut if this method has already been invoked
    /// elsewhere for the same type.
    fn visit_message<M: MessageSchema>(&self, name: &str);
    // TODO: way to get a name for a message that refers to where it's printed out in the schema
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
    fn register(fields: &mut impl FieldSet, schema: &impl SchemaSet);
}

/// Trait for an encoding to describe its representation.
pub trait ValueSchema<E, T> {
    fn repr() -> Box<dyn Display>;
}

/// Translation standin; proxies Display when the value schema has a description.
pub struct Repr<E, T>(PhantomData<(E, T)>);

impl<E, T> Repr<E, T> {
    pub fn new() -> Self {
        Self(PhantomData)
    }
}

impl<E, T> Display for Repr<E, T>
where
    (): ValueSchema<E, T>,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        <() as ValueSchema<E, T>>::repr().fmt(f)
    }
}

/// A collected internally-complete set of message definitions.
pub struct Schema {
    types: RefCell<BTreeMap<TypeId, MessageInfo>>,
    // TODO: pre-organize the types (or their names) for disambiguation and which index they will
    //  be found at in the final printout
}

impl Schema {
    pub fn new() -> Self {
        Self {
            types: Default::default(),
        }
    }
}

impl SchemaSet for Rc<Schema> {
    fn visit_message<M: MessageSchema>(&self, name: &str) {
        let mut types = self.types.borrow_mut();
        match types.entry(TypeId::of::<M>()) {
            Entry::Vacant(entry) => {
                let field_set = entry.insert(MessageInfo::new(name));
                M::register(field_set, self);
            }
            Entry::Occupied(mut entry) => {
                entry.get_mut().add_name(name);
                // type was already registered previously
            }
        }
    }
}

impl Display for Schema {
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
            1 => format!("{name:?}", name = self.names.first().unwrap()),
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
