//! Golden rows: the forward-compatibility guard for the stored CBOR.
//!
//! Every database written since the CBOR switch holds rows like the frozen
//! files below. Each file holds the key store rows this build wrote for every
//! label the store writes, from groups shaped like XMTP's, plus the
//! `db_serialize` columns that hold OpenMLS types. Its keys are throwaway test
//! keys made for the file; they protect nothing.
//!
//! The tests check that:
//! - every label the store writes has a row (read from the store's source);
//! - rows and columns with a public type decode with it and encode back to
//!   the same CBOR (map order aside), so a renamed or dropped field, optional
//!   ones included, fails;
//! - OpenMLS reads the crate-private rows the way it reads them in
//!   production: it loads the groups, reads its keys, decrypts a message from
//!   a past epoch, commits a stored proposal, merges a stored staged commit,
//!   and exchanges messages;
//! - the rows this build writes now, from the same steps, have the same field
//!   and variant names as the newest frozen file, so a dependency bump that
//!   renames, adds or removes a stored field fails CI;
//! - the frozen files hold the shapes XMTP stores.
//!
//! A frozen file is never edited or rewritten: its SHA-256 is pinned. For a
//! deliberate format change, run
//! `dev/nix-shell 'just test workspace -p xmtp_db write_golden_rows --run-ignored only'`
//! (native only), rename the new `golden_rows_next.json` to
//! `golden_rows_<release>.json`, add it to [`GOLDEN_FILES`] with its hash, and
//! keep every older file: rows from every released format must keep loading.
//! Say in the commit why the stored format changed.

use std::collections::{BTreeMap, BTreeSet};

use cbor4ii::core::{Value as Cbor, dec::Decode as _, enc::Encode as _, utils::SliceReader};
use diesel::prelude::*;
use openmls::{
    credentials::{BasicCredential, Credential, CredentialWithKey},
    extensions::{
        AppDataDictionary, AppDataDictionaryExtension, ApplicationIdExtension, Extension,
        ExtensionType, Extensions, LastResortExtension, RequiredCapabilitiesExtension,
        UnknownExtension,
    },
    group::{
        GroupId, MlsGroup, MlsGroupCreateConfig, MlsGroupJoinConfig, StagedCommit, StagedWelcome,
    },
    key_packages::{KeyPackage, KeyPackageBundle},
    messages::proposals::ProposalType,
    prelude::{
        Capabilities, CredentialType, KeyPackageRef, LeafNodeParameters, MlsMessageBodyIn,
        MlsMessageIn, ProcessedMessageContent, ProtocolMessage, SignatureScheme,
        tls_codec::{Deserialize as _, VLBytes},
    },
    treesync::{EncryptionKey, test_utils::read_keys_from_key_store},
};
use openmls_basic_credential::SignatureKeyPair;
use openmls_traits::{
    OpenMlsProvider,
    storage::{CURRENT_VERSION, StorageProvider},
};
use serde_json::{Value, json};
use xmtp_configuration::{
    MAX_PAST_EPOCHS, WELCOME_POINTEE_ENCRYPTION_AEAD_TYPES_EXTENSION_ID,
    WELCOME_WRAPPER_ENCRYPTION_EXTENSION_ID,
};
use xmtp_cryptography::{XmtpInstallationCredential, configuration::CIPHERSUITE};

use super::{decode_list, decode_value, encode_value, key, value_from_slice, value_to_vec};
use crate::{
    ConnectionExt, TestDb, XmtpOpenMlsProvider, XmtpTestDb, db_deserialize, db_serialize,
    sql_key_store::{
        COMMIT_LOG_SIGNER_PRIVATE_KEY, KEY_PACKAGE_REFERENCES, KEY_PACKAGE_WRAPPER_PRIVATE_KEY,
        SqlKeyStore,
    },
};

/// A frozen golden file and the SHA-256 of its text.
struct GoldenFile {
    name: &'static str,
    text: &'static str,
    sha256: &'static str,
}

/// Every frozen file, oldest first. The newest one is what this build writes.
const GOLDEN_FILES: &[GoldenFile] = &[GoldenFile {
    name: "golden_rows_rc4.json",
    text: include_str!("golden_rows_rc4.json"),
    sha256: "45430ae8e6244a5f57a8e0583c6fd5fb4d27d9c8b751cee8f790278e2966b3d7",
}];

const STORES: [&str; 3] = ["alice", "bob", "carol"];
const COLUMNS: [&str; 3] = ["Credential", "StagedCommit", "XmtpInstallationCredential"];
/// `ComponentId::COMMIT_LOG_SIGNER` in `xmtp_mls_common`, as XMTP groups use it.
const COMMIT_LOG_SIGNER: u16 = 0x800B;
const LATE_MESSAGE: &[u8] = b"sent in epoch 1, read in epoch 2";

type Provider = XmtpOpenMlsProvider<crate::DefaultMlsStore>;

/// Every label the key store writes, read from its source: a label added
/// there without a golden row fails `golden_rows_cover_every_label`.
fn store_labels() -> BTreeSet<&'static str> {
    const SOURCE: &str = include_str!("../../sql_key_store.rs");
    let production = SOURCE
        .split("\n#[cfg(test)]\npub(crate) mod tests")
        .next()
        .unwrap();
    production
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            let declaration = line.strip_prefix("pub ").unwrap_or(line);
            let (_, label) = declaration
                .strip_prefix("const ")?
                .split_once(": &[u8] = b\"")?;
            label.strip_suffix("\";")
        })
        .collect()
}

