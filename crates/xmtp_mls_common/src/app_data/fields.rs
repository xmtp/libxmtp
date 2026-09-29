//! Typed metadata fields: the developer-facing view of a group's AppData
//! dictionary.
//!
//! `xmtp_mls`'s `MlsGroup` metadata-field methods (`metadata_fields`,
//! `metadata_values`, `user_data`, `update_metadata_field`,
//! `update_user_data`, …) read and write application and public well-known
//! components here. The bindings do not expose them yet. Everything is a pure function of one committed dictionary: a
//! [`FieldSnapshot`] lists the fields its registry describes, decodes their
//! values, and encodes writes. Nothing here consults the network or storage.
//!
//! - [`MetadataFieldRef`] names a field: its component ID plus a label. The
//!   well-known constants ([`MetadataFieldRef::GROUP_NAME`], …) cover the
//!   public well-known fields. The core resolves a ref by ID only.
//! - [`MetadataFieldDescriptor`] describes a listed field: its ref,
//!   [`MetadataComponentType`], and committed permissions.
//! - [`FieldValue`], [`FieldKey`], and [`MetadataValue`] carry typed values;
//!   [`MetadataFieldValue`] and [`UserFieldValue`] pair them with a ref.
//! - [`ComponentMutation`] and [`UserFieldUpdate`] describe writes.
//!   [`FieldSnapshot::field_write`] and [`FieldSnapshot::user_data_writes`]
//!   encode them as [`FieldWrite`]s, and [`FieldSnapshot::resolve_writes`]
//!   turns those into `AppDataUpdate` operations against a dictionary.
//!
//! ## Authority
//!
//! The committed registry is the only authority for a field's existence and
//! policy. A well-known field's type is fixed by the protocol ([`component_type`]);
//! an application field's type is its registry entry's tag. The backend
//! catalogue only supplies the label of an application field and so the name
//! that [`FieldSnapshot::field`] matches. A registry entry that does not
//! decode is preserved in the dictionary but is not a field.

use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
};

use openmls::{extensions::AppDataDictionary, messages::proposals::AppDataUpdateOperation};
use tls_codec::{Deserialize, Serialize, Size, VLBytes};
use xmtp_configuration::ApplicationComponentDefinition;
use xmtp_proto::xmtp::mls::message_contents::{ComponentPermissions, ComponentType};

use crate::{
    app_data::{
        component_id::ComponentId,
        component_registry::ComponentRegistry,
        component_source::{ComponentSourceError, apply_app_data_update_payload, component_type},
    },
    inbox_id::InboxId,
    tls_map::{TlsMap, TlsMapDelta},
    tls_set::{TlsKeyHash, TlsSet, TlsSetDelta},
};

/// Names a metadata field: its component ID and, when one exists, its
/// well-known or backend catalogue name.
///
/// The name is only a label. Every lookup resolves a ref by
/// `component_id`, so a ref built on a client with another backend
/// snapshot still names the same field. `==` compares the label too;
/// compare `component_id`s to ask whether two refs name one field.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MetadataFieldRef {
    pub component_id: ComponentId,
    pub name: Option<Cow<'static, str>>,
}

impl MetadataFieldRef {
    pub const GROUP_NAME: Self = Self::well_known(ComponentId::GROUP_NAME);
    pub const GROUP_DESCRIPTION: Self = Self::well_known(ComponentId::GROUP_DESCRIPTION);
    pub const GROUP_IMAGE_URL: Self = Self::well_known(ComponentId::GROUP_IMAGE_URL);
    pub const GROUP_IMAGE: Self = Self::well_known(ComponentId::GROUP_IMAGE);
    pub const APP_DATA: Self = Self::well_known(ComponentId::APP_DATA);
    pub const USER_DISPLAY_NAME: Self = Self::well_known(ComponentId::USER_DISPLAY_NAME);
    pub const MESSAGE_DISAPPEAR_FROM_NS: Self =
        Self::well_known(ComponentId::MESSAGE_DISAPPEAR_FROM_NS);
    pub const MESSAGE_DISAPPEAR_IN_NS: Self =
        Self::well_known(ComponentId::MESSAGE_DISAPPEAR_IN_NS);

    /// The well-known fields an app may see, when the group registers them.
    /// Registry, role, membership, protocol, signer, and bootstrap
    /// components are internal.
    // implements: META-069
    pub const PUBLIC_WELL_KNOWN: [Self; 8] = [
        Self::GROUP_NAME,
        Self::GROUP_DESCRIPTION,
        Self::GROUP_IMAGE_URL,
        Self::GROUP_IMAGE,
        Self::APP_DATA,
        Self::USER_DISPLAY_NAME,
        Self::MESSAGE_DISAPPEAR_FROM_NS,
        Self::MESSAGE_DISAPPEAR_IN_NS,
    ];

    /// An unnamed ref to `component_id`.
    pub const fn new(component_id: ComponentId) -> Self {
        Self {
            component_id,
            name: None,
        }
    }

    /// The ref of a well-known ID, named from [`ComponentId::WELL_KNOWN_NAMES`].
    /// Fails to compile for an ID without a well-known name.
    const fn well_known(id: ComponentId) -> Self {
        let names = ComponentId::WELL_KNOWN_NAMES;
        let mut i = 0;
        while names[i].0.as_u16() != id.as_u16() {
            i += 1;
        }
        Self {
            component_id: id,
            name: Some(Cow::Borrowed(names[i].1)),
        }
    }
}

/// The type of a scalar field or of a map value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetadataScalarType {
    Bytes,
    String,
}

/// The type of a map or set key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetadataKeyType {
    Bytes,
    InboxId,
}

/// The shape of a field's value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    /// A registry type tag this build does not know. Its values can be
    /// neither read nor written.
    Unknown {
        tag: i32,
    },
}

impl MetadataComponentType {
    /// The shape of a registry `component_type` tag.
    pub fn from_tag(tag: i32) -> Self {
        use MetadataKeyType as K;
        use MetadataScalarType as S;
        match ComponentType::try_from(tag) {
            Ok(ComponentType::Bytes) => Self::Bytes,
            Ok(ComponentType::String) => Self::String,
            Ok(ComponentType::TlsMapBytesBytes) => Self::map(K::Bytes, S::Bytes),
            Ok(ComponentType::TlsMapInboxIdBytes) => Self::map(K::InboxId, S::Bytes),
            Ok(ComponentType::TlsMapInboxIdString) => Self::map(K::InboxId, S::String),
            Ok(ComponentType::TlsSetBytes) => Self::Set { key_type: K::Bytes },
            Ok(ComponentType::TlsSetInboxId) => Self::Set {
                key_type: K::InboxId,
            },
            Ok(ComponentType::Unspecified) | Err(_) => Self::Unknown { tag },
        }
    }

    /// The registry tag of this shape, or `None` for a shape no tag
    /// describes (a map with byte keys and string values).
    pub fn tag(self) -> Option<ComponentType> {
        use MetadataKeyType as K;
        use MetadataScalarType as S;
        Some(match self {
            Self::Bytes => ComponentType::Bytes,
            Self::String => ComponentType::String,
            Self::Map {
                key_type: K::Bytes,
                value_type: S::Bytes,
            } => ComponentType::TlsMapBytesBytes,
            Self::Map {
                key_type: K::InboxId,
                value_type: S::Bytes,
            } => ComponentType::TlsMapInboxIdBytes,
            Self::Map {
                key_type: K::InboxId,
                value_type: S::String,
            } => ComponentType::TlsMapInboxIdString,
            Self::Set { key_type: K::Bytes } => ComponentType::TlsSetBytes,
            Self::Set {
                key_type: K::InboxId,
            } => ComponentType::TlsSetInboxId,
            Self::Map {
                key_type: K::Bytes,
                value_type: S::String,
            }
            | Self::Unknown { .. } => return None,
        })
    }

    const fn map(key_type: MetadataKeyType, value_type: MetadataScalarType) -> Self {
        Self::Map {
            key_type,
            value_type,
        }
    }
}

/// A field listed by [`FieldSnapshot::fields`].
#[derive(Debug, Clone, PartialEq)]
pub struct MetadataFieldDescriptor {
    pub field: MetadataFieldRef,
    pub component_type: MetadataComponentType,
    /// The committed insert, update, and delete policies.
    pub permissions: ComponentPermissions,
}

