//! Replaces the backend's application catalogue for clients built after the
//! call, so tests can give two clients different catalogues. Test and
//! conformance builds only.

use parking_lot::Mutex;
use xmtp_configuration as config;
use xmtp_mls::mls_common::app_data::fields;

use super::{
    ApplicationComponentDefinition, ComponentPermissions, MetadataBasePolicy,
    MetadataComponentType, MetadataPolicy,
};
use crate::XmtpError;

static APPLICATION_COMPONENTS: Mutex<Option<Vec<config::ApplicationComponentDefinition>>> =
    parking_lot::const_mutex(None);

/// Clients built after this call use `components` as the backend's
/// application catalogue, or the fetched catalogue when absent. The
/// catalogue is process-wide: every client built while it is set uses it,
/// whoever builds it, so build one client at a time while it is set.
#[cfg(feature = "conformance")]
#[xmtp_macro::sdk_export]
pub async fn sdk_conformance_use_application_components(
    components: Option<Vec<ApplicationComponentDefinition>>,
) -> Result<(), XmtpError> {
    use_application_components(components)
}

pub(crate) fn use_application_components(
    components: Option<Vec<ApplicationComponentDefinition>>,
) -> Result<(), XmtpError> {
    let components = components
        .map(|list| list.iter().map(definition).collect::<Result<Vec<_>, _>>())
        .transpose()?;
    *APPLICATION_COMPONENTS.lock() = components;
    Ok(())
}

/// The catalogue set by [`use_application_components`].
pub(crate) fn application_components() -> Option<Vec<config::ApplicationComponentDefinition>> {
    APPLICATION_COMPONENTS.lock().clone()
}

fn definition(
    value: &ApplicationComponentDefinition,
) -> Result<config::ApplicationComponentDefinition, XmtpError> {
    Ok(config::ApplicationComponentDefinition {
        component_id: value.component_id,
        name: value.name.clone(),
        component_type: component_type(value.component_type)?,
        permissions: permissions(&value.permissions),
        in_groups: value.in_groups,
        in_dms: value.in_dms,
    })
}

fn component_type(value: MetadataComponentType) -> Result<i32, XmtpError> {
    use MetadataComponentType as T;
    let shape = match value {
        T::Unknown { tag } => return Ok(tag),
        T::Bytes => fields::MetadataComponentType::Bytes,
        T::String => fields::MetadataComponentType::String,
        T::Map {
            key_type,
            value_type,
        } => fields::MetadataComponentType::Map {
            key_type: key_type.into(),
            value_type: value_type.into(),
        },
        T::Set { key_type } => fields::MetadataComponentType::Set {
            key_type: key_type.into(),
        },
    };
    shape
        .tag()
        .map(Into::into)
        .ok_or_else(|| XmtpError::invalid_argument("no registry type has this shape"))
}

fn permissions(value: &ComponentPermissions) -> config::ComponentPermissions {
    config::ComponentPermissions {
        insert: Some(policy(&value.insert)),
        update: Some(policy(&value.update)),
        delete: Some(policy(&value.delete)),
    }
}

fn policy(value: &MetadataPolicy) -> config::MetadataPolicy {
    use xmtp_proto::xmtp::mls::message_contents::metadata_policy::MetadataBasePolicy as Base;
    match value {
        MetadataPolicy::Base(base) => config::MetadataPolicy::Base(match *base {
            MetadataBasePolicy::Allow => Base::Allow.into(),
            MetadataBasePolicy::Deny => Base::Deny.into(),
            MetadataBasePolicy::AllowIfAdmin => Base::AllowIfAdmin.into(),
            MetadataBasePolicy::AllowIfSuperAdmin => Base::AllowIfSuperAdmin.into(),
            MetadataBasePolicy::AllowIfSelfOrNonMember => Base::AllowIfSelfOrNonMember.into(),
            MetadataBasePolicy::Unknown { tag } => tag,
        }),
        MetadataPolicy::And(list) => config::MetadataPolicy::And(list.iter().map(policy).collect()),
        MetadataPolicy::Any(list) => config::MetadataPolicy::Any(list.iter().map(policy).collect()),
    }
}