#[derive(Debug)]
struct Row {
    label: String,
    key: Vec<u8>,
    value: Vec<u8>,
}

/// The label of a storage key: the longest label it starts with.
fn label_of(storage_key: &[u8]) -> String {
    store_labels()
        .into_iter()
        .filter(|label| storage_key.starts_with(label.as_bytes()))
        .max_by_key(|label| label.len())
        .unwrap_or_else(|| panic!("unknown label in {}", hex::encode(storage_key)))
        .to_owned()
}

async fn provider() -> Provider {
    let store = TestDb::create_persistent_store(None).await;
    XmtpOpenMlsProvider::new(SqlKeyStore::new(store.db()))
}

#[derive(QueryableByName)]
struct KeyValue {
    #[diesel(sql_type = diesel::sql_types::Binary)]
    key_bytes: Vec<u8>,
    #[diesel(sql_type = diesel::sql_types::Binary)]
    value_bytes: Vec<u8>,
}

fn dump(provider: &Provider) -> Vec<Row> {
    let rows: Vec<KeyValue> = provider
        .storage()
        .conn
        .raw_query(|conn| {
            diesel::sql_query(
                "SELECT key_bytes, value_bytes FROM openmls_key_value WHERE version = ? ORDER BY key_bytes",
            )
            .bind::<diesel::sql_types::Integer, _>(i32::from(CURRENT_VERSION))
            .load(conn)
        })
        .unwrap();
    rows.into_iter()
        .map(|row| Row {
            label: label_of(&row.key_bytes),
            key: row.key_bytes,
            value: row.value_bytes,
        })
        .collect()
}

fn restore(provider: &Provider, rows: &[Row]) {
    provider
        .storage()
        .conn
        .raw_query(|conn| {
            for row in rows {
                diesel::sql_query(
                    "REPLACE INTO openmls_key_value (key_bytes, version, value_bytes) VALUES (?, ?, ?)",
                )
                .bind::<diesel::sql_types::Binary, _>(&row.key)
                .bind::<diesel::sql_types::Integer, _>(i32::from(CURRENT_VERSION))
                .bind::<diesel::sql_types::Binary, _>(&row.value)
                .execute(conn)?;
            }
            Ok(())
        })
        .unwrap();
}

fn credential(name: &str, signer: &SignatureKeyPair) -> CredentialWithKey {
    CredentialWithKey {
        credential: BasicCredential::new(name.as_bytes().to_vec()).into(),
        signature_key: signer.to_public_vec().into(),
    }
}

/// The extension types XMTP groups require, as `build_group_config` sets them.
const REQUIRED_EXTENSIONS: [ExtensionType; 3] = [
    ExtensionType::AppDataDictionary,
    ExtensionType::LastResort,
    ExtensionType::ApplicationId,
];

/// A member's capabilities, as `build_key_package` advertises them.
fn member_capabilities() -> Capabilities {
    let mut extensions = REQUIRED_EXTENSIONS.to_vec();
    extensions.extend([
        ExtensionType::Unknown(WELCOME_WRAPPER_ENCRYPTION_EXTENSION_ID),
        ExtensionType::Unknown(WELCOME_POINTEE_ENCRYPTION_AEAD_TYPES_EXTENSION_ID),
    ]);
    Capabilities::new(
        None,
        Some(&[CIPHERSUITE]),
        Some(&extensions),
        Some(&[
            ProposalType::GroupContextExtensions,
            ProposalType::AppDataUpdate,
        ]),
        None,
    )
}

/// A key package shaped like `build_key_package`'s: a last-resort extension,
/// an Unknown extension, and an ApplicationId leaf extension.
fn key_package(
    provider: &impl OpenMlsProvider,
    signer: &SignatureKeyPair,
    name: &str,
) -> KeyPackageBundle {
    KeyPackage::builder()
        .leaf_node_capabilities(member_capabilities())
        .leaf_node_extensions(
            Extensions::single(Extension::ApplicationId(ApplicationIdExtension::new(
                name.as_bytes(),
            )))
            .unwrap(),
        )
        .key_package_extensions(
            Extensions::from_vec(vec![
                Extension::LastResort(LastResortExtension::default()),
                Extension::Unknown(
                    WELCOME_POINTEE_ENCRYPTION_AEAD_TYPES_EXTENSION_ID,
                    UnknownExtension(vec![0, 1]),
                ),
            ])
            .unwrap(),
        )
        .build(CIPHERSUITE, provider, signer, credential(name, signer))
        .unwrap()
}

/// A group config shaped like `build_group_config`'s: an AppData dictionary
/// with a commit-log signer, RequiredCapabilities, XMTP's past epochs and the
/// ratchet tree extension.
fn group_config() -> MlsGroupCreateConfig {
    let mut dictionary = AppDataDictionary::new();
    let _ = dictionary.insert(COMMIT_LOG_SIGNER, vec![0x42; 32]);
    let capabilities = member_capabilities();
    MlsGroupCreateConfig::builder()
        .with_group_context_extensions(
            Extensions::from_vec(vec![
                Extension::AppDataDictionary(AppDataDictionaryExtension::new(dictionary)),
                Extension::RequiredCapabilities(RequiredCapabilitiesExtension::new(
                    &REQUIRED_EXTENSIONS,
                    &[ProposalType::AppDataUpdate],
                    &[CredentialType::Basic],
                )),
            ])
            .unwrap(),
        )
        .capabilities(capabilities)
        .ciphersuite(CIPHERSUITE)
        .max_past_epochs(MAX_PAST_EPOCHS)
        .use_ratchet_tree_extension(true)
        .build()
}