impl MetadataFieldDescriptor {
    /// Whether this is a per-user field: a map keyed by inbox ID.
    // implements: META-069
    pub fn is_user_field(&self) -> bool {
        matches!(
            self.component_type,
            MetadataComponentType::Map {
                key_type: MetadataKeyType::InboxId,
                ..
            }
        )
    }

    /// The scalar type of a user field's values.
    fn user_value_type(&self) -> Option<MetadataScalarType> {
        match self.component_type {
            MetadataComponentType::Map {
                key_type: MetadataKeyType::InboxId,
                value_type,
            } => Some(value_type),
            _ => None,
        }
    }
}

/// A scalar value or map value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldValue {
    Bytes(Vec<u8>),
    String(String),
}

/// A map or set key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldKey {
    Bytes(Vec<u8>),
    InboxId(InboxId),
}

/// A decoded field value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MetadataValue {
    Scalar(FieldValue),
    /// Entries in key order.
    Map(Vec<MapEntry>),
    /// Keys in key order.
    Set(Vec<FieldKey>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapEntry {
    pub key: FieldKey,
    pub value: FieldValue,
}

/// A field and its value, absent when the group holds none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetadataFieldValue {
    pub field: MetadataFieldRef,
    pub value: Option<MetadataValue>,
}

/// One inbox's value of a user field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserFieldValue {
    pub field: MetadataFieldRef,
    pub value: FieldValue,
}

/// Set (`Some`) or clear (`None`) the caller's own entry of a user field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserFieldUpdate {
    pub field: MetadataFieldRef,
    pub value: Option<FieldValue>,
}

/// One key-level change to a map field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MapMutation {
    Insert(FieldKey, FieldValue),
    Update(FieldKey, FieldValue),
    Delete(FieldKey),
}

/// One key-level change to a set field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetMutation {
    Insert(FieldKey),
    Delete(FieldKey),
    /// Delete the key whose TLS serialization hashes (SHA-256) to this value.
    DeleteByHash([u8; 32]),
}

/// A write to one field. Map and set deltas apply atomically: an Insert of
/// a present key, or an Update or Delete of an absent key, rejects the
/// whole commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComponentMutation {
    /// Set a scalar field.
    Replace(FieldValue),
    /// Remove the whole component.
    Remove,
    MapDelta(Vec<MapMutation>),
    SetDelta(Vec<SetMutation>),
}

/// An encoded write, fixed to the type it was encoded under.
/// [`FieldSnapshot::resolve_writes`] refuses it if the field's type changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldWrite {
    pub component_id: ComponentId,
    pub component_type: ComponentType,
    pub operation: WriteOperation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteOperation {
    /// An `AppDataUpdate::Update` payload.
    Update(Vec<u8>),
    /// Remove the component if present.
    Remove,
    /// Insert or update the writer's own key of an inbox-keyed map with
    /// these value bytes.
    SetOwn(Vec<u8>),
    /// Delete the writer's own key of an inbox-keyed map if present.
    ClearOwn,
}

#[derive(Debug, thiserror::Error)]
pub enum FieldError {
    /// The group's registry lists no field with this ID.
    #[error("no metadata field {0} in this group")]
    UnknownField(ComponentId),
    #[error("metadata field {0} is not a user field")]
    NotUserField(ComponentId),
    #[error("metadata field {0} appears more than once")]
    DuplicateField(ComponentId),
    #[error("metadata field {component_id} has unsupported type tag {tag}")]
    UnsupportedType { component_id: ComponentId, tag: i32 },
    /// A value, key, or mutation does not match the field's type.
    #[error("value does not match the type of metadata field {0}")]
    TypeMismatch(ComponentId),
    /// The field's committed type changed after the write was encoded.
    #[error("metadata field {component_id} changed type from {expected:?} to {actual:?}")]
    TypeChanged {
        component_id: ComponentId,
        expected: ComponentType,
        actual: MetadataComponentType,
    },
    /// The committed registry's policies deny this write.
    #[error("the group's policy denies this write to metadata field {0}")]
    Denied(ComponentId),
    #[error(transparent)]
    Component(#[from] ComponentSourceError),
}

impl From<tls_codec::Error> for FieldError {
    fn from(error: tls_codec::Error) -> Self {
        ComponentSourceError::from(error).into()
    }
}

/// The metadata fields of one dictionary and their values.
pub struct FieldSnapshot<'a> {
    dictionary: Option<&'a AppDataDictionary>,
    registry: ComponentRegistry,
    fields: Vec<MetadataFieldDescriptor>,
}

impl<'a> FieldSnapshot<'a> {
    /// List the fields of `dictionary`'s registry, labelling each
    /// application field with the first `catalogue` definition of its ID
    /// when that definition is valid, as registration does. `None` is a
    /// group without a dictionary.
    // implements: META-069
    pub fn new(
        dictionary: Option<&'a AppDataDictionary>,
        catalogue: &[ApplicationComponentDefinition],
    ) -> Result<Self, FieldError> {
        let registry =
            match dictionary.and_then(|d| d.get(&ComponentId::COMPONENT_REGISTRY.as_u16())) {
                Some(bytes) => ComponentRegistry::from_bytes(bytes).map_err(|e| {
                    ComponentSourceError::MalformedComponentValue {
                        component_id: ComponentId::COMPONENT_REGISTRY,
                        reason: format!("registry decode: {e}"),
                    }
                })?,
                None => ComponentRegistry::new(),
            };
        let fields = registry
            .iter()
            .filter_map(Result::ok)
            .filter_map(|(id, meta)| {
                let (name, component_type) = if id.is_app_range() {
                    let name = catalogue
                        .iter()
                        .find(|d| d.component_id == id.as_u16())
                        .filter(|d| d.validate().is_ok())
                        .map(|d| Cow::Owned(d.name.clone()));
                    (name, MetadataComponentType::from_tag(meta.component_type))
                } else {
                    let known = MetadataFieldRef::PUBLIC_WELL_KNOWN
                        .into_iter()
                        .find(|f| f.component_id == id)?;
                    (known.name, component_type(id)?.into())
                };
                Some(MetadataFieldDescriptor {
                    field: MetadataFieldRef {
                        component_id: id,
                        name,
                    },
                    component_type,
                    permissions: meta.permissions?,
                })
            })
            .collect();
        Ok(Self {
            dictionary,
            registry,
            fields,
        })
    }

    /// The group's fields in component ID order.
    pub fn fields(&self) -> &[MetadataFieldDescriptor] {
        &self.fields
    }

    /// The listed field named `name`. Well-known IDs sort before
    /// application IDs, so a well-known field wins a name clash.
    // implements: META-069
    pub fn field(&self, name: &str) -> Option<&MetadataFieldDescriptor> {
        self.fields
            .iter()
            .find(|f| f.field.name.as_deref() == Some(name))
    }

    /// The listed field with `field`'s ID; its name is ignored.
    pub fn resolve(
        &self,
        field: &MetadataFieldRef,
    ) -> Result<&MetadataFieldDescriptor, FieldError> {
        self.resolve_id(field.component_id)
    }

    fn resolve_id(&self, id: ComponentId) -> Result<&MetadataFieldDescriptor, FieldError> {
        self.fields
            .iter()
            .find(|f| f.field.component_id == id)
            .ok_or(FieldError::UnknownField(id))
    }

    /// The value of `field`, `None` when absent.
    // implements: META-070
    pub fn value(&self, field: &MetadataFieldRef) -> Result<Option<MetadataValue>, FieldError> {
        let descriptor = self.resolve(field)?;
        let id = descriptor.field.component_id;
        self.bytes(id)
            .map(|bytes| decode(id, descriptor.component_type, bytes))
            .transpose()
    }

    /// The values of `fields`, in request order. Each record carries the
    /// listed ref, whatever name the request used.
    // implements: META-070
    pub fn values(
        &self,
        fields: &[MetadataFieldRef],
    ) -> Result<Vec<MetadataFieldValue>, FieldError> {
        fields
            .iter()
            .map(|field| {
                Ok(MetadataFieldValue {
                    field: self.resolve(field)?.field.clone(),
                    value: self.value(field)?,
                })
            })
            .collect()
    }

