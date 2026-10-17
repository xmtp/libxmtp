// Records for descriptors, values and writes. Included from `metadata.rs` to
// keep UniFFI module paths stable.

/// The type of a scalar field or of a map value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum MetadataScalarType {
    Bytes,
    String,
}

/// The type of a map or set key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum MetadataKeyType {
    Bytes,
    InboxId,
}

/// The shape of a field's value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum MetadataComponentType {
    Bytes,
    String,
    Map {
        key_type: MetadataKeyType,
        value_type: MetadataScalarType,
    },
    Set {
        key_type: MetadataKeyType,
    },
    /// A type tag this SDK does not know. Its values can be neither read nor
    /// written.
    Unknown {
        tag: i32,
    },
}

/// A member policy for inserting, updating or deleting a field.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum MetadataPolicy {
    Base(MetadataBasePolicy),
    /// Allows a write when every policy allows it.
    And(Vec<MetadataPolicy>),
    /// Allows a write when any policy allows it.
    Any(Vec<MetadataPolicy>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum MetadataBasePolicy {
    Allow,
    Deny,
    AllowIfAdmin,
    AllowIfSuperAdmin,
    AllowIfSelfOrNonMember,
    /// A policy tag this SDK does not know. Tag 0 is an unspecified or
    /// missing policy.
    Unknown {
        tag: i32,
    },
}

/// A field's insert, update and delete policies.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ComponentPermissions {
    pub insert: MetadataPolicy,
    pub update: MetadataPolicy,
    pub delete: MetadataPolicy,
}

/// A field of a conversation, with its committed type and policies.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct MetadataFieldDescriptor {
    pub field: MetadataFieldRef,
    pub component_type: MetadataComponentType,
    pub permissions: ComponentPermissions,
    /// True for a map keyed by inbox ID, which holds one value per inbox.
    pub is_user_field: bool,
}

/// A field that a backend offers apps. A new conversation registers it when
/// `in_groups` or `in_dms` selects the conversation kind.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ApplicationComponentDefinition {
    pub component_id: u16,
    pub name: String,
    pub component_type: MetadataComponentType,
    pub permissions: ComponentPermissions,
    pub in_groups: bool,
    pub in_dms: bool,
}

/// A scalar value or map value.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum FieldValue {
    Bytes(Vec<u8>),
    String(String),
}

/// A map or set key.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum FieldKey {
    Bytes(Vec<u8>),
    /// The inbox ID text in lowercase hex, the form reads return; any other
    /// spelling fails `InvalidArgument`.
    InboxId(String),
}

/// A field value.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum MetadataValue {
    Scalar(FieldValue),
    /// Entries in key order.
    Map(Vec<MapEntry>),
    /// Keys in key order.
    Set(Vec<FieldKey>),
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct MapEntry {
    pub key: FieldKey,
    pub value: FieldValue,
}

/// A field and its value. The value is absent when the conversation holds
/// none.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct MetadataFieldValue {
    pub field: MetadataFieldRef,
    pub value: Option<MetadataValue>,
}

/// One inbox's value of a user field.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct UserFieldValue {
    pub field: MetadataFieldRef,
    pub value: FieldValue,
}

/// Sets the caller's own value of a user field, or clears it when `value`
/// is absent.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct UserFieldUpdate {
    pub field: MetadataFieldRef,
    pub value: Option<FieldValue>,
}

/// One key change to a map field.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum MapMutation {
    Insert(FieldKey, FieldValue),
    Update(FieldKey, FieldValue),
    Delete(FieldKey),
}

/// One key change to a set field.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum SetMutation {
    Insert(FieldKey),
    Delete(FieldKey),
    /// Deletes the key whose TLS serialization has this 32-byte SHA-256
    /// hash.
    DeleteByHash(Vec<u8>),
}

/// A write to one field. A map or set delta applies atomically: inserting a
/// present key, or updating or deleting an absent key, rejects the whole
/// write.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ComponentMutation {
    /// Sets a scalar field.
    Replace(FieldValue),
    /// Removes the whole value.
    Remove,
    MapDelta(Vec<MapMutation>),
    SetDelta(Vec<SetMutation>),
}
