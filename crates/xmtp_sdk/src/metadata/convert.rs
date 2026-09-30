//! Conversions between the façade metadata records and core, protobuf and
//! configuration types. Values from core are complete; host input is checked
//! here and fails with `InvalidArgument`.

use std::borrow::Cow;

use xmtp_mls::mls_common::{
    app_data::{component_id::ComponentId, fields},
    inbox_id::InboxId as CoreInboxId,
};
use xmtp_proto::xmtp::mls::message_contents::{
    ComponentPermissions as ProtoPermissions, MetadataPolicy as ProtoPolicy,
    metadata_policy::{Kind, MetadataBasePolicy as ProtoBase},
};

use super::*;
use crate::XmtpError;

impl From<MetadataFieldRef> for fields::MetadataFieldRef {
    fn from(value: MetadataFieldRef) -> Self {
        Self {
            component_id: ComponentId::new(value.component_id),
            name: value.name.map(Cow::Owned),
        }
    }
}

impl From<fields::MetadataScalarType> for MetadataScalarType {
    fn from(value: fields::MetadataScalarType) -> Self {
        match value {
            fields::MetadataScalarType::Bytes => Self::Bytes,
            fields::MetadataScalarType::String => Self::String,
        }
    }
}

impl From<fields::MetadataKeyType> for MetadataKeyType {
    fn from(value: fields::MetadataKeyType) -> Self {
        match value {
            fields::MetadataKeyType::Bytes => Self::Bytes,
            fields::MetadataKeyType::InboxId => Self::InboxId,
        }
    }
}

impl From<MetadataScalarType> for fields::MetadataScalarType {
    fn from(value: MetadataScalarType) -> Self {
        match value {
            MetadataScalarType::Bytes => Self::Bytes,
            MetadataScalarType::String => Self::String,
        }
    }
}

impl From<MetadataKeyType> for fields::MetadataKeyType {
    fn from(value: MetadataKeyType) -> Self {
        match value {
            MetadataKeyType::Bytes => Self::Bytes,
            MetadataKeyType::InboxId => Self::InboxId,
        }
    }
}

impl From<fields::MetadataComponentType> for MetadataComponentType {
    fn from(value: fields::MetadataComponentType) -> Self {
        use fields::MetadataComponentType as C;
        match value {
            C::Bytes => Self::Bytes,
            C::String => Self::String,
            C::Map {
                key_type,
                value_type,
            } => Self::Map {
                key_type: key_type.into(),
                value_type: value_type.into(),
            },
            C::Set { key_type } => Self::Set {
                key_type: key_type.into(),
            },
            C::Unknown { tag } => Self::Unknown { tag },
        }
    }
}

impl MetadataBasePolicy {
    fn from_tag(tag: i32) -> Self {
        match ProtoBase::try_from(tag) {
            Ok(ProtoBase::Allow) => Self::Allow,
            Ok(ProtoBase::Deny) => Self::Deny,
            Ok(ProtoBase::AllowIfAdmin) => Self::AllowIfAdmin,
            Ok(ProtoBase::AllowIfSuperAdmin) => Self::AllowIfSuperAdmin,
            Ok(ProtoBase::AllowIfSelfOrNonMember) => Self::AllowIfSelfOrNonMember,
            Ok(ProtoBase::Unspecified) | Err(_) => Self::Unknown { tag },
        }
    }
}

/// A policy with no kind, or no policy at all, is unspecified.
const UNSPECIFIED: MetadataPolicy = MetadataPolicy::Base(MetadataBasePolicy::Unknown { tag: 0 });

impl From<&ProtoPolicy> for MetadataPolicy {
    fn from(value: &ProtoPolicy) -> Self {
        let policies = |list: &[ProtoPolicy]| list.iter().map(Into::into).collect::<Vec<_>>();
        match &value.kind {
            Some(Kind::Base(tag)) => Self::Base(MetadataBasePolicy::from_tag(*tag)),
            Some(Kind::AndCondition(condition)) => Self::And(policies(&condition.policies)),
            Some(Kind::AnyCondition(condition)) => Self::Any(policies(&condition.policies)),
            None => UNSPECIFIED,
        }
    }
}

impl From<&xmtp_configuration::MetadataPolicy> for MetadataPolicy {
    fn from(value: &xmtp_configuration::MetadataPolicy) -> Self {
        use xmtp_configuration::MetadataPolicy as P;
        match value {
            P::Base(tag) => Self::Base(MetadataBasePolicy::from_tag(*tag)),
            P::And(list) => Self::And(list.iter().map(Into::into).collect()),
            P::Any(list) => Self::Any(list.iter().map(Into::into).collect()),
        }
    }
}