    /// The value under `key` of the map `field`.
    // implements: META-070
    pub fn map_value(
        &self,
        field: &MetadataFieldRef,
        key: &FieldKey,
    ) -> Result<Option<FieldValue>, FieldError> {
        let descriptor = self.resolve(field)?;
        let id = descriptor.field.component_id;
        match (descriptor.component_type, key) {
            (MetadataComponentType::Unknown { tag }, _) => {
                return Err(FieldError::UnsupportedType {
                    component_id: id,
                    tag,
                });
            }
            (
                MetadataComponentType::Map {
                    key_type: MetadataKeyType::Bytes,
                    ..
                },
                FieldKey::Bytes(_),
            )
            | (
                MetadataComponentType::Map {
                    key_type: MetadataKeyType::InboxId,
                    ..
                },
                FieldKey::InboxId(_),
            ) => {}
            _ => return Err(FieldError::TypeMismatch(id)),
        }
        let Some(MetadataValue::Map(entries)) = self.value(field)? else {
            return Ok(None);
        };
        Ok(entries
            .into_iter()
            .find(|entry| entry.key == *key)
            .map(|entry| entry.value))
    }

    /// Per-inbox values of user fields. `fields = None` selects every user
    /// field in ID order; `inbox_ids = None` selects the `DM_MEMBERS` pair
    /// when present (only a DM holds it) and the
    /// `GROUP_MEMBERSHIP` inboxes otherwise. Every selected inbox has an
    /// entry, empty when it holds no selected value. A field whose value
    /// does not decode under its committed type contributes no values; a
    /// field named twice is refused.
    // implements: META-072
    pub fn user_data(
        &self,
        fields: Option<&[MetadataFieldRef]>,
        inbox_ids: Option<&[InboxId]>,
    ) -> Result<BTreeMap<InboxId, Vec<UserFieldValue>>, FieldError> {
        if let Some(id) =
            fields.and_then(|refs| first_duplicate(refs.iter().map(|f| f.component_id)))
        {
            return Err(FieldError::DuplicateField(id));
        }
        let fields = match fields {
            None => self.fields.iter().filter(|f| f.is_user_field()).collect(),
            Some(refs) => refs
                .iter()
                .map(|field| {
                    let descriptor = self.resolve(field)?;
                    if descriptor.is_user_field() {
                        Ok(descriptor)
                    } else {
                        Err(FieldError::NotUserField(field.component_id))
                    }
                })
                .collect::<Result<Vec<_>, _>>()?,
        };
        let mut data: BTreeMap<InboxId, Vec<UserFieldValue>> = match inbox_ids {
            Some(ids) => ids.iter().map(|id| (*id, Vec::new())).collect(),
            None => self.default_inboxes()?.map(|id| (id, Vec::new())).collect(),
        };
        for descriptor in fields {
            // A registry update may re-type a field over its stored value, so
            // a value can fail to decode; it holds no typed values to return,
            // and must not hide every other field's.
            let entries = match self.value(&descriptor.field) {
                Ok(Some(MetadataValue::Map(entries))) => entries,
                Ok(_) => continue,
                Err(error) => {
                    tracing::warn!(field = %descriptor.field.component_id, %error, "skipping undecodable user field");
                    continue;
                }
            };
            for MapEntry { key, value } in entries {
                if let FieldKey::InboxId(inbox) = key
                    && let Some(values) = data.get_mut(&inbox)
                {
                    values.push(UserFieldValue {
                        field: descriptor.field.clone(),
                        value,
                    });
                }
            }
        }
        Ok(data)
    }

    fn default_inboxes(&self) -> Result<impl Iterator<Item = InboxId>, FieldError> {
        let inboxes: Vec<InboxId> = match self.bytes(ComponentId::DM_MEMBERS) {
            Some(bytes) => TlsSet::<InboxId>::tls_deserialize_exact(bytes)
                .map_err(|e| malformed(ComponentId::DM_MEMBERS, e))?
                .into_iter()
                .collect(),
            None => self
                .bytes(ComponentId::GROUP_MEMBERSHIP)
                .map(TlsMap::<InboxId, VLBytes>::tls_deserialize_exact)
                .transpose()
                .map_err(|e| malformed(ComponentId::GROUP_MEMBERSHIP, e))?
                .into_iter()
                .flat_map(|map| map.into_iter().map(|(inbox, _)| inbox))
                .collect(),
        };
        Ok(inboxes.into_iter())
    }

    /// Encode `mutation` of `field` under the field's committed type.
    // implements: META-071
    pub fn field_write(
        &self,
        field: &MetadataFieldRef,
        mutation: &ComponentMutation,
    ) -> Result<FieldWrite, FieldError> {
        use MetadataComponentType as T;
        use MetadataKeyType as K;
        let descriptor = self.resolve(field)?;
        let id = descriptor.field.component_id;
        let ty = descriptor.component_type;
        let component_type = wire_type(id, ty)?;
        let payload = match (ty, mutation) {
            (_, ComponentMutation::Remove) => {
                return Ok(FieldWrite {
                    component_id: id,
                    component_type,
                    operation: WriteOperation::Remove,
                });
            }
            (T::Bytes, ComponentMutation::Replace(value)) => {
                scalar(MetadataScalarType::Bytes, value).map(|b| Ok(b.to_vec()))
            }
            (T::String, ComponentMutation::Replace(value)) => {
                scalar(MetadataScalarType::String, value).map(|b| Ok(b.to_vec()))
            }
            (
                T::Map {
                    key_type: K::Bytes,
                    value_type,
                },
                ComponentMutation::MapDelta(mutations),
            ) => map_delta(mutations, value_type, bytes_key).map(|d| d.tls_serialize_detached()),
            (
                T::Map {
                    key_type: K::InboxId,
                    value_type,
                },
                ComponentMutation::MapDelta(mutations),
            ) => map_delta(mutations, value_type, inbox_key).map(|d| d.tls_serialize_detached()),
            (T::Set { key_type: K::Bytes }, ComponentMutation::SetDelta(mutations)) => {
                set_delta(mutations, bytes_key).map(|d| d.tls_serialize_detached())
            }
            (
                T::Set {
                    key_type: K::InboxId,
                },
                ComponentMutation::SetDelta(mutations),
            ) => set_delta(mutations, inbox_key).map(|d| d.tls_serialize_detached()),
            _ => None,
        }
        .ok_or(FieldError::TypeMismatch(id))??;
        Ok(FieldWrite {
            component_id: id,
            component_type,
            operation: WriteOperation::Update(payload),
        })
    }

    /// Encode the writer's own-key sets and clears of user fields.
    // implements: META-073
    pub fn user_data_writes(
        &self,
        values: &[UserFieldUpdate],
    ) -> Result<Vec<FieldWrite>, FieldError> {
        if let Some(id) = first_duplicate(values.iter().map(|v| v.field.component_id)) {
            return Err(FieldError::DuplicateField(id));
        }
        values
            .iter()
            .map(|UserFieldUpdate { field, value }| {
                let descriptor = self.resolve(field)?;
                let id = descriptor.field.component_id;
                let value_type = descriptor
                    .user_value_type()
                    .ok_or(FieldError::NotUserField(id))?;
                let operation = match value {
                    Some(value) => WriteOperation::SetOwn(
                        scalar(value_type, value)
                            .ok_or(FieldError::TypeMismatch(id))?
                            .to_vec(),
                    ),
                    None => WriteOperation::ClearOwn,
                };
                Ok(FieldWrite {
                    component_id: id,
                    component_type: wire_type(id, descriptor.component_type)?,
                    operation,
                })
            })
            .collect()
    }

    /// The `AppDataUpdate` operations that carry out `writes` for the
    /// inbox `own`, given the current component values in `values`: this
    /// snapshot's dictionary with any pending updates applied.
    ///
    /// Types and the field list come from this snapshot, which must be the
    /// committed one. Each write is refused if its field is gone
    /// or changed type. A write that changes nothing (a clear of an absent
    /// key, a removal of an absent component) is dropped. Every payload is
    /// applied to its current value, so bounds, UTF-8, and key presence
    /// fail here rather than in the commit.
    // implements: META-071, META-073
    pub fn resolve_writes(
        &self,
        values: Option<&AppDataDictionary>,
        own: InboxId,
        writes: &[FieldWrite],
    ) -> Result<Vec<(ComponentId, AppDataUpdateOperation)>, FieldError> {
        if let Some(id) = first_duplicate(writes.iter().map(|w| w.component_id)) {
            return Err(FieldError::DuplicateField(id));
        }
        writes
            .iter()
            .filter_map(|write| {
                self.resolve_write(values, own, write)
                    .map(|op| op.map(|op| (write.component_id, op)))
                    .transpose()
            })
            .collect()
    }