fn join_config() -> MlsGroupJoinConfig {
    MlsGroupJoinConfig::builder()
        .max_past_epochs(MAX_PAST_EPOCHS)
        .use_ratchet_tree_extension(true)
        .build()
}

fn process(provider: &Provider, group: &mut MlsGroup, message: &[u8]) -> ProcessedMessageContent {
    let message = MlsMessageIn::tls_deserialize_exact(message).unwrap();
    let message: ProtocolMessage = message.try_into_protocol_message().unwrap();
    group
        .process_message(provider, message)
        .unwrap()
        .into_content()
}

fn tls(message: &impl openmls::prelude::tls_codec::Serialize) -> Vec<u8> {
    message.tls_serialize_detached().unwrap()
}

/// Process an application message and return its bytes.
fn read_message(provider: &Provider, group: &mut MlsGroup, message: &[u8]) -> Vec<u8> {
    let ProcessedMessageContent::ApplicationMessage(message) = process(provider, group, message)
    else {
        panic!("not an application message");
    };
    message.into_bytes()
}

/// Process a commit and merge it.
fn merge_commit(provider: &Provider, group: &mut MlsGroup, commit: &[u8]) {
    let ProcessedMessageContent::StagedCommitMessage(staged) = process(provider, group, commit)
    else {
        panic!("not a commit");
    };
    group.merge_staged_commit(provider, *staged).unwrap();
}

