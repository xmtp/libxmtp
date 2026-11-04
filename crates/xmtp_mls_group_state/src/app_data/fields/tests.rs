use prost::Message;
use xmtp_configuration::{
    ComponentPermissions as CataloguePermissions, MetadataPolicy as CataloguePolicy,
};
use xmtp_proto::xmtp::mls::message_contents::{
    ComponentMetadata, MetadataPolicy,
    metadata_policy::{Kind, MetadataBasePolicy},
};

use super::*;

mod reads;
mod writes;

const STATUS: ComponentId = ComponentId::new(0xC001);
const SCORES: ComponentId = ComponentId::new(0xC002);
const FUTURE: ComponentId = ComponentId::new(0xC003);
const BROKEN: ComponentId = ComponentId::new(0xC004);
const TAGS: ComponentId = ComponentId::new(0xC005);
const LINKS: ComponentId = ComponentId::new(0xC006);
const PARTIAL: ComponentId = ComponentId::new(0xC007);

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
            // An entry without a delete policy.
            (
                PARTIAL,
                ComponentMetadata {
                    permissions: Some(ComponentPermissions {
                        delete_policy: None,
                        ..permissions(MetadataBasePolicy::Allow)
                    }),
                    component_type: tag(ComponentType::String),
                    external_committer_permissions: None,
                }
                .encode_to_vec(),
            ),
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
