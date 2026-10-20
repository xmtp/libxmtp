//! Golden rows: forward-compatibility guard for the stored CBOR.
//!
//! `golden_rows.json` holds key store rows that this build wrote for every
//! label libxmtp stores, plus the `db_serialize` columns that hold OpenMLS
//! types. Each row is decoded here with the type that reads it in production.
//! Most OpenMLS value types are crate-private, so those rows are read the way
//! OpenMLS reads them: `MlsGroup::load`, a commit that both members process,
//! and an application message.
//!
//! A dependency bump that renames, removes or retypes a stored field makes
//! these tests fail: every database written since the CBOR switch holds rows
//! like these. Adding a field with a serde default is safe and keeps them
//! passing.
//!
//! Update the file only for a deliberate format change, together with a plan
//! for the rows that existing databases hold. Regenerate it with
//! `dev/nix-shell 'just test workspace -p xmtp_db write_golden_rows --run-ignored only'`
//! (native only), and say in the commit why the stored format changed.

use std::collections::{BTreeMap, BTreeSet};

use diesel::prelude::*;
use openmls::{
    credentials::Credential,
    group::{GroupId, MlsGroup, StagedCommit},
    key_packages::KeyPackageBundle,
    prelude::{
        KeyPackageRef, MlsMessageIn, ProcessedMessageContent, ProtocolMessage,
        tls_codec::{Deserialize as _, Serialize as _},
    },
    treesync::{EncryptionKey, test_utils::read_keys_from_key_store},
};
use openmls_basic_credential::SignatureKeyPair;
use openmls_traits::{OpenMlsProvider, storage::CURRENT_VERSION};
use serde_json::Value;
use xmtp_cryptography::XmtpInstallationCredential;

use super::key;
use crate::{
    ConnectionExt, TestDb, XmtpOpenMlsProvider, XmtpTestDb, db_deserialize,
    sql_key_store::SqlKeyStore,
};

const GOLDEN: &str = include_str!("golden_rows.json");

/// Every label libxmtp writes. OpenMLS's own labels come first.
const LABELS: [&str; 21] = [
    "KeyPackage",
    "EncryptionKeyPair",
    "SignatureKeyPair",
    "EpochKeyPairs",
    "Tree",
    "GroupContext",
    "InterimTranscriptHash",
    "ConfirmationTag",
    "OwnLeafNodeIndex",
    "EpochSecrets",
    "MessageSecrets",
    "MlsGroupJoinConfig",
    "OwnLeafNodes",
    "GroupState",
    "QueuedProposal",
    "ProposalQueueRefs",
    "ResumptionPskStore",
    "ApplicationExportTree",
    "KeyPackageReferences",
    "KeyPackageWrapperPrivateKey",
    "CommitLogSignerPrivateKey",
];

const MESSAGE: &[u8] = b"golden rows";

type Provider = XmtpOpenMlsProvider<crate::DefaultMlsStore>;

#[derive(Debug)]
struct Row {
    label: String,
    key: Vec<u8>,
    value: Vec<u8>,
}

/// The label of a storage key: the longest label it starts with.
fn label_of(storage_key: &[u8]) -> &'static str {
    LABELS
        .iter()
        .filter(|label| storage_key.starts_with(label.as_bytes()))
        .max_by_key(|label| label.len())
        .unwrap_or_else(|| panic!("unknown label in {}", hex::encode(storage_key)))
}