/// The rows and columns of XMTP-shaped groups, as this build writes them.
///
/// Group 1 (Alice, Bob, Carol):
/// - Alice adds Bob and Carol; all three exchange messages in epoch 1, so the
///   message secrets hold decryption ratchets. Alice keeps one epoch-1
///   message for Bob unread.
/// - Alice removes Carol: Carol's group is inactive, and epoch 1 moves into
///   the past epoch trees.
/// - Alice and Bob exchange messages in epoch 2.
/// - Alice proposes a self-update. Both store it, and Alice stores her own
///   new leaf node and its key pair.
///
/// Group 2 (Alice): Alice proposes adding a member and commits it with an
/// update path, and does not merge. Her store holds an own staged commit with
/// a proposal and a new leaf key pair.
///
/// Bob's store also holds a spare key package, the XMTP labels for it, and
/// a commit-log signer key. The columns are Bob's credential, an
/// installation key, and Alice's staged commit.
async fn build() -> Value {
    let (alice, bob, carol) = (provider().await, provider().await, provider().await);
    let signer = |provider: &Provider| {
        let signer = SignatureKeyPair::new(CIPHERSUITE.signature_algorithm()).unwrap();
        signer.store(provider.storage()).unwrap();
        signer
    };
    let (alice_signer, bob_signer, carol_signer) = (signer(&alice), signer(&bob), signer(&carol));
    let bob_joining = key_package(&bob, &bob_signer, "bob");
    let bob_spare = key_package(&bob, &bob_signer, "bob");
    let carol_joining = key_package(&carol, &carol_signer, "carol");

    let config = group_config();
    let mut group = MlsGroup::new(
        &alice,
        &alice_signer,
        &config,
        credential("alice", &alice_signer),
    )
    .unwrap();
    let (_, welcome, _) = group
        .add_members(
            &alice,
            &alice_signer,
            &[
                bob_joining.key_package().clone(),
                carol_joining.key_package().clone(),
            ],
        )
        .unwrap();
    group.merge_pending_commit(&alice).unwrap();
    let join = |provider: &Provider| {
        let welcome = MlsMessageIn::tls_deserialize_exact(tls(&welcome)).unwrap();
        let MlsMessageBodyIn::Welcome(welcome) = welcome.extract() else {
            panic!("not a Welcome");
        };
        StagedWelcome::new_from_welcome(provider, &join_config(), welcome, None)
            .unwrap()
            .into_group(provider)
            .unwrap()
    };
    let (mut bob_group, mut carol_group) = (join(&bob), join(&carol));

    // Epoch 1: messages from each member to the others.
    let from_alice = tls(&group
        .create_message(&alice, &alice_signer, b"1 from alice")
        .unwrap());
    read_message(&bob, &mut bob_group, &from_alice);
    read_message(&carol, &mut carol_group, &from_alice);
    let from_bob = tls(&bob_group
        .create_message(&bob, &bob_signer, b"1 from bob")
        .unwrap());
    read_message(&alice, &mut group, &from_bob);
    read_message(&carol, &mut carol_group, &from_bob);
    let late = tls(&group
        .create_message(&alice, &alice_signer, LATE_MESSAGE)
        .unwrap());

    // Epoch 2: Alice removes Carol.
    let carol_leaf = group
        .members()
        .find(|member| {
            BasicCredential::try_from(member.credential.clone())
                .is_ok_and(|credential| credential.identity() == b"carol")
        })
        .unwrap()
        .index;
    let (commit, _, _) = group
        .remove_members(&alice, &alice_signer, &[carol_leaf])
        .unwrap();
    group.merge_pending_commit(&alice).unwrap();
    merge_commit(&bob, &mut bob_group, &tls(&commit));
    merge_commit(&carol, &mut carol_group, &tls(&commit));
    assert!(!carol_group.is_active());
    let from_alice = tls(&group
        .create_message(&alice, &alice_signer, b"2 from alice")
        .unwrap());
    read_message(&bob, &mut bob_group, &from_alice);
    let from_bob = tls(&bob_group
        .create_message(&bob, &bob_signer, b"2 from bob")
        .unwrap());
    read_message(&alice, &mut group, &from_bob);

    // Alice's pending self-update, stored by both.
    let (proposal, _) = group
        .propose_self_update(&alice, &alice_signer, LeafNodeParameters::default())
        .unwrap();
    let ProcessedMessageContent::ProposalMessage(queued) =
        process(&bob, &mut bob_group, &tls(&proposal))
    else {
        panic!("not a proposal");
    };
    bob_group
        .store_pending_proposal(bob.storage(), *queued)
        .unwrap();

    // Group 2: Alice's own staged commit with a proposal and a new leaf.
    let mut second = MlsGroup::new(
        &alice,
        &alice_signer,
        &config,
        credential("alice", &alice_signer),
    )
    .unwrap();
    let throwaway = openmls_rust_crypto::OpenMlsRustCrypto::default();
    let dave_signer = SignatureKeyPair::new(CIPHERSUITE.signature_algorithm()).unwrap();
    let dave = key_package(&throwaway, &dave_signer, "dave");
    second
        .propose_add_member(&alice, &alice_signer, dave.key_package())
        .unwrap();
    // A path, so the commit holds Alice's new leaf and its key pair.
    second
        .commit_builder()
        .consume_proposal_store(true)
        .force_self_update(true)
        .load_psks(alice.storage())
        .unwrap()
        .build(alice.rand(), alice.crypto(), &alice_signer, |_| true)
        .unwrap()
        .stage_commit(&alice)
        .unwrap();
    let staged = second.pending_commit().expect("an own staged commit");

    // The XMTP labels, as `identity.rs` and `commit_log_key.rs` write them.
    let hash_ref = bob_spare.key_package().hash_ref(bob.crypto()).unwrap();
    let init_key = tls(bob_spare.key_package().hpke_init_key());
    let storage = bob.storage();
    storage
        .write::<CURRENT_VERSION>(
            KEY_PACKAGE_REFERENCES,
            &init_key,
            &encode_value(KEY_PACKAGE_REFERENCES, &hash_ref).unwrap(),
        )
        .unwrap();
    storage
        .write::<CURRENT_VERSION>(
            KEY_PACKAGE_WRAPPER_PRIVATE_KEY,
            &key::to_vec(&hash_ref).unwrap(),
            &encode_value(KEY_PACKAGE_WRAPPER_PRIVATE_KEY, [0x5a; 32].as_slice()).unwrap(),
        )
        .unwrap();
    storage
        .write::<CURRENT_VERSION>(
            COMMIT_LOG_SIGNER_PRIVATE_KEY,
            &key::to_vec(group.group_id().as_slice()).unwrap(),
            &encode_value(COMMIT_LOG_SIGNER_PRIVATE_KEY, [0x24; 32].as_slice()).unwrap(),
        )
        .unwrap();

    let rows = |provider: &Provider| {
        dump(provider)
            .into_iter()
            .map(|row| {
                json!({
                    "label": row.label,
                    "key": hex::encode(row.key),
                    "value": hex::encode(row.value),
                })
            })
            .collect::<Vec<_>>()
    };
    json!({
        "description": "Key store rows and db_serialize columns written by libxmtp with CBOR values, from XMTP-shaped test groups. The keys are throwaway test keys. src/sql_key_store/codec/golden_tests.rs builds and reads them. Frozen: never edit or regenerate this file.",
        "openmls": "xmtp/openmls b5be619c8ec0a5a7086303c0a8ec59400910c7ee (v0.9.1 with xmtp patches)",
        "groups": {
            "first": hex::encode(group.group_id().as_slice()),
            "second": hex::encode(second.group_id().as_slice()),
        },
        "late_message": hex::encode(late),
        "stores": {
            "alice": rows(&alice),
            "bob": rows(&bob),
            "carol": rows(&carol),
        },
        "columns": {
            "Credential": hex::encode(db_serialize(&credential("bob", &bob_signer).credential).unwrap()),
            "StagedCommit": hex::encode(db_serialize(staged).unwrap()),
            "XmtpInstallationCredential": hex::encode(db_serialize(&XmtpInstallationCredential::new()).unwrap()),
        },
    })
}

/// Write `golden_rows_next.json` from this build. See the module doc: a new
/// file joins [`GOLDEN_FILES`]; a frozen one is never rewritten.
#[cfg(not(target_arch = "wasm32"))]
#[ignore = "writes golden_rows_next.json for a deliberate format change"]
#[xmtp_common::test(unwrap_try = true)]
async fn write_golden_rows() {
    let text = serde_json::to_string_pretty(&build().await)? + "\n";
    std::fs::write(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/sql_key_store/codec/golden_rows_next.json"),
        text,
    )?;
}