    fn resolve_write(
        &self,
        values: Option<&AppDataDictionary>,
        own: InboxId,
        write: &FieldWrite,
    ) -> Result<Option<AppDataUpdateOperation>, FieldError> {
        let id = write.component_id;
        let actual = self.resolve_id(id)?.component_type;
        if actual.tag() != Some(write.component_type) {
            return Err(FieldError::TypeChanged {
                component_id: id,
                expected: write.component_type,
                actual,
            });
        }
        let current = values.and_then(|d| d.get(&id.as_u16()));
        let owned = || -> Result<bool, FieldError> {
            Ok(current
                .map(TlsMap::<InboxId, VLBytes>::tls_deserialize_exact)
                .transpose()
                .map_err(|e| malformed(id, e))?
                .is_some_and(|map| map.contains_key(&own)))
        };
        let delta = TlsMapDelta::<InboxId, VLBytes>::new();
        let payload = match &write.operation {
            WriteOperation::Remove => return Ok(current.map(|_| AppDataUpdateOperation::Remove)),
            WriteOperation::Update(payload) => payload.clone(),
            WriteOperation::SetOwn(value) if owned()? => delta
                .update(own, value.as_slice().into())
                .tls_serialize_detached()?,
            WriteOperation::SetOwn(value) => delta
                .insert(own, value.as_slice().into())
                .tls_serialize_detached()?,
            WriteOperation::ClearOwn if owned()? => delta.delete(own).tls_serialize_detached()?,
            WriteOperation::ClearOwn => return Ok(None),
        };
        apply_app_data_update_payload(id, &payload, current, &self.registry)?;
        Ok(Some(AppDataUpdateOperation::Update(payload.into())))
    }

    fn bytes(&self, id: ComponentId) -> Option<&'a [u8]> {
        self.dictionary.and_then(|d| d.get(&id.as_u16()))
    }
}

impl From<ComponentType> for MetadataComponentType {
    fn from(ty: ComponentType) -> Self {
        Self::from_tag(ty as i32)
    }
}

/// The registry tag a write to field `id` of shape `ty` is encoded under.
fn wire_type(id: ComponentId, ty: MetadataComponentType) -> Result<ComponentType, FieldError> {
    ty.tag().ok_or(FieldError::UnsupportedType {
        component_id: id,
        tag: match ty {
            MetadataComponentType::Unknown { tag } => tag,
            _ => ComponentType::Unspecified as i32,
        },
    })
}

fn decode(
    id: ComponentId,
    ty: MetadataComponentType,
    bytes: &[u8],
) -> Result<MetadataValue, FieldError> {
    use MetadataComponentType as T;
    use MetadataKeyType as K;
    let value = |ty, bytes: VLBytes| field_value(id, ty, bytes.into());
    let tls = |e| malformed(id, e);
    Ok(match ty {
        T::Bytes => {
            MetadataValue::Scalar(field_value(id, MetadataScalarType::Bytes, bytes.into())?)
        }
        T::String => {
            MetadataValue::Scalar(field_value(id, MetadataScalarType::String, bytes.into())?)
        }
        T::Map {
            key_type: K::Bytes,
            value_type,
        } => MetadataValue::Map(
            TlsMap::<VLBytes, VLBytes>::tls_deserialize_exact(bytes)
                .map_err(tls)?
                .into_iter()
                .map(|(key, v)| {
                    Ok(MapEntry {
                        key: FieldKey::Bytes(key.into()),
                        value: value(value_type, v)?,
                    })
                })
                .collect::<Result<_, FieldError>>()?,
        ),
        T::Map {
            key_type: K::InboxId,
            value_type,
        } => MetadataValue::Map(
            TlsMap::<InboxId, VLBytes>::tls_deserialize_exact(bytes)
                .map_err(tls)?
                .into_iter()
                .map(|(key, v)| {
                    Ok(MapEntry {
                        key: FieldKey::InboxId(key),
                        value: value(value_type, v)?,
                    })
                })
                .collect::<Result<_, FieldError>>()?,
        ),
        T::Set { key_type: K::Bytes } => MetadataValue::Set(
            TlsSet::<VLBytes>::tls_deserialize_exact(bytes)
                .map_err(tls)?
                .into_iter()
                .map(|key| FieldKey::Bytes(key.into()))
                .collect(),
        ),
        T::Set {
            key_type: K::InboxId,
        } => MetadataValue::Set(
            TlsSet::<InboxId>::tls_deserialize_exact(bytes)
                .map_err(tls)?
                .into_iter()
                .map(FieldKey::InboxId)
                .collect(),
        ),
        T::Unknown { tag } => {
            return Err(FieldError::UnsupportedType {
                component_id: id,
                tag,
            });
        }
    })
}

fn field_value(
    id: ComponentId,
    ty: MetadataScalarType,
    bytes: Vec<u8>,
) -> Result<FieldValue, FieldError> {
    Ok(match ty {
        MetadataScalarType::Bytes => FieldValue::Bytes(bytes),
        MetadataScalarType::String => {
            FieldValue::String(String::from_utf8(bytes).map_err(|e| {
                ComponentSourceError::MalformedComponentValue {
                    component_id: id,
                    reason: format!("invalid UTF-8: {e}"),
                }
            })?)
        }
    })
}

fn malformed(id: ComponentId, error: tls_codec::Error) -> FieldError {
    ComponentSourceError::MalformedComponentValue {
        component_id: id,
        reason: format!("TLS decode: {error}"),
    }
    .into()
}

/// The bytes of `value` if it has type `ty`.
fn scalar(ty: MetadataScalarType, value: &FieldValue) -> Option<&[u8]> {
    match (ty, value) {
        (MetadataScalarType::Bytes, FieldValue::Bytes(bytes)) => Some(bytes),
        (MetadataScalarType::String, FieldValue::String(s)) => Some(s.as_bytes()),
        _ => None,
    }
}

fn bytes_key(key: &FieldKey) -> Option<VLBytes> {
    match key {
        FieldKey::Bytes(bytes) => Some(bytes.as_slice().into()),
        FieldKey::InboxId(_) => None,
    }
}

fn inbox_key(key: &FieldKey) -> Option<InboxId> {
    match key {
        FieldKey::InboxId(inbox) => Some(*inbox),
        FieldKey::Bytes(_) => None,
    }
}

/// The delta of `mutations`, or `None` if a key or value has the wrong type.
fn map_delta<K>(
    mutations: &[MapMutation],
    value_type: MetadataScalarType,
    key: fn(&FieldKey) -> Option<K>,
) -> Option<TlsMapDelta<K, VLBytes>> {
    mutations
        .iter()
        .try_fold(TlsMapDelta::new(), |delta, mutation| {
            Some(match mutation {
                MapMutation::Insert(k, v) => delta.insert(key(k)?, scalar(value_type, v)?.into()),
                MapMutation::Update(k, v) => delta.update(key(k)?, scalar(value_type, v)?.into()),
                MapMutation::Delete(k) => delta.delete(key(k)?),
            })
        })
}

/// The delta of `mutations`, or `None` if a key has the wrong type.
fn set_delta<K: Serialize + Size>(
    mutations: &[SetMutation],
    key: fn(&FieldKey) -> Option<K>,
) -> Option<TlsSetDelta<K>> {
    mutations
        .iter()
        .try_fold(TlsSetDelta::new(), |delta, mutation| {
            Some(match mutation {
                SetMutation::Insert(k) => delta.insert(key(k)?),
                SetMutation::Delete(k) => delta.remove(key(k)?),
                SetMutation::DeleteByHash(hash) => {
                    delta.remove_by_hash(TlsKeyHash::from_bytes(*hash))
                }
            })
        })
}

fn first_duplicate(ids: impl IntoIterator<Item = ComponentId>) -> Option<ComponentId> {
    let mut seen = BTreeSet::new();
    ids.into_iter().find(|id| !seen.insert(*id))
}

#[cfg(test)]
mod tests {
    use prost::Message;
    use xmtp_configuration::{
        ComponentPermissions as CataloguePermissions, MetadataPolicy as CataloguePolicy,
    };
    use xmtp_proto::xmtp::mls::message_contents::{
        ComponentMetadata, MetadataPolicy,
        metadata_policy::{Kind, MetadataBasePolicy},
    };

    use super::*;

    const STATUS: ComponentId = ComponentId::new(0xC001);
    const SCORES: ComponentId = ComponentId::new(0xC002);
    const FUTURE: ComponentId = ComponentId::new(0xC003);
    const BROKEN: ComponentId = ComponentId::new(0xC004);
    const TAGS: ComponentId = ComponentId::new(0xC005);
    const LINKS: ComponentId = ComponentId::new(0xC006);