async fn provider() -> Provider {
    let store = TestDb::create_persistent_store(None).await;
    XmtpOpenMlsProvider::new(SqlKeyStore::new(store.db()))
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

fn process(provider: &Provider, group: &mut MlsGroup, message: &[u8]) -> ProcessedMessageContent {
    let message = MlsMessageIn::tls_deserialize_exact(message).unwrap();
    let message: ProtocolMessage = message.try_into_protocol_message().unwrap();
    group
        .process_message(provider, message)
        .unwrap()
        .into_content()
}

struct Golden {
    group_id: GroupId,
    alice: Vec<Row>,
    bob: Vec<Row>,
    columns: BTreeMap<String, Vec<u8>>,
}

fn golden() -> Golden {
    let golden: Value = serde_json::from_str(GOLDEN).unwrap();
    let rows = |name: &str| -> Vec<Row> {
        golden["stores"][name]
            .as_array()
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
        group_id: GroupId::from_slice(&hex::decode(golden["group_id"].as_str().unwrap()).unwrap()),
        alice: rows("alice"),
        bob: rows("bob"),
        columns: golden["columns"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(name, value)| (name.clone(), hex::decode(value.as_str().unwrap()).unwrap()))
            .collect(),
    }
}

/// The key part of a `label || label || key || version || version` storage
/// key that holds one length-prefixed byte string.
fn doubled_key_bytes(row: &Row) -> &[u8] {
    let label = row.label.len();
    let key = &row.key[2 * label..row.key.len() - 4];
    key::decode_bytes(key).expect("a length-prefixed key")
}

/// The golden file has a row for every label libxmtp writes, and its labels
/// match their keys.
#[xmtp_common::test(unwrap_try = true)]
fn golden_rows_cover_every_label() {
    let golden = golden();
    let labels: BTreeSet<&str> = golden
        .alice
        .iter()
        .chain(&golden.bob)
        .map(|row| {
            assert_eq!(row.label, label_of(&row.key));
            row.label.as_str()
        })
        .collect();
    assert_eq!(labels, BTreeSet::from(LABELS));
    assert_eq!(
        golden
            .columns
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["Credential", "StagedCommit", "XmtpInstallationCredential"]
    );
}

/// Rows whose type is public decode with that type, and the columns decode
/// with the types that read them.
#[xmtp_common::test(unwrap_try = true)]
fn golden_rows_decode_as_their_public_types() {
    let golden = golden();
    for row in golden.alice.iter().chain(&golden.bob) {
        let label = row.label.as_bytes();
        let decoded = match row.label.as_str() {
            "KeyPackage" => super::decode_value::<KeyPackageBundle>(label, &row.value).map(drop),
            "SignatureKeyPair" => {
                super::decode_value::<SignatureKeyPair>(label, &row.value).map(drop)
            }
            "GroupContext" => {
                super::decode_value::<openmls::group::GroupContext>(label, &row.value).map(drop)
            }
            "KeyPackageReferences" => {
                super::decode_value::<KeyPackageRef>(label, &row.value).map(drop)
            }
            "KeyPackageWrapperPrivateKey" | "CommitLogSignerPrivateKey" => {
                super::decode_value::<Vec<u8>>(label, &row.value).map(drop)
            }
            "OwnLeafNodes" | "ProposalQueueRefs" => super::decode_list(label, &row.value).map(drop),
            _ => continue,
        };
        assert!(decoded.is_ok(), "{} does not decode", row.label);
    }
    db_deserialize::<Credential>(&golden.columns["Credential"])?;
    db_deserialize::<XmtpInstallationCredential>(&golden.columns["XmtpInstallationCredential"])?;
    db_deserialize::<StagedCommit>(&golden.columns["StagedCommit"])?;
}

/// Cut and corrupted copies of the rows with a public type are clean errors,
/// never panics: every proper prefix fails, and every single-byte change
/// either fails or decodes.
#[xmtp_common::test(unwrap_try = true)]
fn cut_and_corrupted_golden_rows_are_errors() {
    use super::value_from_slice;
    let golden = golden();
    let mut checked = 0;
    for row in golden.alice.iter().chain(&golden.bob) {
        let decodes = |bytes: &[u8]| -> Option<bool> {
            Some(match row.label.as_str() {
                "KeyPackage" => value_from_slice::<KeyPackageBundle>(bytes).is_ok(),
                "SignatureKeyPair" => value_from_slice::<SignatureKeyPair>(bytes).is_ok(),
                "GroupContext" => value_from_slice::<openmls::group::GroupContext>(bytes).is_ok(),
                "KeyPackageReferences" => value_from_slice::<KeyPackageRef>(bytes).is_ok(),
                "OwnLeafNodes" | "ProposalQueueRefs" => {
                    super::decode_list(row.label.as_bytes(), bytes).is_ok()
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
    assert!(checked >= 6, "only {checked} rows checked");
}

/// OpenMLS reads the rest of the rows itself. Restored into new stores, both
/// members load the group, read their encryption keys, commit Alice's pending
/// update, and exchange a message.
#[xmtp_common::test(unwrap_try = true)]
async fn golden_rows_load_and_run_the_group() {
    let golden = golden();
    let (alice, bob) = (provider().await, provider().await);
    restore(&alice, &golden.alice);
    restore(&bob, &golden.bob);

    for (provider, rows) in [(&alice, &golden.alice), (&bob, &golden.bob)] {
        for row in rows.iter().filter(|row| row.label == "EncryptionKeyPair") {
            let public = EncryptionKey::from(doubled_key_bytes(row).to_vec());
            let pair = read_keys_from_key_store(provider, &public);
            assert_eq!(pair.public, doubled_key_bytes(row));
        }
    }
    let signer = |rows: &[Row]| -> SignatureKeyPair {
        let row = rows
            .iter()
            .find(|row| row.label == "SignatureKeyPair")
            .unwrap();
        super::decode_value(row.label.as_bytes(), &row.value).unwrap()
    };
    let (alice_signer, bob_signer) = (signer(&golden.alice), signer(&golden.bob));

    let mut alice_group = MlsGroup::load(alice.storage(), &golden.group_id)?.unwrap();
    let mut bob_group = MlsGroup::load(bob.storage(), &golden.group_id)?.unwrap();
    assert_eq!(alice_group.members().count(), 2);
    assert_eq!(alice_group.pending_proposals().count(), 1);
    assert_eq!(
        alice_group.export_secret(alice.crypto(), "golden", &[], 32)?,
        bob_group.export_secret(bob.crypto(), "golden", &[], 32)?
    );

    // Alice commits her stored self-update proposal: it needs her own leaf
    // node and the new leaf's encryption key. Bob needs his epoch keys to
    // decrypt the path.
    let (commit, _, _) = alice_group.commit_to_pending_proposals(&alice, &alice_signer)?;
    alice_group.merge_pending_commit(&alice)?;
    let ProcessedMessageContent::StagedCommitMessage(staged) =
        process(&bob, &mut bob_group, &commit.tls_serialize_detached()?)
    else {
        panic!("not a commit");
    };
    bob_group.merge_staged_commit(&bob, *staged)?;
    assert_eq!(alice_group.epoch(), bob_group.epoch());

    let message = bob_group.create_message(&bob, &bob_signer, MESSAGE)?;
    let ProcessedMessageContent::ApplicationMessage(received) =
        process(&alice, &mut alice_group, &message.tls_serialize_detached()?)
    else {
        panic!("not an application message");
    };
    assert_eq!(received.into_bytes(), MESSAGE);
}

#[cfg(not(target_arch = "wasm32"))]
mod writer {
    //! Writes `golden_rows.json`. Native only: it writes a file.
    use super::*;
    use openmls::{
        credentials::{BasicCredential, CredentialWithKey},
        extensions::{
            AppDataDictionary, AppDataDictionaryExtension, Extension, ExtensionType, Extensions,
        },
        group::{MlsGroupCreateConfig, MlsGroupJoinConfig, StagedWelcome},
        key_packages::KeyPackage,
        prelude::{Capabilities, LeafNodeParameters, MlsMessageBodyIn},
    };
    use serde_json::json;
    use xmtp_cryptography::configuration::CIPHERSUITE;

    use super::super::encode_value;
    use crate::{
        db_serialize,
        sql_key_store::{
            COMMIT_LOG_SIGNER_PRIVATE_KEY, KEY_PACKAGE_REFERENCES, KEY_PACKAGE_WRAPPER_PRIVATE_KEY,
        },
    };

    const GOLDEN_PATH: &str = "src/sql_key_store/codec/golden_rows.json";
    /// `ComponentId::COMMIT_LOG_SIGNER` in `xmtp_mls_common`, as XMTP groups use it.
    const COMMIT_LOG_SIGNER: u16 = 0x800B;
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
                label: label_of(&row.key_bytes).to_owned(),
                key: row.key_bytes,
                value: row.value_bytes,
            })
            .collect()
    }

    fn credential(name: &str, signer: &SignatureKeyPair) -> CredentialWithKey {
        CredentialWithKey {
            credential: BasicCredential::new(name.as_bytes().to_vec()).into(),
            signature_key: signer.to_public_vec().into(),
        }
    }

    fn capabilities() -> Capabilities {
        Capabilities::new(
            None,
            None,
            Some(&[ExtensionType::AppDataDictionary]),
            None,
            None,
        )
    }

    /// The rows and columns of a two-member group, as this build writes them.
    ///
    /// - Alice creates a group with an AppData dictionary (as XMTP groups have),
    ///   adds Bob, and leaves a self-update proposal pending, so her store holds
    ///   a queued proposal, the proposal refs list and an own leaf node.
    /// - Bob joins from the Welcome and keeps a second, unused key package.
    /// - Bob's store also holds the XMTP labels for that key package and a
    ///   commit log signer key.
    /// - The columns are Bob's credential, an installation key, and the staged
    ///   commit Bob makes from Alice's next commit.
    async fn build() -> Value {
        let (alice, bob) = (provider().await, provider().await);
        let alice_signer = SignatureKeyPair::new(CIPHERSUITE.signature_algorithm()).unwrap();
        alice_signer.store(alice.storage()).unwrap();
        let bob_signer = SignatureKeyPair::new(CIPHERSUITE.signature_algorithm()).unwrap();
        bob_signer.store(bob.storage()).unwrap();

        let key_package = |provider: &Provider| {
            KeyPackage::builder()
                .leaf_node_capabilities(capabilities())
                .build(
                    CIPHERSUITE,
                    provider,
                    &bob_signer,
                    credential("bob", &bob_signer),
                )
                .unwrap()
        };
        let joining = key_package(&bob);
        let spare = key_package(&bob);

        let mut dictionary = AppDataDictionary::new();
        let _ = dictionary.insert(COMMIT_LOG_SIGNER, vec![0x42; 32]);
        let config = MlsGroupCreateConfig::builder()
            .ciphersuite(CIPHERSUITE)
            .use_ratchet_tree_extension(true)
            .capabilities(capabilities())
            .with_group_context_extensions(
                Extensions::single(Extension::AppDataDictionary(
                    AppDataDictionaryExtension::new(dictionary),
                ))
                .unwrap(),
            )
            .build();
        let mut group = MlsGroup::new(
            &alice,
            &alice_signer,
            &config,
            credential("alice", &alice_signer),
        )
        .unwrap();
        let (_, welcome, _) = group
            .add_members(&alice, &alice_signer, &[joining.key_package().clone()])
            .unwrap();
        group.merge_pending_commit(&alice).unwrap();
        let welcome =
            MlsMessageIn::tls_deserialize_exact(welcome.tls_serialize_detached().unwrap()).unwrap();
        let MlsMessageBodyIn::Welcome(welcome) = welcome.extract() else {
            panic!("not a Welcome");
        };
        let mut bob_group =
            StagedWelcome::new_from_welcome(&bob, &MlsGroupJoinConfig::default(), welcome, None)
                .unwrap()
                .into_group(&bob)
                .unwrap();
        let (proposal, _) = group
            .propose_self_update(&alice, &alice_signer, LeafNodeParameters::default())
            .unwrap();

        // The XMTP labels, as `identity.rs` and `commit_log_key.rs` write them.
        let hash_ref = spare.key_package().hash_ref(bob.crypto()).unwrap();
        let init_key = spare
            .key_package()
            .hpke_init_key()
            .tls_serialize_detached()
            .unwrap();
        let storage = bob.storage();
        storage
            .write::<CURRENT_VERSION>(
                KEY_PACKAGE_REFERENCES,
                &init_key,
                &encode_value(KEY_PACKAGE_REFERENCES, &hash_ref).unwrap(),
            )
            .unwrap();
        let hash_ref_key = key::to_vec(&hash_ref).unwrap();
        storage
            .write::<CURRENT_VERSION>(
                KEY_PACKAGE_WRAPPER_PRIVATE_KEY,
                &hash_ref_key,
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
        let (alice_rows, bob_rows) = (rows(&alice), rows(&bob));

        // A staged commit, from Bob's side of Alice's next commit.
        let bob_proposal = process(
            &bob,
            &mut bob_group,
            &proposal.tls_serialize_detached().unwrap(),
        );
        let ProcessedMessageContent::ProposalMessage(queued) = bob_proposal else {
            panic!("not a proposal");
        };
        bob_group
            .store_pending_proposal(bob.storage(), *queued)
            .unwrap();
        let (commit, _, _) = group
            .commit_to_pending_proposals(&alice, &alice_signer)
            .unwrap();
        let ProcessedMessageContent::StagedCommitMessage(staged) = process(
            &bob,
            &mut bob_group,
            &commit.tls_serialize_detached().unwrap(),
        ) else {
            panic!("not a commit");
        };

        json!({
            "description": "Key store rows and db_serialize columns written by libxmtp with CBOR values. src/sql_key_store/codec/golden_tests.rs builds and reads them.",
            "openmls": "xmtp/openmls b5be619c8ec0a5a7086303c0a8ec59400910c7ee (v0.9.1 with xmtp patches)",
            "group_id": hex::encode(group.group_id().as_slice()),
            "stores": { "alice": alice_rows, "bob": bob_rows },
            "columns": {
                "Credential": hex::encode(db_serialize(&credential("bob", &bob_signer).credential).unwrap()),
                "XmtpInstallationCredential": hex::encode(db_serialize(&XmtpInstallationCredential::new()).unwrap()),
                "StagedCommit": hex::encode(db_serialize(&*staged).unwrap()),
            },
        })
    }

    /// Write `golden_rows.json`. Run only for a deliberate format change; see
    /// the module doc.
    #[ignore]
    #[xmtp_common::test(unwrap_try = true)]
    async fn write_golden_rows() {
        let text = serde_json::to_string_pretty(&build().await)? + "\n";
        std::fs::write(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(GOLDEN_PATH),
            text,
        )?;
    }
}