struct Golden {
    name: &'static str,
    json: Value,
    stores: BTreeMap<&'static str, Vec<Row>>,
    columns: BTreeMap<String, Vec<u8>>,
}

impl Golden {
    fn group(&self, name: &str) -> GroupId {
        GroupId::from_slice(&hex::decode(self.json["groups"][name].as_str().unwrap()).unwrap())
    }

    fn rows(&self) -> impl Iterator<Item = &Row> {
        self.stores.values().flatten()
    }
}

fn parse(file: &GoldenFile) -> Golden {
    let json: Value = serde_json::from_str(file.text).unwrap();
    let rows = |rows: &Value| -> Vec<Row> {
        rows.as_array()
            .unwrap()
            .iter()
            .map(|row| Row {
                label: row["label"].as_str().unwrap().to_owned(),
                key: hex::decode(row["key"].as_str().unwrap()).unwrap(),
                value: hex::decode(row["value"].as_str().unwrap()).unwrap(),
            })
            .collect()
    };
    Golden {
        name: file.name,
        stores: STORES
            .into_iter()
            .map(|store| (store, rows(&json["stores"][store])))
            .collect(),
        columns: json["columns"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(name, value)| (name.clone(), hex::decode(value.as_str().unwrap()).unwrap()))
            .collect(),
        json,
    }
}

fn goldens() -> Vec<Golden> {
    GOLDEN_FILES.iter().map(parse).collect()
}

/// The key part of a `label || label || key || version || version` storage
/// key that holds one length-prefixed byte string.
fn doubled_key_bytes(row: &Row) -> &[u8] {
    let label = row.label.len();
    let key = &row.key[2 * label..row.key.len() - 4];
    key::decode_bytes(key).expect("a length-prefixed key")
}

/// A hash reference from its raw bytes, through its TLS encoding.
fn hash_ref(bytes: &[u8]) -> KeyPackageRef {
    KeyPackageRef::tls_deserialize_exact(tls(&VLBytes::new(bytes.to_vec()))).unwrap()
}

fn cbor(bytes: &[u8]) -> Cbor {
    Cbor::decode(&mut SliceReader::new(bytes)).unwrap()
}

/// The CBOR of `bytes` with every map's entries in a fixed order, so two
/// encodings of a value with a hash map compare equal.
fn canonical(bytes: &[u8]) -> Vec<u8> {
    fn sort(value: Cbor) -> Cbor {
        match value {
            Cbor::Array(items) => Cbor::Array(items.into_iter().map(sort).collect()),
            Cbor::Map(entries) => {
                let mut entries: Vec<(Vec<u8>, Cbor, Cbor)> = entries
                    .into_iter()
                    .map(|(key, value)| {
                        let key = sort(key);
                        (encode(&key), key, sort(value))
                    })
                    .collect();
                entries.sort_by(|a, b| a.0.cmp(&b.0));
                Cbor::Map(
                    entries
                        .into_iter()
                        .map(|(_, key, value)| (key, value))
                        .collect(),
                )
            }
            Cbor::Tag(tag, inner) => Cbor::Tag(tag, Box::new(sort(*inner))),
            other => other,
        }
    }
    fn encode(value: &Cbor) -> Vec<u8> {
        let mut writer = cbor4ii::core::utils::BufWriter::new(Vec::new());
        value.encode(&mut writer).unwrap();
        writer.into_inner()
    }
    encode(&sort(cbor(bytes)))
}

/// Decode `bytes` as a `T` and check that it encodes back to the same CBOR.
/// A field the decoder does not read, or reads as a default, fails.
fn round_trips<T: serde::Serialize + serde::de::DeserializeOwned>(what: &str, bytes: &[u8]) {
    let decoded: T = value_from_slice(bytes).unwrap_or_else(|error| panic!("{what}: {error}"));
    let encoded = value_to_vec(&decoded).unwrap();
    assert_eq!(
        canonical(&encoded),
        canonical(bytes),
        "{what} does not encode back to its stored bytes"
    );
}

/// Every map key path in a stored value, with array items merged, and every
/// text value with its path (a unit variant is stored as its name). A list
/// row is an array of byte strings, each an encoded value: its items' paths
/// count.
fn key_paths(row: &Row, paths: &mut BTreeSet<String>) {
    fn walk(value: &Cbor, prefix: &str, paths: &mut BTreeSet<String>) {
        match value {
            Cbor::Map(entries) => {
                for (key, value) in entries {
                    let name = match key {
                        Cbor::Text(text) => text.clone(),
                        _ => "*".to_owned(),
                    };
                    let path = format!("{prefix}.{name}");
                    paths.insert(path.clone());
                    walk(value, &path, paths);
                }
            }
            Cbor::Array(items) => {
                for item in items {
                    walk(item, &format!("{prefix}[]"), paths);
                }
            }
            Cbor::Tag(_, inner) => walk(inner, prefix, paths),
            // A unit variant is stored as its name.
            Cbor::Text(text) => {
                paths.insert(format!("{prefix}={text}"));
            }
            _ => {}
        }
    }
    if matches!(row.label.as_str(), "OwnLeafNodes" | "ProposalQueueRefs") {
        for item in decode_list(row.label.as_bytes(), &row.value)
            .unwrap()
            .iter()
        {
            walk(&cbor(item), "[]", paths);
        }
    } else {
        walk(&cbor(&row.value), "", paths);
    }
}