impl From<&ProtoPermissions> for ComponentPermissions {
    fn from(value: &ProtoPermissions) -> Self {
        let policy = |policy: &Option<ProtoPolicy>| policy.as_ref().map_or(UNSPECIFIED, Into::into);
        Self {
            insert: policy(&value.insert_policy),
            update: policy(&value.update_policy),
            delete: policy(&value.delete_policy),
        }
    }
}

impl From<&xmtp_configuration::ComponentPermissions> for ComponentPermissions {
    fn from(value: &xmtp_configuration::ComponentPermissions) -> Self {
        let policy = |policy: &Option<xmtp_configuration::MetadataPolicy>| {
            policy.as_ref().map_or(UNSPECIFIED, Into::into)
        };
        Self {
            insert: policy(&value.insert),
            update: policy(&value.update),
            delete: policy(&value.delete),
        }
    }
}

impl From<fields::MetadataFieldDescriptor> for MetadataFieldDescriptor {
    fn from(value: fields::MetadataFieldDescriptor) -> Self {
        Self {
            is_user_field: value.is_user_field(),
            permissions: (&value.permissions).into(),
            component_type: value.component_type.into(),
            field: value.field.into(),
        }
    }
}

impl From<&xmtp_configuration::ApplicationComponentDefinition> for ApplicationComponentDefinition {
    fn from(value: &xmtp_configuration::ApplicationComponentDefinition) -> Self {
        Self {
            component_id: value.component_id,
            name: value.name.clone(),
            component_type: fields::MetadataComponentType::from_tag(value.component_type).into(),
            permissions: (&value.permissions).into(),
            in_groups: value.in_groups,
            in_dms: value.in_dms,
        }
    }
}

impl From<fields::FieldValue> for FieldValue {
    fn from(value: fields::FieldValue) -> Self {
        match value {
            fields::FieldValue::Bytes(bytes) => Self::Bytes(bytes),
            fields::FieldValue::String(text) => Self::String(text),
        }
    }
}

impl From<FieldValue> for fields::FieldValue {
    fn from(value: FieldValue) -> Self {
        match value {
            FieldValue::Bytes(bytes) => Self::Bytes(bytes),
            FieldValue::String(text) => Self::String(text),
        }
    }
}

impl From<fields::FieldKey> for FieldKey {
    fn from(value: fields::FieldKey) -> Self {
        match value {
            fields::FieldKey::Bytes(bytes) => Self::Bytes(bytes),
            fields::FieldKey::InboxId(id) => Self::InboxId(id.to_hex()),
        }
    }
}

impl TryFrom<FieldKey> for fields::FieldKey {
    type Error = XmtpError;

    fn try_from(value: FieldKey) -> Result<Self, Self::Error> {
        Ok(match value {
            FieldKey::Bytes(bytes) => Self::Bytes(bytes),
            FieldKey::InboxId(id) => Self::InboxId(core_inbox_id(&id)?),
        })
    }
}

/// Parses inbox ID text as the 32-byte inbox ID the metadata fields key by.
pub(crate) fn core_inbox_id(id: &str) -> Result<CoreInboxId, XmtpError> {
    CoreInboxId::from_hex(id).map_err(|error| XmtpError::invalid_argument(error.to_string()))
}

impl From<fields::MetadataValue> for MetadataValue {
    fn from(value: fields::MetadataValue) -> Self {
        match value {
            fields::MetadataValue::Scalar(value) => Self::Scalar(value.into()),
            fields::MetadataValue::Map(entries) => Self::Map(
                entries
                    .into_iter()
                    .map(|entry| MapEntry {
                        key: entry.key.into(),
                        value: entry.value.into(),
                    })
                    .collect(),
            ),
            fields::MetadataValue::Set(keys) => {
                Self::Set(keys.into_iter().map(Into::into).collect())
            }
        }
    }
}

impl From<fields::MetadataFieldValue> for MetadataFieldValue {
    fn from(value: fields::MetadataFieldValue) -> Self {
        Self {
            field: value.field.into(),
            value: value.value.map(Into::into),
        }
    }
}

impl From<fields::UserFieldValue> for UserFieldValue {
    fn from(value: fields::UserFieldValue) -> Self {
        Self {
            field: value.field.into(),
            value: value.value.into(),
        }
    }
}