    fn inbox(tag: u8) -> InboxId {
        InboxId::from_bytes([tag; 32])
    }

    fn policy(base: MetadataBasePolicy) -> Option<MetadataPolicy> {
        Some(MetadataPolicy {
            kind: Some(Kind::Base(base as i32)),
        })
    }

    fn permissions(base: MetadataBasePolicy) -> ComponentPermissions {
        ComponentPermissions {
            insert_policy: policy(base),
            update_policy: policy(base),
            delete_policy: policy(base),
        }
    }

    /// A registry entry with `tag` and every policy `Allow`.
    fn entry(tag: i32) -> Vec<u8> {
        ComponentMetadata {
            permissions: Some(permissions(MetadataBasePolicy::Allow)),
            component_type: tag,
            external_committer_permissions: None,
        }
        .encode_to_vec()
    }

    fn inbox_map(entries: &[(InboxId, &[u8])]) -> Vec<u8> {
        let mut map = TlsMap::<InboxId, VLBytes>::new();
        for (key, value) in entries {
            map.set(*key, value.to_vec().into());
        }
        map.tls_serialize_detached().unwrap()
    }

    fn inbox_set(inboxes: &[InboxId]) -> Vec<u8> {
        TlsSet::from_iter(inboxes.iter().copied())
            .tls_serialize_detached()
            .unwrap()
    }

    /// A dictionary whose registry holds `registry` verbatim and whose other
    /// components hold `values`.
    fn dictionary(
        registry: &[(ComponentId, Vec<u8>)],
        values: &[(ComponentId, Vec<u8>)],
    ) -> AppDataDictionary {
        let mut entries = TlsMap::<ComponentId, VLBytes>::new();
        for (id, raw) in registry {
            entries.set(*id, raw.clone().into());
        }
        let mut dictionary = AppDataDictionary::new();
        dictionary.insert(
            ComponentId::COMPONENT_REGISTRY.as_u16(),
            entries.tls_serialize_detached().unwrap(),
        );
        for (id, value) in values {
            dictionary.insert(id.as_u16(), value.clone());
        }
        dictionary
    }

    fn tag(ty: ComponentType) -> i32 {
        ty as i32
    }

    /// Members a and b; c held a display name before it left.
    fn group() -> AppDataDictionary {
        dictionary(
            &[
                (
                    ComponentId::GROUP_MEMBERSHIP,
                    entry(tag(ComponentType::TlsMapInboxIdBytes)),
                ),
                (ComponentId::GROUP_NAME, entry(tag(ComponentType::String))),
                (
                    ComponentId::GROUP_DESCRIPTION,
                    entry(tag(ComponentType::String)),
                ),
                (
                    ComponentId::MIN_SUPPORTED_PROTOCOL_VERSION,
                    entry(tag(ComponentType::String)),
                ),
                // A registry tag that disagrees with the protocol type.
                (
                    ComponentId::USER_DISPLAY_NAME,
                    entry(tag(ComponentType::Bytes)),
                ),
                (STATUS, entry(tag(ComponentType::String))),
                (SCORES, entry(tag(ComponentType::TlsMapInboxIdBytes))),
                (FUTURE, entry(99)),
                (BROKEN, vec![0xFF, 0xFF]),
                (TAGS, entry(tag(ComponentType::TlsSetInboxId))),
                (LINKS, entry(tag(ComponentType::TlsMapBytesBytes))),
            ],
            &[
                (
                    ComponentId::GROUP_MEMBERSHIP,
                    inbox_map(&[(inbox(0xA), b""), (inbox(0xB), b"")]),
                ),
                (ComponentId::GROUP_NAME, b"Team".to_vec()),
                (
                    ComponentId::USER_DISPLAY_NAME,
                    inbox_map(&[(inbox(0xA), b"Alix"), (inbox(0xC), b"Carol")]),
                ),
                (SCORES, inbox_map(&[(inbox(0xA), &[7])])),
                (FUTURE, vec![9]),
                (BROKEN, vec![1, 2, 3]),
                (TAGS, inbox_set(&[inbox(0xB)])),
            ],
        )
    }

    /// The backend catalogue of a client whose snapshot disagrees with the
    /// group: it types `STATUS` as bytes and names it after a well-known
    /// field, and its first definition of `SCORES` is invalid, so a later
    /// valid one must not name it either.
    fn catalogue() -> Vec<ApplicationComponentDefinition> {
        let allow = Some(CataloguePolicy::Base(MetadataBasePolicy::Allow as i32));
        let definition = |id: ComponentId, name: &str| ApplicationComponentDefinition {
            component_id: id.as_u16(),
            name: name.into(),
            component_type: tag(ComponentType::Bytes),
            permissions: CataloguePermissions {
                insert: allow.clone(),
                update: allow.clone(),
                delete: allow.clone(),
            },
            in_groups: true,
            in_dms: true,
        };
        vec![
            definition(STATUS, "GROUP_NAME"),
            definition(SCORES, ""),
            definition(SCORES, "scores"),
        ]
    }