/// The key paths of every label's rows in a store, and of every column.
fn shapes(golden: &Golden) -> BTreeMap<String, BTreeSet<String>> {
    let mut shapes: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (store, rows) in &golden.stores {
        for row in rows {
            key_paths(
                row,
                shapes.entry(format!("{store} {}", row.label)).or_default(),
            );
        }
    }
    for (name, value) in &golden.columns {
        let row = Row {
            label: name.clone(),
            key: vec![],
            value: value.clone(),
        };
        key_paths(&row, shapes.entry(format!("column {name}")).or_default());
    }
    shapes
}

/// Every text string in the stored values: field names, variant names and
/// text fields.
fn texts(golden: &Golden) -> BTreeSet<String> {
    fn walk(value: &Cbor, texts: &mut BTreeSet<String>) {
        match value {
            Cbor::Text(text) => {
                texts.insert(text.clone());
            }
            Cbor::Array(items) => items.iter().for_each(|item| walk(item, texts)),
            Cbor::Map(entries) => entries.iter().for_each(|(key, value)| {
                walk(key, texts);
                walk(value, texts);
            }),
            Cbor::Tag(_, inner) => walk(inner, texts),
            _ => {}
        }
    }
    let mut texts = BTreeSet::new();
    for row in golden.rows() {
        let mut values = vec![row.value.clone()];
        if matches!(row.label.as_str(), "OwnLeafNodes" | "ProposalQueueRefs") {
            values = decode_list(row.label.as_bytes(), &row.value)
                .unwrap()
                .to_vec();
        }
        for value in values {
            walk(&cbor(&value), &mut texts);
        }
    }
    for value in golden.columns.values() {
        walk(&cbor(value), &mut texts);
    }
    texts
}

/// Each frozen file is unchanged.
#[xmtp_common::test(unwrap_try = true)]
fn golden_files_are_frozen() {
    for file in GOLDEN_FILES {
        assert_eq!(
            hex::encode(xmtp_cryptography::hash::sha256_array(file.text.as_bytes())),
            file.sha256,
            "{} changed; frozen golden files are never edited",
            file.name
        );
    }
}

/// Every label the key store writes has a row in each frozen file, and each
/// row's label matches its key.
#[xmtp_common::test(unwrap_try = true)]
fn golden_rows_cover_every_label() {
    let labels = store_labels();
    assert_eq!(labels.len(), 21, "the store's labels: {labels:?}");
    for golden in goldens() {
        let stored: BTreeSet<&str> = golden
            .rows()
            .map(|row| {
                assert_eq!(row.label, label_of(&row.key));
                row.label.as_str()
            })
            .collect();
        assert_eq!(stored, labels, "{}", golden.name);
        assert_eq!(
            golden
                .columns
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            COLUMNS,
            "{}",
            golden.name
        );
    }
}

/// The frozen files hold the shapes XMTP groups store, not only a plain
/// OpenMLS group: Unknown extensions, RequiredCapabilities, decryption
/// ratchets, past epoch trees for XMTP's past epochs, an inactive group, and
/// an own staged commit with a proposal and a new leaf.
#[xmtp_common::test(unwrap_try = true)]
fn golden_rows_hold_production_shapes() {
    for golden in goldens() {
        let texts = texts(&golden);
        for shape in [
            "Unknown",
            "RequiredCapabilities",
            "LastResort",
            "ApplicationId",
            "AppDataDictionary",
            "DecryptionRatchet",
            "EncryptionRatchet",
            "Inactive",
            "Operational",
            "PendingCommit",
            "Member",
        ] {
            assert!(texts.contains(shape), "{} has no {shape}", golden.name);
        }

        // Message secrets that keep XMTP's past epochs, with a past epoch.
        let past_epochs = golden
            .rows()
            .filter(|row| row.label == "MessageSecrets")
            .filter_map(|row| match cbor(&row.value) {
                Cbor::Map(entries) => Some(entries),
                _ => None,
            })
            .any(|entries| {
                let field = |name: &str| {
                    entries
                        .iter()
                        .find(|(key, _)| matches!(key, Cbor::Text(text) if text == name))
                        .map(|(_, value)| value)
                };
                matches!(field("max_epochs"), Some(Cbor::Integer(epochs)) if *epochs == MAX_PAST_EPOCHS as i128)
                    && matches!(field("past_epoch_trees"), Some(Cbor::Array(trees)) if !trees.is_empty())
            });
        assert!(past_epochs, "{} has no past epoch tree", golden.name);

        // The own staged commit holds a proposal and a new leaf key pair.
        let paths = shapes(&golden);
        let staged = &paths["column StagedCommit"];
        assert!(
            staged
                .iter()
                .any(|path| path.starts_with(".state.GroupMember.new_keypairs[].")),
            "{} has no staged new leaf key pair: {staged:?}",
            golden.name
        );
        assert!(
            staged.iter().any(|path| path.contains("Add")),
            "{} has no staged Add proposal: {staged:?}",
            golden.name
        );
    }
}

