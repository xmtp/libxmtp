//! Metadata fields and user data through the SDK surface. Alix and Bo read
//! one group through different backend catalogues, so a name labels a
//! field for one reader only and the component ID identifies it.

use std::collections::HashMap;

use super::*;
use crate::{
    ApplicationComponentDefinition, ComponentMutation, ComponentPermissions, Conversation,
    ErrorCategory, FieldKey, FieldValue, Group, MapEntry, MapMutation, MetadataBasePolicy as Base,
    MetadataComponentType, MetadataFieldDescriptor, MetadataFieldRef, MetadataFieldValue,
    MetadataKeyType, MetadataPolicy, MetadataScalarType, MetadataValue, SetMutation,
    UserFieldUpdate, UserFieldValue, WellKnownMetadataField as WellKnown,
    metadata::catalogue_override::use_application_components, metadata_field_ref,
};

const STATUS: u16 = 0xC001;
const NICKNAME: u16 = 0xC002;
const TOPIC: u16 = 0xC003;
const AVATAR: u16 = 0xC004;
const LATER: u16 = 0xC005;
const LABELS: u16 = 0xC006;
const TAGS: u16 = 0xC007;
const MEMBERS: u16 = 0xC008;
const KEPT: u16 = 0xC009;

const NICKNAME_TYPE: MetadataComponentType = MetadataComponentType::Map {
    key_type: MetadataKeyType::InboxId,
    value_type: MetadataScalarType::String,
};

fn field(component_id: u16, name: Option<&str>) -> MetadataFieldRef {
    MetadataFieldRef {
        component_id,
        name: name.map(Into::into),
    }
}

fn permissions(base: Base) -> ComponentPermissions {
    ComponentPermissions {
        insert: MetadataPolicy::Base(base),
        update: MetadataPolicy::Base(base),
        delete: MetadataPolicy::Base(base),
    }
}

fn definition(
    component_id: u16,
    name: &str,
    component_type: MetadataComponentType,
    base: Base,
    in_dms: bool,
) -> ApplicationComponentDefinition {
    ApplicationComponentDefinition {
        component_id,
        name: name.into(),
        component_type,
        permissions: permissions(base),
        in_groups: true,
        in_dms,
    }
}

/// Alix's catalogue. `later` has a type tag no SDK knows, so no
/// conversation registers it.
fn alix_catalogue() -> Vec<ApplicationComponentDefinition> {
    vec![
        definition(
            STATUS,
            "status",
            MetadataComponentType::String,
            Base::Allow,
            true,
        ),
        definition(
            NICKNAME,
            "nickname",
            NICKNAME_TYPE,
            Base::AllowIfSelfOrNonMember,
            true,
        ),
        definition(
            TOPIC,
            "topic",
            MetadataComponentType::String,
            Base::AllowIfAdmin,
            true,
        ),
        definition(
            AVATAR,
            "avatar",
            MetadataComponentType::Bytes,
            Base::Allow,
            false,
        ),
        definition(
            LATER,
            "later",
            MetadataComponentType::Unknown { tag: 99 },
            Base::Allow,
            true,
        ),
    ]
}

/// Bo's catalogue gives `status` to another field and names `STATUS`
/// after a well-known field, with a type and policy the group never
/// committed.
fn bo_catalogue() -> Vec<ApplicationComponentDefinition> {
    vec![
        definition(
            STATUS,
            "GROUP_NAME",
            MetadataComponentType::Bytes,
            Base::Deny,
            true,
        ),
        definition(
            AVATAR,
            "status",
            MetadataComponentType::Bytes,
            Base::Allow,
            false,
        ),
    ]
}

/// Held from setting a catalogue until it is reset, because tests on other
/// threads share the process-wide catalogue. Only these tests take it. A
/// client that another test builds on a shared process while a catalogue is
/// set gets that catalogue; nextest runs each test in its own process.
static CATALOGUE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Resets the catalogue before releasing the lock, also when the build
/// panics.
struct CatalogueSet {
    _lock: tokio::sync::MutexGuard<'static, ()>,
}

impl Drop for CatalogueSet {
    fn drop(&mut self) {
        // Clearing converts nothing, so it cannot fail.
        let _ = use_application_components(None);
    }
}

async fn client_with(catalogue: Vec<ApplicationComponentDefinition>) -> Client {
    let _catalogue = CatalogueSet {
        _lock: CATALOGUE.lock().await,
    };
    use_application_components(Some(catalogue)).unwrap();
    Client::create(crate::generate_local_signer().await, options())
        .await
        .unwrap()
}

/// Alix's group with Bo, and Bo's handle on it.
async fn group_pair(alix: &Client, bo: &Client) -> (Arc<Group>, Arc<Group>) {
    let group = alix
        .conversations()
        .create_group(vec![bo.inbox_id()], None)
        .await
        .unwrap();
    bo.conversations().sync().await.unwrap();
    let Some(Conversation::Group { group: bo_group }) =
        bo.conversations().get_by_id(group.id()).await.unwrap()
    else {
        panic!("Bo has the group");
    };
    (group, bo_group)
}

fn string(value: &str) -> FieldValue {
    FieldValue::String(value.into())
}

fn set(field: MetadataFieldRef, value: &str) -> UserFieldUpdate {
    UserFieldUpdate {
        field,
        value: Some(string(value)),
    }
}

fn user_value(field: MetadataFieldRef, value: &str) -> UserFieldValue {
    UserFieldValue {
        field,
        value: string(value),
    }
}

/// The error's variant name, code, category and retryability.
fn kind(error: XmtpError) -> (String, String, String, bool) {
    let (name, details) = match error {
        XmtpError::UnknownField(details) => ("UnknownField", details),
        XmtpError::NotUserField(details) => ("NotUserField", details),
        XmtpError::DuplicateField(details) => ("DuplicateField", details),
        XmtpError::TypeMismatch(details) => ("TypeMismatch", details),
        XmtpError::PermissionDenied(details) => ("PermissionDenied", details),
        XmtpError::InvalidArgument(details) => ("InvalidArgument", details),
        XmtpError::ClientClosed(details) => ("ClientClosed", details),
        XmtpError::Unknown(details) => ("Unknown", details),
        other => panic!("unexpected error {other:?}"),
    };
    let category = format!("{:?}", details.category);
    (name.into(), details.code, category, details.retryable)
}

/// The error is the `name` variant with code `name` in `category`, and is
/// not retryable.
fn expect_kind(error: XmtpError, name: &str, category: ErrorCategory) {
    assert_eq!(
        kind(error),
        (name.into(), name.into(), format!("{category:?}"), false)
    );
}

mod calls;
#[cfg(not(target_arch = "wasm32"))]
mod catalogue_admission;
mod collections;
mod descriptors;
mod profiles;
mod reads;