    fn snapshot(dictionary: &AppDataDictionary) -> FieldSnapshot<'_> {
        FieldSnapshot::new(Some(dictionary), &catalogue()).unwrap()
    }

    fn named(id: ComponentId, name: &'static str) -> MetadataFieldRef {
        MetadataFieldRef {
            component_id: id,
            name: Some(Cow::Borrowed(name)),
        }
    }

    fn string(s: &str) -> FieldValue {
        FieldValue::String(s.into())
    }

    /// The listing holds the valid application entries and the registered
    /// public well-known fields, in ID order. A well-known field's type
    /// is the protocol's even when its registry tag disagrees; an application
    /// field's type is its registry tag, not the catalogue's; an unknown tag
    /// is `Unknown`; internal and malformed entries are not fields.
    // verifies: META-069
    #[xmtp_common::test(unwrap_try = true)]
    fn fields_describe_the_committed_registry() {
        use MetadataComponentType as T;
        let dictionary = group();
        let fields = snapshot(&dictionary);
        let described: Vec<_> = fields
            .fields()
            .iter()
            .map(|f| (f.field.clone(), f.component_type, f.is_user_field()))
            .collect();
        let string_map = T::Map {
            key_type: MetadataKeyType::InboxId,
            value_type: MetadataScalarType::String,
        };
        let bytes_map = T::Map {
            key_type: MetadataKeyType::InboxId,
            value_type: MetadataScalarType::Bytes,
        };
        assert_eq!(
            described,
            vec![
                (MetadataFieldRef::GROUP_NAME, T::String, false),
                (MetadataFieldRef::GROUP_DESCRIPTION, T::String, false),
                (MetadataFieldRef::USER_DISPLAY_NAME, string_map, true),
                (named(STATUS, "GROUP_NAME"), T::String, false),
                (MetadataFieldRef::new(SCORES), bytes_map, true),
                (MetadataFieldRef::new(FUTURE), T::Unknown { tag: 99 }, false),
                (
                    MetadataFieldRef::new(TAGS),
                    T::Set {
                        key_type: MetadataKeyType::InboxId
                    },
                    false
                ),
                (
                    MetadataFieldRef::new(LINKS),
                    T::Map {
                        key_type: MetadataKeyType::Bytes,
                        value_type: MetadataScalarType::Bytes
                    },
                    false
                ),
            ]
        );
        assert!(
            fields
                .fields()
                .iter()
                .all(|f| f.permissions == permissions(MetadataBasePolicy::Allow))
        );
        assert!(
            FieldSnapshot::new(None, &catalogue())
                .unwrap()
                .fields()
                .is_empty()
        );
    }

    /// A descriptor's permissions are the committed entry's, not the
    /// catalogue's.
    // verifies: META-069
    #[xmtp_common::test(unwrap_try = true)]
    fn permissions_come_from_the_registry() {
        let mut raw =
            ComponentMetadata::decode(entry(tag(ComponentType::String)).as_slice()).unwrap();
        raw.permissions = Some(permissions(MetadataBasePolicy::AllowIfAdmin));
        let dictionary = dictionary(&[(STATUS, raw.encode_to_vec())], &[]);
        let fields = snapshot(&dictionary);
        assert_eq!(
            fields.fields()[0].permissions,
            permissions(MetadataBasePolicy::AllowIfAdmin)
        );
    }

    /// A name lookup prefers the well-known field when an application field
    /// has the same name, and finds nothing for an unlisted name.
    // verifies: META-069
    #[xmtp_common::test(unwrap_try = true)]
    fn name_lookup_prefers_the_well_known_field() {
        let dictionary = group();
        let fields = snapshot(&dictionary);
        assert_eq!(
            fields.field("GROUP_NAME").map(|f| &f.field),
            Some(&MetadataFieldRef::GROUP_NAME)
        );
        assert_eq!(
            fields.field("USER_DISPLAY_NAME").map(|f| &f.field),
            Some(&MetadataFieldRef::USER_DISPLAY_NAME)
        );
        assert!(fields.field("MIN_SUPPORTED_PROTOCOL_VERSION").is_none());
        assert!(fields.field("GROUP_IMAGE").is_none());
        assert!(fields.field("nothing").is_none());
    }

    /// A ref resolves by ID alone: another name or none reads the same
    /// field, and a batch record carries the listed ref.
    // verifies: META-070
    #[xmtp_common::test(unwrap_try = true)]
    fn refs_resolve_by_id() {
        let dictionary = group();
        let fields = snapshot(&dictionary);
        let team = Some(MetadataValue::Scalar(string("Team")));
        for field in [
            MetadataFieldRef::GROUP_NAME,
            MetadataFieldRef::new(ComponentId::GROUP_NAME),
            named(ComponentId::GROUP_NAME, "STATUS"),
        ] {
            assert_eq!(fields.value(&field).unwrap(), team);
            assert_eq!(
                fields.values(std::slice::from_ref(&field)).unwrap(),
                vec![MetadataFieldValue {
                    field: MetadataFieldRef::GROUP_NAME,
                    value: team.clone(),
                }]
            );
        }
        // The catalogue names STATUS "GROUP_NAME", but a ref by that name
        // with STATUS's ID still reads STATUS.
        assert_eq!(fields.value(&named(STATUS, "GROUP_NAME")).unwrap(), None);
    }

    /// A batch read returns one record per ref in request order, typed by
    /// the field, with a missing value absent. An unlisted ref, an internal
    /// field, and an unknown type are errors.
    // verifies: META-070
    #[xmtp_common::test(unwrap_try = true)]
    fn values_are_typed_and_in_request_order() {
        let dictionary = group();
        let fields = snapshot(&dictionary);
        let batch = fields
            .values(&[
                MetadataFieldRef::new(SCORES),
                MetadataFieldRef::GROUP_DESCRIPTION,
                MetadataFieldRef::new(TAGS),
                MetadataFieldRef::USER_DISPLAY_NAME,
            ])
            .unwrap();
        let values: Vec<_> = batch
            .into_iter()
            .map(|v| (v.field.component_id, v.value))
            .collect();
        assert_eq!(
            values,
            vec![
                (
                    SCORES,
                    Some(MetadataValue::Map(vec![MapEntry {
                        key: FieldKey::InboxId(inbox(0xA)),
                        value: FieldValue::Bytes(vec![7]),
                    }]))
                ),
                (ComponentId::GROUP_DESCRIPTION, None),
                (
                    TAGS,
                    Some(MetadataValue::Set(vec![FieldKey::InboxId(inbox(0xB))]))
                ),
                (
                    ComponentId::USER_DISPLAY_NAME,
                    Some(MetadataValue::Map(vec![
                        MapEntry {
                            key: FieldKey::InboxId(inbox(0xA)),
                            value: string("Alix"),
                        },
                        MapEntry {
                            key: FieldKey::InboxId(inbox(0xC)),
                            value: string("Carol"),
                        },
                    ]))
                ),
            ]
        );
        assert!(fields.values(&[]).unwrap().is_empty());
        for (field, id) in [
            (MetadataFieldRef::new(BROKEN), BROKEN),
            (MetadataFieldRef::GROUP_IMAGE, ComponentId::GROUP_IMAGE),
            (
                MetadataFieldRef::new(ComponentId::MIN_SUPPORTED_PROTOCOL_VERSION),
                ComponentId::MIN_SUPPORTED_PROTOCOL_VERSION,
            ),
        ] {
            assert!(matches!(
                fields.values(&[MetadataFieldRef::GROUP_NAME, field]),
                Err(FieldError::UnknownField(unknown)) if unknown == id
            ));
        }
        assert!(matches!(
            fields.value(&MetadataFieldRef::new(FUTURE)),
            Err(FieldError::UnsupportedType {
                component_id: FUTURE,
                tag: 99
            })
        ));
    }

    /// A map read takes a key of the field's key type and returns the
    /// value under it, or none.
    // verifies: META-070
    #[xmtp_common::test(unwrap_try = true)]
    fn map_value_reads_one_key() {
        let dictionary = group();
        let fields = snapshot(&dictionary);
        let names = MetadataFieldRef::USER_DISPLAY_NAME;
        assert_eq!(
            fields
                .map_value(&names, &FieldKey::InboxId(inbox(0xC)))
                .unwrap(),
            Some(string("Carol"))
        );
        assert_eq!(
            fields
                .map_value(&names, &FieldKey::InboxId(inbox(0xB)))
                .unwrap(),
            None
        );
        assert_eq!(
            fields
                .map_value(&MetadataFieldRef::new(LINKS), &FieldKey::Bytes(vec![1]))
                .unwrap(),
            None
        );
        for (field, key) in [
            (names.clone(), FieldKey::Bytes(vec![0xA; 32])),
            (MetadataFieldRef::new(LINKS), FieldKey::InboxId(inbox(0xA))),
            (MetadataFieldRef::GROUP_NAME, FieldKey::Bytes(vec![])),
            (MetadataFieldRef::new(TAGS), FieldKey::InboxId(inbox(0xB))),
        ] {
            assert!(matches!(
                fields.map_value(&field, &key),
                Err(FieldError::TypeMismatch(id)) if id == field.component_id
            ));
        }
        assert!(matches!(
            fields.map_value(&MetadataFieldRef::new(FUTURE), &FieldKey::Bytes(vec![])),
            Err(FieldError::UnsupportedType { .. })
        ));
    }

    /// User data defaults to every user field and every current member,
    /// each with a list (empty when it holds nothing). Explicit inboxes may
    /// name a former member; empty filters select nothing. Explicit fields
    /// must be user fields in the group, each named once.
    // verifies: META-072
    #[xmtp_common::test(unwrap_try = true)]
    fn user_data_selects_fields_and_inboxes() {
        let dictionary = group();
        let fields = snapshot(&dictionary);
        let value = |field: &MetadataFieldRef, value| UserFieldValue {
            field: field.clone(),
            value,
        };
        let scores = MetadataFieldRef::new(SCORES);
        let names = MetadataFieldRef::USER_DISPLAY_NAME;
        assert_eq!(
            fields.user_data(None, None).unwrap(),
            BTreeMap::from([
                (
                    inbox(0xA),
                    vec![
                        value(&names, string("Alix")),
                        value(&scores, FieldValue::Bytes(vec![7])),
                    ]
                ),
                (inbox(0xB), vec![]),
            ])
        );
        assert_eq!(
            fields
                .user_data(
                    Some(std::slice::from_ref(&scores)),
                    Some(&[inbox(0xC), inbox(0xA)])
                )
                .unwrap(),
            BTreeMap::from([
                (inbox(0xA), vec![value(&scores, FieldValue::Bytes(vec![7]))]),
                (inbox(0xC), vec![]),
            ])
        );
        assert_eq!(
            fields.user_data(None, Some(&[inbox(0xC)])).unwrap(),
            BTreeMap::from([(inbox(0xC), vec![value(&names, string("Carol"))])])
        );
        assert_eq!(
            fields.user_data(Some(&[]), None).unwrap(),
            BTreeMap::from([(inbox(0xA), vec![]), (inbox(0xB), vec![])])
        );
        assert!(fields.user_data(None, Some(&[])).unwrap().is_empty());
        assert!(matches!(
            fields.user_data(Some(&[MetadataFieldRef::GROUP_NAME]), None),
            Err(FieldError::NotUserField(ComponentId::GROUP_NAME))
        ));
        assert!(matches!(
            fields.user_data(Some(&[MetadataFieldRef::new(LINKS)]), None),
            Err(FieldError::NotUserField(LINKS))
        ));
        assert!(matches!(
            fields.user_data(Some(&[MetadataFieldRef::new(BROKEN)]), None),
            Err(FieldError::UnknownField(BROKEN))
        ));
        assert!(matches!(
            fields.user_data(Some(&[scores.clone(), named(SCORES, "scores")]), None),
            Err(FieldError::DuplicateField(SCORES))
        ));
    }

    /// A user field whose stored value does not decode under its committed
    /// type, as after a registry update re-types it, yields no user data
    /// instead of failing the read of every other field. A single read of
    /// it still reports the malformed value.
    // verifies: META-072
    #[xmtp_common::test(unwrap_try = true)]
    fn user_data_skips_undecodable_fields() {
        let dictionary = dictionary(
            &[
                (
                    ComponentId::GROUP_MEMBERSHIP,
                    entry(tag(ComponentType::TlsMapInboxIdBytes)),
                ),
                (
                    ComponentId::USER_DISPLAY_NAME,
                    entry(tag(ComponentType::TlsMapInboxIdString)),
                ),
                (STATUS, entry(tag(ComponentType::TlsMapInboxIdString))),
            ],
            &[
                (
                    ComponentId::GROUP_MEMBERSHIP,
                    inbox_map(&[(inbox(0xA), &[1])]),
                ),
                (
                    ComponentId::USER_DISPLAY_NAME,
                    inbox_map(&[(inbox(0xA), b"Alix")]),
                ),
                (STATUS, inbox_map(&[(inbox(0xA), &[0xFF])])),
            ],
        );
        let fields = snapshot(&dictionary);
        let status = named(STATUS, "GROUP_NAME");
        let names = UserFieldValue {
            field: MetadataFieldRef::USER_DISPLAY_NAME,
            value: string("Alix"),
        };
        assert_eq!(
            fields.user_data(None, None)?,
            BTreeMap::from([(inbox(0xA), vec![names])])
        );
        assert_eq!(
            fields.user_data(Some(std::slice::from_ref(&status)), None)?,
            BTreeMap::from([(inbox(0xA), vec![])])
        );
        assert!(fields.value(&status).is_err());
    }

    /// In a DM the default inboxes are the `DM_MEMBERS` pair, whatever the
    /// membership map holds.
    // verifies: META-072
    #[xmtp_common::test(unwrap_try = true)]
    fn user_data_defaults_to_the_dm_pair() {
        let dictionary = dictionary(
            &[(
                ComponentId::USER_DISPLAY_NAME,
                entry(tag(ComponentType::TlsMapInboxIdString)),
            )],
            &[
                (
                    ComponentId::DM_MEMBERS,
                    inbox_set(&[inbox(0xA), inbox(0xD)]),
                ),
                (
                    ComponentId::GROUP_MEMBERSHIP,
                    inbox_map(&[(inbox(0xA), b"")]),
                ),
            ],
        );
        let data = snapshot(&dictionary).user_data(None, None).unwrap();
        assert_eq!(
            data.into_keys().collect::<Vec<_>>(),
            vec![inbox(0xA), inbox(0xD)]
        );
    }

    fn update(write: FieldWrite) -> Vec<u8> {
        match write.operation {
            WriteOperation::Update(payload) => payload,
            other => panic!("expected an update, got {other:?}"),
        }
    }

    /// A write is encoded under the committed type: the protocol's for a
    /// well-known field and the registry's for an application field, never
    /// the catalogue's. A value, key, or mutation of the wrong shape, an
    /// unknown type, and an unlisted field are refused.
    // verifies: META-071
    #[xmtp_common::test(unwrap_try = true)]
    fn field_writes_follow_the_committed_type() {
        use ComponentMutation as M;
        let dictionary = group();
        let fields = snapshot(&dictionary);
        let status = MetadataFieldRef::new(STATUS);
        let write = fields
            .field_write(&status, &M::Replace(string("away")))
            .unwrap();
        assert_eq!(write.component_type, ComponentType::String);
        assert_eq!(update(write), b"away");

        let insert = M::MapDelta(vec![MapMutation::Insert(
            FieldKey::InboxId(inbox(0xB)),
            string("Bo"),
        )]);
        let write = fields
            .field_write(&MetadataFieldRef::USER_DISPLAY_NAME, &insert)
            .unwrap();
        assert_eq!(write.component_type, ComponentType::TlsMapInboxIdString);
        assert_eq!(
            update(write),
            TlsMapDelta::<InboxId, VLBytes>::new()
                .insert(inbox(0xB), b"Bo".as_slice().into())
                .tls_serialize_detached()
                .unwrap()
        );

        let set = M::SetDelta(vec![
            SetMutation::Insert(FieldKey::InboxId(inbox(0xA))),
            SetMutation::DeleteByHash([1; 32]),
        ]);
        let write = fields
            .field_write(&MetadataFieldRef::new(TAGS), &set)
            .unwrap();
        assert_eq!(
            update(write),
            TlsSetDelta::<InboxId>::new()
                .insert(inbox(0xA))
                .remove_by_hash(TlsKeyHash::from_bytes([1; 32]))
                .tls_serialize_detached()
                .unwrap()
        );
        assert_eq!(
            fields.field_write(&status, &M::Remove).unwrap().operation,
            WriteOperation::Remove
        );

        let bytes_name = M::MapDelta(vec![MapMutation::Update(
            FieldKey::InboxId(inbox(0xA)),
            FieldValue::Bytes(b"Alix".to_vec()),
        )]);
        for (field, mutation) in [
            (
                status.clone(),
                M::Replace(FieldValue::Bytes(b"away".to_vec())),
            ),
            (status.clone(), M::MapDelta(vec![])),
            (MetadataFieldRef::USER_DISPLAY_NAME, bytes_name),
            (
                MetadataFieldRef::new(SCORES),
                M::MapDelta(vec![MapMutation::Delete(FieldKey::Bytes(vec![0xA; 32]))]),
            ),
            (
                MetadataFieldRef::new(TAGS),
                M::SetDelta(vec![SetMutation::Delete(FieldKey::Bytes(vec![]))]),
            ),
            (MetadataFieldRef::new(LINKS), M::SetDelta(vec![])),
        ] {
            assert!(matches!(
                fields.field_write(&field, &mutation),
                Err(FieldError::TypeMismatch(id)) if id == field.component_id
            ));
        }
        for mutation in [M::Remove, M::Replace(FieldValue::Bytes(vec![]))] {
            assert!(matches!(
                fields.field_write(&MetadataFieldRef::new(FUTURE), &mutation),
                Err(FieldError::UnsupportedType {
                    component_id: FUTURE,
                    tag: 99
                })
            ));
        }
        assert!(matches!(
            fields.field_write(&MetadataFieldRef::GROUP_IMAGE, &M::Remove),
            Err(FieldError::UnknownField(ComponentId::GROUP_IMAGE))
        ));
    }

    /// Own user data updates name each user field once, with a value of its
    /// map value type.
    // verifies: META-073
    #[xmtp_common::test(unwrap_try = true)]
    fn user_data_writes_refuse_bad_updates() {
        let dictionary = group();
        let fields = snapshot(&dictionary);
        let set = |field: MetadataFieldRef, value| UserFieldUpdate { field, value };
        assert_eq!(
            fields
                .user_data_writes(&[
                    set(MetadataFieldRef::USER_DISPLAY_NAME, Some(string("Bo"))),
                    set(MetadataFieldRef::new(SCORES), None),
                ])
                .unwrap(),
            vec![
                FieldWrite {
                    component_id: ComponentId::USER_DISPLAY_NAME,
                    component_type: ComponentType::TlsMapInboxIdString,
                    operation: WriteOperation::SetOwn(b"Bo".to_vec()),
                },
                FieldWrite {
                    component_id: SCORES,
                    component_type: ComponentType::TlsMapInboxIdBytes,
                    operation: WriteOperation::ClearOwn,
                },
            ]
        );
        assert!(fields.user_data_writes(&[]).unwrap().is_empty());
        assert!(matches!(
            fields.user_data_writes(&[
                set(MetadataFieldRef::USER_DISPLAY_NAME, Some(string("Bo"))),
                set(named(ComponentId::USER_DISPLAY_NAME, "nickname"), None),
            ]),
            Err(FieldError::DuplicateField(ComponentId::USER_DISPLAY_NAME))
        ));
        assert!(matches!(
            fields.user_data_writes(&[set(MetadataFieldRef::GROUP_NAME, Some(string("x")))]),
            Err(FieldError::NotUserField(ComponentId::GROUP_NAME))
        ));
        assert!(matches!(
            fields.user_data_writes(&[set(MetadataFieldRef::new(SCORES), Some(string("7")))]),
            Err(FieldError::TypeMismatch(SCORES))
        ));
        assert!(matches!(
            fields.user_data_writes(&[set(
                MetadataFieldRef::USER_DISPLAY_NAME,
                Some(FieldValue::Bytes(b"Bo".to_vec()))
            )]),
            Err(FieldError::TypeMismatch(ComponentId::USER_DISPLAY_NAME))
        ));
        assert!(matches!(
            fields.user_data_writes(&[set(MetadataFieldRef::new(BROKEN), None)]),
            Err(FieldError::UnknownField(BROKEN))
        ));
    }

    fn own_write(id: ComponentId, ty: ComponentType, operation: WriteOperation) -> FieldWrite {
        FieldWrite {
            component_id: id,
            component_type: ty,
            operation,
        }
    }

    fn map_delta(delta: TlsMapDelta<InboxId, VLBytes>) -> AppDataUpdateOperation {
        AppDataUpdateOperation::Update(delta.tls_serialize_detached().unwrap().into())
    }

    /// An own-key write only ever names the writer's inbox: a set is an
    /// Insert when the key is absent and an Update when present, a clear
    /// is a Delete when present and nothing when absent.
    // verifies: META-073
    #[xmtp_common::test(unwrap_try = true)]
    fn own_writes_resolve_against_current_values() {
        let dictionary = group();
        let fields = snapshot(&dictionary);
        let names = |operation| {
            own_write(
                ComponentId::USER_DISPLAY_NAME,
                ComponentType::TlsMapInboxIdString,
                operation,
            )
        };
        let scores = |operation| own_write(SCORES, ComponentType::TlsMapInboxIdBytes, operation);
        let resolve = |own, writes: &[FieldWrite]| {
            fields
                .resolve_writes(Some(&dictionary), own, writes)
                .unwrap()
        };
        let id = ComponentId::USER_DISPLAY_NAME;
        assert_eq!(
            resolve(inbox(0xA), &[names(WriteOperation::SetOwn(b"Al".to_vec()))]),
            vec![(
                id,
                map_delta(TlsMapDelta::new().update(inbox(0xA), b"Al".as_slice().into()))
            )]
        );
        assert_eq!(
            resolve(inbox(0xB), &[names(WriteOperation::SetOwn(b"Bo".to_vec()))]),
            vec![(
                id,
                map_delta(TlsMapDelta::new().insert(inbox(0xB), b"Bo".as_slice().into()))
            )]
        );
        assert_eq!(
            resolve(
                inbox(0xA),
                &[
                    names(WriteOperation::ClearOwn),
                    scores(WriteOperation::ClearOwn)
                ]
            ),
            vec![
                (id, map_delta(TlsMapDelta::new().delete(inbox(0xA)))),
                (SCORES, map_delta(TlsMapDelta::new().delete(inbox(0xA)))),
            ]
        );
        assert!(
            resolve(
                inbox(0xB),
                &[
                    names(WriteOperation::ClearOwn),
                    scores(WriteOperation::ClearOwn)
                ]
            )
            .is_empty()
        );
        assert!(resolve(inbox(0xB), &[]).is_empty());
        // Against an empty dictionary every set inserts.
        assert_eq!(
            fields
                .resolve_writes(
                    None,
                    inbox(0xA),
                    &[names(WriteOperation::SetOwn(b"A".to_vec()))]
                )
                .unwrap(),
            vec![(
                id,
                map_delta(TlsMapDelta::new().insert(inbox(0xA), b"A".as_slice().into()))
            )]
        );
        assert!(matches!(
            fields.resolve_writes(
                Some(&dictionary),
                inbox(0xA),
                &[
                    names(WriteOperation::ClearOwn),
                    names(WriteOperation::ClearOwn)
                ]
            ),
            Err(FieldError::DuplicateField(ComponentId::USER_DISPLAY_NAME))
        ));
    }

    /// A write is checked against its current value before any commit: a
    /// removal of an absent component is dropped, and a value the group
    /// would reject, or a delta that no longer applies, is refused.
    // verifies: META-071
    #[xmtp_common::test(unwrap_try = true)]
    fn writes_are_checked_against_current_values() {
        let dictionary = group();
        let fields = snapshot(&dictionary);
        let status = |operation| own_write(STATUS, ComponentType::String, operation);
        let resolve =
            |writes: &[FieldWrite]| fields.resolve_writes(Some(&dictionary), inbox(0xA), writes);
        assert!(
            resolve(&[status(WriteOperation::Remove)])
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            resolve(&[own_write(
                SCORES,
                ComponentType::TlsMapInboxIdBytes,
                WriteOperation::Remove
            )])
            .unwrap(),
            vec![(SCORES, AppDataUpdateOperation::Remove)]
        );
        assert!(matches!(
            resolve(&[status(WriteOperation::Update(vec![b'x'; 8193]))]),
            Err(FieldError::Component(_))
        ));
        assert!(matches!(
            resolve(&[status(WriteOperation::Update(vec![0xC3, 0x28]))]),
            Err(FieldError::Component(_))
        ));
        let insert_present = TlsMapDelta::<InboxId, VLBytes>::new()
            .insert(inbox(0xA), b"again".as_slice().into())
            .tls_serialize_detached()
            .unwrap();
        assert!(matches!(
            resolve(&[own_write(
                ComponentId::USER_DISPLAY_NAME,
                ComponentType::TlsMapInboxIdString,
                WriteOperation::Update(insert_present)
            )]),
            Err(FieldError::Component(_))
        ));
    }

    /// A write encoded under one snapshot is refused if its field changed
    /// type or left the registry before it is resolved.
    // verifies: META-071
    #[xmtp_common::test(unwrap_try = true)]
    fn stale_writes_are_refused() {
        let encoded = snapshot(&group())
            .field_write(
                &MetadataFieldRef::new(STATUS),
                &ComponentMutation::Replace(string("x")),
            )
            .unwrap();
        let retyped = dictionary(&[(STATUS, entry(tag(ComponentType::Bytes)))], &[]);
        assert!(matches!(
            snapshot(&retyped).resolve_writes(
                Some(&retyped),
                inbox(0xA),
                std::slice::from_ref(&encoded)
            ),
            Err(FieldError::TypeChanged {
                component_id: STATUS,
                expected: ComponentType::String,
                actual: MetadataComponentType::Bytes,
            })
        ));
        let removed = dictionary(&[], &[]);
        assert!(matches!(
            snapshot(&removed).resolve_writes(Some(&removed), inbox(0xA), &[encoded]),
            Err(FieldError::UnknownField(STATUS))
        ));
    }

    /// An entry this build cannot interpret neither blocks the snapshot nor
    /// is touched: other fields still read and write, the entry keeps its
    /// bytes, and a write names only its own component.
    // verifies: META-016, META-017
    #[xmtp_common::test(unwrap_try = true)]
    fn uninterpreted_entries_are_left_alone() {
        let dictionary = group();
        let before = dictionary.clone();
        let fields = snapshot(&dictionary);
        let writes = fields
            .user_data_writes(&[UserFieldUpdate {
                field: MetadataFieldRef::USER_DISPLAY_NAME,
                value: Some(string("Bo")),
            }])
            .unwrap();
        let updates = fields
            .resolve_writes(Some(&dictionary), inbox(0xB), &writes)
            .unwrap();
        assert_eq!(
            updates.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
            vec![ComponentId::USER_DISPLAY_NAME]
        );
        let _ = fields.values(&[MetadataFieldRef::new(FUTURE)]);
        assert_eq!(dictionary, before);
        assert_eq!(dictionary.get(&BROKEN.as_u16()), Some([1, 2, 3].as_slice()));
    }

    /// Every wire type tag maps to its shape and back; an unspecified or
    /// unknown tag is `Unknown` and has no wire tag.
    #[xmtp_common::test(unwrap_try = true)]
    fn type_tags_round_trip() {
        for tag in 1..=7 {
            let shape = MetadataComponentType::from_tag(tag);
            assert_eq!(shape.tag().map(|t| t as i32), Some(tag));
        }
        for tag in [0, 8, -1] {
            assert_eq!(
                MetadataComponentType::from_tag(tag),
                MetadataComponentType::Unknown { tag }
            );
            assert_eq!(MetadataComponentType::from_tag(tag).tag(), None);
        }
    }
}
