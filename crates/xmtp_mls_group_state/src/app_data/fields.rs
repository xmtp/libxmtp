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
};
use xmtp_mls_tls_collections::{
    tls_map::{TlsMap, TlsMapDelta},
    tls_set::{TlsKeyHash, TlsSet, TlsSetDelta},
};

mod writes;

use writes::first_duplicate;

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
    /// group without a dictionary. [`ComponentRegistry::iter`] already
    /// skips an entry missing any of its three member policies, so every
    /// listed field has all three.
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

    /// The registry the fields come from.
    pub fn registry(&self) -> &ComponentRegistry {
        &self.registry
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

#[cfg(test)]
mod tests;
