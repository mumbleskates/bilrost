use alloc::boxed::Box;
use alloc::fmt::Display;
use alloc::rc::Rc;
use core::any::Any;

/// Receptacle for a full schema that can record the schemas of many messages.
pub trait SchemaSet {
    // TODO: notes for a type, like what it should be called etc.
    /// Visits a specific message type. May shortcut if this method has already been invoked
    /// elsewhere for the same type.
    fn visit_message<M: MessageSchema>(&self);
}

/// Receptacle for fields in a single specific message type.
pub trait FieldSet {
    // TODO: notes for the whole type, like whether it's a oneof or whatever
    /// Adds a field to the type.
    fn add_field(&self, name: &str, tag: u32, repr: Box<dyn Display>);
}

/// A message type that can report the fields in its schema
pub trait MessageSchema: Any {
    fn register(schema: Rc<impl SchemaSet + FieldSet>);
}

/// Trait for an encoding to describe its representation.
pub trait ValueSchema<E, T> {
    fn repr() -> Box<dyn Display>;
}

/// Translation standin; proxies Display when the value schema has a description.
pub struct Repr<E, T>;

impl<E, T> Display for Repr<E, T>
    where (): ValueSchema<E, T>,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        <() as ValueSchema<E, T>>::repr().fmt(f)
    }
}