/// The rows this build writes, from the same steps, have the same field and
/// variant names as the newest frozen file. A dependency bump that renames,
/// adds or removes a stored field fails here; see the module doc for how to
/// add a new frozen file on purpose.
#[xmtp_common::test(unwrap_try = true)]
async fn this_build_writes_the_newest_golden_shapes() {
    let newest = parse(GOLDEN_FILES.last().unwrap());
    let fresh = build().await;
    let fresh = Golden {
        name: "this build",
        stores: STORES
            .into_iter()
            .map(|store| {
                let rows = fresh["stores"][store]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|row| Row {
                        label: row["label"].as_str().unwrap().to_owned(),
                        key: hex::decode(row["key"].as_str().unwrap()).unwrap(),
                        value: hex::decode(row["value"].as_str().unwrap()).unwrap(),
                    })
                    .collect();
                (store, rows)
            })
            .collect(),
        columns: fresh["columns"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(name, value)| (name.clone(), hex::decode(value.as_str().unwrap()).unwrap()))
            .collect(),
        json: fresh,
    };
    let (frozen, now) = (shapes(&newest), shapes(&fresh));
    assert_eq!(
        frozen.keys().collect::<Vec<_>>(),
        now.keys().collect::<Vec<_>>(),
        "this build writes other labels than {}",
        newest.name
    );
    for (label, paths) in &frozen {
        let written = &now[label];
        assert!(
            paths == written,
            "{label}: this build writes another shape than {}.\nonly frozen: {:?}\nonly written: {:?}",
            newest.name,
            paths.difference(written).collect::<Vec<_>>(),
            written.difference(paths).collect::<Vec<_>>(),
        );
    }
}

/// Rows and columns with a public type decode with it and encode back to the
/// same CBOR.
#[xmtp_common::test(unwrap_try = true)]
fn golden_rows_round_trip_their_public_types() {
    for golden in goldens() {
        let mut checked = BTreeSet::new();
        for row in golden.rows() {
            let what = format!("{} {}", golden.name, row.label);
            match row.label.as_str() {
                "KeyPackage" => round_trips::<KeyPackageBundle>(&what, &row.value),
                "SignatureKeyPair" => round_trips::<SignatureKeyPair>(&what, &row.value),
                "GroupContext" => round_trips::<openmls::group::GroupContext>(&what, &row.value),
                "KeyPackageReferences" => round_trips::<KeyPackageRef>(&what, &row.value),
                "KeyPackageWrapperPrivateKey" | "CommitLogSignerPrivateKey" => {
                    round_trips::<Vec<u8>>(&what, &row.value)
                }
                "OwnLeafNodeIndex" => {
                    round_trips::<openmls::prelude::LeafNodeIndex>(&what, &row.value)
                }
                "MlsGroupJoinConfig" => round_trips::<MlsGroupJoinConfig>(&what, &row.value),
                "QueuedProposal" => {
                    round_trips::<openmls::prelude::QueuedProposal>(&what, &row.value)
                }
                "GroupState" => round_trips::<openmls::group::MlsGroupState>(&what, &row.value),
                "OwnLeafNodes" => {
                    let items = decode_list(row.label.as_bytes(), &row.value)?;
                    assert_eq!(
                        *super::encode_list(row.label.as_bytes(), &items)?,
                        row.value
                    );
                    for item in items.iter() {
                        round_trips::<openmls::prelude::LeafNode>(&what, item);
                    }
                }
                "ProposalQueueRefs" => {
                    let items = decode_list(row.label.as_bytes(), &row.value)?;
                    assert_eq!(
                        *super::encode_list(row.label.as_bytes(), &items)?,
                        row.value
                    );
                    for item in items.iter() {
                        round_trips::<KeyPackageRef>(&what, item);
                    }
                }
                _ => continue,
            }
            checked.insert(row.label.as_str());
        }
        assert_eq!(checked.len(), 12, "{}: {checked:?}", golden.name);
        round_trips::<Credential>("Credential", &golden.columns["Credential"]);
        round_trips::<XmtpInstallationCredential>(
            "XmtpInstallationCredential",
            &golden.columns["XmtpInstallationCredential"],
        );
        round_trips::<StagedCommit>("StagedCommit", &golden.columns["StagedCommit"]);
    }
}

/// Cut and corrupted copies of the rows with a public type are clean errors,
/// never panics: every proper prefix fails, and every single-byte change
/// either fails or decodes.
#[xmtp_common::test(unwrap_try = true)]
fn cut_and_corrupted_golden_rows_are_errors() {
    let mut checked = 0;
    for golden in goldens() {
        for row in golden.rows() {
            let decodes = |bytes: &[u8]| -> Option<bool> {
                Some(match row.label.as_str() {
                    "KeyPackage" => value_from_slice::<KeyPackageBundle>(bytes).is_ok(),
                    "SignatureKeyPair" => value_from_slice::<SignatureKeyPair>(bytes).is_ok(),
                    "GroupContext" => {
                        value_from_slice::<openmls::group::GroupContext>(bytes).is_ok()
                    }
                    "KeyPackageReferences" => value_from_slice::<KeyPackageRef>(bytes).is_ok(),
                    "OwnLeafNodes" | "ProposalQueueRefs" => {
                        decode_list(row.label.as_bytes(), bytes).is_ok()
                    }
                    _ => return None,
                })
            };
            if decodes(&row.value).is_none() {
                continue;
            }
            assert_eq!(decodes(&row.value), Some(true), "{}", row.label);
            for end in 0..row.value.len() {
                assert_eq!(
                    decodes(&row.value[..end]),
                    Some(false),
                    "{} cut at {end}",
                    row.label
                );
            }
            for index in 0..row.value.len() {
                let mut corrupt = row.value.clone();
                corrupt[index] ^= 0xff;
                let _ = decodes(&corrupt);
            }
            checked += 1;
        }
    }
    assert!(checked >= 8, "only {checked} rows checked");
}