impl From<UserFieldUpdate> for fields::UserFieldUpdate {
    fn from(value: UserFieldUpdate) -> Self {
        Self {
            field: value.field.into(),
            value: value.value.map(Into::into),
        }
    }
}

impl TryFrom<MapMutation> for fields::MapMutation {
    type Error = XmtpError;

    fn try_from(value: MapMutation) -> Result<Self, Self::Error> {
        Ok(match value {
            MapMutation::Insert(key, value) => Self::Insert(key.try_into()?, value.into()),
            MapMutation::Update(key, value) => Self::Update(key.try_into()?, value.into()),
            MapMutation::Delete(key) => Self::Delete(key.try_into()?),
        })
    }
}

impl TryFrom<SetMutation> for fields::SetMutation {
    type Error = XmtpError;

    fn try_from(value: SetMutation) -> Result<Self, Self::Error> {
        Ok(match value {
            SetMutation::Insert(key) => Self::Insert(key.try_into()?),
            SetMutation::Delete(key) => Self::Delete(key.try_into()?),
            SetMutation::DeleteByHash(hash) => Self::DeleteByHash(
                hash.try_into()
                    .map_err(|_| XmtpError::invalid_argument("a key hash is 32 bytes"))?,
            ),
        })
    }
}

impl TryFrom<ComponentMutation> for fields::ComponentMutation {
    type Error = XmtpError;

    fn try_from(value: ComponentMutation) -> Result<Self, Self::Error> {
        Ok(match value {
            ComponentMutation::Replace(value) => Self::Replace(value.into()),
            ComponentMutation::Remove => Self::Remove,
            ComponentMutation::MapDelta(changes) => Self::MapDelta(
                changes
                    .into_iter()
                    .map(TryInto::try_into)
                    .collect::<Result<_, _>>()?,
            ),
            ComponentMutation::SetDelta(changes) => Self::SetDelta(
                changes
                    .into_iter()
                    .map(TryInto::try_into)
                    .collect::<Result<_, _>>()?,
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use xmtp_mls::mls_common::app_data::{component_id::ComponentId, fields};
    use xmtp_proto::xmtp::mls::message_contents::{
        ComponentPermissions as ProtoPermissions, MetadataPolicy as ProtoPolicy,
        metadata_policy::{AndCondition, AnyCondition, Kind},
    };

    use super::super::*;

    /// A descriptor keeps an unknown type tag and the whole committed policy
    /// tree; an unknown or missing policy reads as unknown.
    #[xmtp_common::test(unwrap_try = true)]
    fn descriptors_keep_unknown_types_and_policy_trees() {
        let base = |tag| ProtoPolicy {
            kind: Some(Kind::Base(tag)),
        };
        let descriptor = fields::MetadataFieldDescriptor {
            field: fields::MetadataFieldRef {
                component_id: ComponentId::new(0xC007),
                name: None,
            },
            component_type: fields::MetadataComponentType::Unknown { tag: 42 },
            permissions: ProtoPermissions {
                insert_policy: Some(ProtoPolicy {
                    kind: Some(Kind::AndCondition(AndCondition {
                        policies: vec![
                            base(3),
                            ProtoPolicy {
                                kind: Some(Kind::AnyCondition(AnyCondition {
                                    policies: vec![base(4), base(9)],
                                })),
                            },
                        ],
                    })),
                }),
                update_policy: Some(ProtoPolicy { kind: None }),
                delete_policy: None,
            },
        };
        let unknown = |tag| MetadataPolicy::Base(MetadataBasePolicy::Unknown { tag });
        assert_eq!(
            MetadataFieldDescriptor::from(descriptor),
            MetadataFieldDescriptor {
                field: MetadataFieldRef {
                    component_id: 0xC007,
                    name: None,
                },
                component_type: MetadataComponentType::Unknown { tag: 42 },
                permissions: ComponentPermissions {
                    insert: MetadataPolicy::And(vec![
                        MetadataPolicy::Base(MetadataBasePolicy::AllowIfAdmin),
                        MetadataPolicy::Any(vec![
                            MetadataPolicy::Base(MetadataBasePolicy::AllowIfSuperAdmin),
                            unknown(9),
                        ]),
                    ]),
                    update: unknown(0),
                    delete: unknown(0),
                },
                is_user_field: false,
            }
        );
    }
}
