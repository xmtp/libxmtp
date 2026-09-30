//! Guard: every `#[error(transparent)]` variant of the core error enums that
//! reach the SDK is opened by the error walk (`error/wrappers.rs`) or allowed
//! here with a reason. A transparent variant forwards `source()` past its inner
//! error, so a new one that nobody opens would silently turn a typed cause into
//! `Unknown`.

use crate::error::OPENED_WRAPPERS;
use std::collections::BTreeSet;

/// (source file under `crates/`, enum name) for every scanned enum.
const SCANNED: &[(&str, &str)] = &[
    ("xmtp_mls/src/client.rs", "ClientError"),
    ("xmtp_mls/src/groups/error.rs", "GroupError"),
    ("xmtp_mls/src/builder.rs", "ClientBuilderError"),
    ("xmtp_mls/src/identity.rs", "IdentityError"),
    ("xmtp_mls/src/identity_updates.rs", "IdentityUpdateError"),
    ("xmtp_mls/src/identity_updates.rs", "InstallationDiffError"),
    (
        "xmtp_mls/src/identity_updates/dependencies.rs",
        "IdentityDependencyError",
    ),
    (
        "xmtp_mls/src/groups/validated_commit.rs",
        "CommitValidationError",
    ),
    ("xmtp_mls/src/mls_store.rs", "MlsStoreError"),
    ("xmtp_mls/src/groups/intents.rs", "IntentError"),
    (
        "xmtp_mls/src/groups/mls_sync.rs",
        "GroupMessageProcessingError",
    ),
    (
        "xmtp_mls/src/subscriptions/incoming/status.rs",
        "IncomingError",
    ),
    (
        "xmtp_mls/src/subscriptions/local_delivery/types.rs",
        "LocalDeliveryError",
    ),
    ("xmtp_mls/src/worker/device_sync/mod.rs", "DeviceSyncError"),
];

const LEAF: &str =
    "a leaf input, validation, crypto, or encoding error with no code in the error table";
const PRIVATE: &str =
    "the inner type is private to xmtp_mls; its own variants keep their inner errors as sources";
const NO_ACTION: &str = "the inner error has no recovery action in the error table";

/// (enum, variant, reason) for transparent variants that the walk does not open.
const ALLOWED: &[(&str, &str, &str)] = &[
    ("ClientError", "AddressValidation", LEAF),
    ("ClientError", "SignatureRequest", LEAF),
    ("ClientError", "LocalEvent", NO_ACTION),
    ("ClientError", "EnrichMessage", NO_ACTION),
    ("ClientError", "Conversion", LEAF),
    ("GroupError", "OutgoingPreparation", PRIVATE),
    ("GroupError", "NotFound", NO_ACTION),
    ("GroupError", "LeaveCantProcessed", LEAF),
    ("GroupError", "AddressValidation", LEAF),
    ("GroupError", "LocalEvent", NO_ACTION),
    ("GroupError", "MetadataPermissionsError", LEAF),
    ("GroupError", "WrapWelcome", LEAF),
    ("GroupError", "UnwrapWelcome", LEAF),
    ("GroupError", "UninitializedField", LEAF),
    ("GroupError", "DeleteMessage", LEAF),
    ("ClientBuilderError", "StorageLocation", NO_ACTION),
    ("ClientBuilderError", "Attachment", NO_ACTION),
    ("ClientBuilderError", "AddressValidation", LEAF),
    ("IdentityError", "CredentialSerialization", LEAF),
    ("IdentityError", "Decode", LEAF),
    ("IdentityError", "SignatureRequestBuilder", LEAF),
    ("IdentityError", "Signature", LEAF),
    ("IdentityError", "BasicCredential", LEAF),
    ("IdentityError", "Crypto", LEAF),
    ("IdentityError", "OpenMls", LEAF),
    ("IdentityError", "OpenMlsStorageError", NO_ACTION),
    ("IdentityError", "KeyPackageGenerationError", LEAF),
    ("IdentityError", "KeyPackageVerificationError", LEAF),
    ("IdentityError", "Association", LEAF),
    ("IdentityError", "Signer", LEAF),
    ("IdentityError", "AddressValidation", LEAF),
    ("IdentityError", "GeneratePostQuantumKey", LEAF),
    ("IdentityError", "InvalidExtension", LEAF),
    ("IdentityError", "UninitializedField", LEAF),
    ("IdentityUpdateError", "InvalidSignatureRequest", LEAF),
    ("IdentityUpdateError", "Validation", LEAF),
    ("CommitValidationError", "Bootstrap", LEAF),
    ("CommitValidationError", "Rule", LEAF),
    ("MlsStoreError", "NotFound", NO_ACTION),
    ("GroupMessageProcessingError", "Envelope", LEAF),
    ("GroupMessageProcessingError", "Codec", LEAF),
    (
        "GroupMessageProcessingError",
        "AssociationDeserialization",
        LEAF,
    ),
    ("GroupMessageProcessingError", "Builder", LEAF),
    ("GroupMessageProcessingError", "EnrichMessage", NO_ACTION),
    ("GroupMessageProcessingError", "Conversion", LEAF),
    ("DeviceSyncError", "ProtoConversion", LEAF),
    ("DeviceSyncError", "Subscribe", NO_ACTION),
    ("DeviceSyncError", "Bincode", LEAF),
    ("DeviceSyncError", "Archive", NO_ACTION),
    ("DeviceSyncError", "Decode", LEAF),
    ("DeviceSyncError", "Deserialization", LEAF),
    ("DeviceSyncError", "Recv", NO_ACTION),
];