/// OpenMLS reads the crate-private rows itself. Restored into new stores:
/// - the key package and signature key rows read back through their keys,
///   and every encryption key pair reads back;
/// - Bob decrypts the epoch-1 message from the past epoch trees;
/// - Bob commits Alice's stored self-update, which Alice can only process
///   with her stored own leaf node and its key pair;
/// - Alice and Bob exchange messages;
/// - Carol's group loads inactive;
/// - Alice's second group loads with its staged commit, and the staged
///   commit column merges into it.
#[xmtp_common::test(unwrap_try = true)]
async fn golden_rows_load_and_run_the_groups() {
    for golden in goldens() {
        let mut providers = BTreeMap::new();
        for (store, rows) in &golden.stores {
            let provider = provider().await;
            restore(&provider, rows);
            providers.insert(*store, provider);
        }
        let (alice, bob, carol) = (&providers["alice"], &providers["bob"], &providers["carol"]);

        let mut signers = BTreeMap::new();
        for (store, rows) in &golden.stores {
            let provider = &providers[store];
            for row in rows {
                match row.label.as_str() {
                    "EncryptionKeyPair" => {
                        let public = EncryptionKey::from(doubled_key_bytes(row).to_vec());
                        let pair = read_keys_from_key_store(provider, &public);
                        assert_eq!(pair.public, doubled_key_bytes(row));
                    }
                    "KeyPackage" => {
                        let bundle: Option<KeyPackageBundle> = provider
                            .storage()
                            .key_package(&hash_ref(doubled_key_bytes(row)))?;
                        assert!(bundle.is_some(), "{store}'s key package does not read back");
                    }
                    "SignatureKeyPair" => {
                        // The storage id is the public key, a label, and the
                        // scheme as a u16 big endian.
                        const ID_SUFFIX: &[u8] = b"RustCryptoSignatureKey";
                        let id = doubled_key_bytes(row);
                        let public = &id[..id.len() - ID_SUFFIX.len() - 2];
                        assert_eq!(&id[public.len()..id.len() - 2], ID_SUFFIX);
                        let signer = SignatureKeyPair::read(
                            provider.storage(),
                            public,
                            SignatureScheme::ED25519,
                        )
                        .expect("the signature key reads back through its key");
                        signers.insert(*store, signer);
                    }
                    _ => {}
                }
            }
        }

        let first = golden.group("first");
        let mut alice_group = MlsGroup::load(alice.storage(), &first)?.unwrap();
        let mut bob_group = MlsGroup::load(bob.storage(), &first)?.unwrap();
        assert_eq!(alice_group.members().count(), 2);
        assert_eq!(alice_group.pending_proposals().count(), 1);
        assert_eq!(bob_group.pending_proposals().count(), 1);

        let late = hex::decode(golden.json["late_message"].as_str().unwrap())?;
        assert_eq!(read_message(bob, &mut bob_group, &late), LATE_MESSAGE);

        let (commit, _, _) = bob_group.commit_to_pending_proposals(bob, &signers["bob"])?;
        bob_group.merge_pending_commit(bob)?;
        merge_commit(alice, &mut alice_group, &tls(&commit));
        assert_eq!(alice_group.epoch(), bob_group.epoch());
        let from_alice = tls(&alice_group.create_message(alice, &signers["alice"], b"after")?);
        assert_eq!(read_message(bob, &mut bob_group, &from_alice), b"after");
        let from_bob = tls(&bob_group.create_message(bob, &signers["bob"], b"reply")?);
        assert_eq!(read_message(alice, &mut alice_group, &from_bob), b"reply");

        let carol_group = MlsGroup::load(carol.storage(), &first)?.unwrap();
        assert!(!carol_group.is_active());

        let mut second = MlsGroup::load(alice.storage(), &golden.group("second"))?.unwrap();
        assert!(second.pending_commit().is_some());
        let staged: StagedCommit = db_deserialize(&golden.columns["StagedCommit"])?;
        let epoch = second.epoch();
        second.merge_staged_commit(alice, staged)?;
        assert_eq!(second.epoch().as_u64(), epoch.as_u64() + 1);
        assert_eq!(second.members().count(), 2);

        // The XMTP labels read back as the production readers read them.
        for row in golden.rows() {
            let label = row.label.as_bytes();
            match row.label.as_str() {
                "KeyPackageReferences" => drop(decode_value::<KeyPackageRef>(label, &row.value)?),
                "KeyPackageWrapperPrivateKey" | "CommitLogSignerPrivateKey" => {
                    assert_eq!(decode_value::<Vec<u8>>(label, &row.value)?.len(), 32)
                }
                _ => {}
            }
        }
        db_deserialize::<Credential>(&golden.columns["Credential"])?;
        db_deserialize::<XmtpInstallationCredential>(
            &golden.columns["XmtpInstallationCredential"],
        )?;
    }
}