/// The transparent variants of `name` in `source`.
fn transparent_variants(source: &str, name: &str) -> Vec<String> {
    let start = source
        .find(&format!("pub enum {name} "))
        .or_else(|| source.find(&format!("pub enum {name}<")))
        .unwrap_or_else(|| panic!("enum {name} not found"));
    let open = start + source[start..].find('{').expect("enum body");
    let mut depth = 0;
    let mut end = open;
    for (offset, character) in source[open..].char_indices() {
        match character {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end = open + offset;
                    break;
                }
            }
            _ => {}
        }
    }
    let body = &source[open..end];
    let mut variants = Vec::new();
    let mut rest = body;
    while let Some(at) = rest.find("#[error(transparent)]") {
        rest = &rest[at + "#[error(transparent)]".len()..];
        let variant = rest
            .lines()
            .map(str::trim)
            .find(|line| line.starts_with(|c: char| c.is_ascii_uppercase()))
            .and_then(|line| {
                line.split(|c: char| !c.is_alphanumeric() && c != '_')
                    .next()
            })
            .expect("variant after #[error(transparent)]");
        variants.push(variant.to_string());
    }
    variants
}

fn short(path: &str) -> &str {
    path.rsplit("::").next().unwrap_or(path).trim()
}

#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
fn every_transparent_core_wrapper_is_opened_or_allowed() {
    let crates = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let opened: BTreeSet<(String, String)> = OPENED_WRAPPERS
        .iter()
        .map(|(ty, variant)| (short(ty).to_string(), variant.to_string()))
        .collect();
    let allowed: BTreeSet<(String, String)> = ALLOWED
        .iter()
        .map(|(ty, variant, _)| (ty.to_string(), variant.to_string()))
        .collect();
    let mut found = BTreeSet::new();
    let mut missing = Vec::new();
    for (file, name) in SCANNED {
        let source = std::fs::read_to_string(crates.join(file))?;
        for variant in transparent_variants(&source, name) {
            let key = (name.to_string(), variant.clone());
            if !opened.contains(&key) && !allowed.contains(&key) {
                missing.push(format!("{name}::{variant}"));
            }
            found.insert(key);
        }
    }
    assert!(
        missing.is_empty(),
        "transparent core wrappers that the error walk neither opens nor allows: {missing:?}"
    );
    // Every entry names a real transparent variant, so the lists stay current.
    let stale: Vec<_> = opened
        .union(&allowed)
        .filter(|key| !found.contains(*key))
        .collect();
    assert!(
        stale.is_empty(),
        "entries that are not transparent variants: {stale:?}"
    );
    assert!(
        opened.is_disjoint(&allowed),
        "a variant is both opened and allowed"
    );
}
