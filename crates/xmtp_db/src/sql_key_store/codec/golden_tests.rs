//! Golden rows: the forward-compatibility guard for the stored CBOR.
//!
//! Every database written since the CBOR switch holds rows like the frozen
//! files below. Each file holds the key store rows a build wrote for every
//! label the store writes, from groups shaped like XMTP's, plus the
//! `db_serialize` columns that hold OpenMLS types. Its keys are throwaway test
//! keys made for the file; they protect nothing.
//!
//! The newest file is the format this build writes. The tests check that:
//! - every label the store writes has a row in it (read from the store's
//!   source);
//! - it holds the shapes XMTP stores, by exact key path, and an epoch and a
//!   generation above 255. A shape is a key path, a unit variant's name, or
//!   the CBOR type of the item at a path, so a byte string that turns into an
//!   array of integers changes it;
//! - its rows and columns with a public type decode and encode back to the
//!   same CBOR (map and hash map order aside), so a renamed or dropped field,
//!   optional ones included, fails;
//! - the rows this build writes now, from the same steps, have its shapes,
//!   so a dependency bump that renames, adds, removes or retypes a stored
//!   field fails CI;
//! - its rows that declare a length past their end are errors, not panics.
//!
//! For every frozen file, the tests check that:
//! - OpenMLS reads the crate-private rows the way it reads them in
//!   production: it loads the groups, reads its keys, decrypts a message
//!   from a past epoch and one of a skipped generation, commits stored
//!   proposals, merges a stored staged commit, and exchanges messages. What
//!   the file's steps leave is read from its `expect` block;
//! - cut and corrupted rows with a public type are errors, not panics;
//! - every shape of an older file is also in the newest file, unless it is
//!   in [`REMOVED_SHAPES`].
//!
//! Known limit: CBOR stores an integer in its shortest form, so the fixture's
//! values still read after a stored integer is narrowed to `u16` or `u32`.
//! Only a narrowing to `u8` fails.
//!
//! A frozen file is never edited or rewritten: its SHA-256 is pinned. When a
//! deliberate change makes this build write another shape (a new field,
//! label or variant, or new steps in [`build`]):
//! 1. run `dev/nix-shell 'just test workspace -p xmtp_db write_golden_rows --run-ignored only'`
//!    (native only);
//! 2. rename the new `golden_rows_next.json` to `golden_rows_<release>.json`
//!    and add it last to [`GOLDEN_FILES`] with its hash;
//! 3. keep every older file: rows from every released format must keep
//!    loading. If the new format removes a shape on purpose, list it in
//!    [`REMOVED_SHAPES`] with the reason.
//!
//! Say in the commit why the stored format changed.

use std::collections::{BTreeMap, BTreeSet};

use cbor4ii::core::{Value as Cbor, dec::Decode as _, enc::Encode as _, utils::SliceReader};
use diesel::prelude::*;
use openmls::{
    component::ComponentData,
    credentials::{BasicCredential, Credential, CredentialWithKey},
    extensions::{
        AppDataDictionary, AppDataDictionaryExtension, ApplicationIdExtension, Extension,
        ExtensionType, Extensions, RequiredCapabilitiesExtension, UnknownExtension,
    },
    group::{
        AppDataDictionaryUpdater, AppDataUpdates, GroupId, MlsGroup, MlsGroupCreateConfig,
        MlsGroupJoinConfig, StagedCommit, StagedWelcome,
    },
    key_packages::{KeyPackage, KeyPackageBundle},
    messages::proposals::{AppDataUpdateOperation, AppDataUpdateProposal, Proposal, ProposalType},
    prelude::{
        Capabilities, CredentialType, KeyPackageRef, LeafNodeIndex, LeafNodeParameters,
        MlsMessageBodyIn, MlsMessageIn, ProcessedMessageContent, ProtocolMessage, SignatureScheme,
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
use xmtp_cryptography::{Secret, XmtpInstallationCredential, configuration::CIPHERSUITE};

use super::{decode_list, encode_key, encode_value, key, value_from_slice, value_to_vec};
use crate::{
    ConnectionExt, TestDb, XmtpMlsStorageProvider, XmtpOpenMlsProvider, XmtpTestDb, db_deserialize,
    db_serialize,
    sql_key_store::{KEY_PACKAGE_REFERENCES, KEY_PACKAGE_WRAPPER_PRIVATE_KEY, SqlKeyStore},
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
    sha256: "68dc02b0f01152e964a873cfa41bdaf9f9e01aa0b9a2cb74fcd05fc24ff84e61",
}];

const STORES: [&str; 3] = ["alice", "bob", "carol"];
const COLUMNS: [&str; 3] = ["Credential", "StagedCommit", "XmtpInstallationCredential"];
/// `ComponentId::COMMIT_LOG_SIGNER` in `xmtp_mls_common`, as XMTP groups use it.
const COMMIT_LOG_SIGNER: u16 = 0x800B;
const LATE_MESSAGE: &[u8] = b"sent in epoch 1, read in epoch 2";
/// Self-updates before the group grows, so every stored epoch is above 255
/// and a stored epoch narrowed to `u8` fails to load. Narrowing to `u16` or
/// `u32` stays untested: the fixture would need that many epochs.
const EPOCHS_BEFORE_JOIN: usize = 256;
/// Messages Alice sends in one epoch, so a stored generation is above 255.
const BURST: usize = 300;
/// The draft-10 `last_resort_key_package` component that marks every XMTP key
/// package.
const LAST_RESORT_COMPONENT: u16 = 0x0004;
/// `ComponentId::GROUP_MEMBERSHIP` in `xmtp_mls_common`, which XMTP updates
/// with AppDataUpdate proposals.
const GROUP_MEMBERSHIP: u16 = 0x8003;
const PROPOSED_MEMBERSHIP: &[u8] = b"membership in a standalone proposal";
const COMMITTED_MEMBERSHIP: &[u8] = b"membership in an inline proposal";

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
        .filter_map(byte_string_constant)
        .collect()
}

/// The value of a one-line `const` or `static` of type `&[u8]` or
/// `&[u8; N]` set to a byte string literal, with any visibility and any
/// comment after it.
fn byte_string_constant(line: &str) -> Option<&str> {
    let mut line = line.trim_start();
    if let Some(rest) = line.strip_prefix("pub") {
        line = match rest.strip_prefix('(') {
            Some(scope) => scope.split_once(')')?.1,
            None => rest,
        };
        line = line.trim_start();
    }
    let declaration = line
        .strip_prefix("const ")
        .or_else(|| line.strip_prefix("static "))?;
    let (_, typed) = declaration.split_once(':')?;
    let (ty, value) = typed.split_once('=')?;
    if !ty.trim().starts_with("&[u8") {
        return None;
    }
    Some(value.trim_start().strip_prefix("b\"")?.split_once('"')?.0)
}

/// The items of a list row: a CBOR array of byte strings, each an encoded
/// value (`encode_list`). Any other row holds one value.
fn list_items(value: &[u8]) -> Option<Vec<Vec<u8>>> {
    match cbor(value) {
        Cbor::Array(items) if !items.is_empty() => items
            .into_iter()
            .map(|item| match item {
                Cbor::Bytes(bytes) => Some(bytes),
                _ => None,
            })
            .collect(),
        _ => None,
    }
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
const REQUIRED_EXTENSIONS: [ExtensionType; 2] = [
    ExtensionType::AppDataDictionary,
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

/// A key package shaped like `build_key_package`'s: the last-resort mark in
/// its AppDataDictionary, an Unknown extension, and an ApplicationId leaf
/// extension.
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
            Extensions::from_vec(vec![Extension::Unknown(
                WELCOME_POINTEE_ENCRYPTION_AEAD_TYPES_EXTENSION_ID,
                UnknownExtension(vec![0, 1]),
            )])
            .unwrap(),
        )
        .mark_as_last_resort()
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

/// The dictionary updates of `proposals`, applied to `updater`'s dictionary.
fn dictionary_changes<'a>(
    mut updater: AppDataDictionaryUpdater<'_>,
    proposals: impl IntoIterator<Item = &'a AppDataUpdateProposal>,
) -> Option<AppDataUpdates> {
    for proposal in proposals {
        match proposal.operation() {
            AppDataUpdateOperation::Update(data) => updater.set(ComponentData::from_parts(
                proposal.component_id(),
                data.clone(),
            )),
            AppDataUpdateOperation::Remove => updater.remove(&proposal.component_id()),
        }
    }
    updater.changes()
}

/// Process a commit and merge it. A commit with AppDataUpdate proposals is
/// resolved with their dictionary updates first.
fn merge_commit(provider: &Provider, group: &mut MlsGroup, commit: &[u8]) {
    let staged = match process(provider, group, commit) {
        ProcessedMessageContent::StagedCommitMessage(staged) => *staged,
        ProcessedMessageContent::UnresolvedAppDataCommit(unresolved) => {
            let changes = dictionary_changes(
                group.app_data_dictionary_updater(),
                unresolved.app_data_update_proposals(),
            );
            group
                .stage_app_data_commit(provider, *unresolved, changes)
                .unwrap()
        }
        _ => panic!("not a commit"),
    };
    group.merge_staged_commit(provider, staged).unwrap();
}

/// Commit every pending proposal, with inline `proposals` and `removals`,
/// as XMTP commits: with the dictionary updates of every AppDataUpdate
/// proposal, and with a path when `path` is set. The commit stays pending.
fn commit_pending(
    provider: &Provider,
    group: &mut MlsGroup,
    signer: &SignatureKeyPair,
    proposals: Vec<Proposal>,
    removals: Vec<LeafNodeIndex>,
    path: bool,
) -> Vec<u8> {
    let mut stage = group
        .commit_builder()
        .consume_proposal_store(true)
        .add_proposals(proposals)
        .propose_removals(removals)
        .force_self_update(path)
        .load_psks(provider.storage())
        .unwrap();
    let changes = dictionary_changes(
        stage.app_data_dictionary_updater(),
        stage.app_data_update_proposals(),
    );
    stage.with_app_data_dictionary_updates(changes);
    let bundle = stage
        .build(provider.rand(), provider.crypto(), signer, |_| true)
        .unwrap()
        .stage_commit(provider)
        .unwrap();
    tls(bundle.commit())
}

/// The group membership component in a group's dictionary.
fn membership(group: &MlsGroup) -> Option<&[u8]> {
    group
        .extensions()
        .app_data_dictionary()?
        .dictionary()
        .get(&GROUP_MEMBERSHIP)
}

/// The leaf of the member named `name`.
fn leaf_of(group: &MlsGroup, name: &str) -> LeafNodeIndex {
    group
        .members()
        .find(|member| {
            BasicCredential::try_from(member.credential.clone())
                .is_ok_and(|credential| credential.identity() == name.as_bytes())
        })
        .unwrap()
        .index
}

/// The OpenMLS revision this build is pinned to, from the workspace manifest.
fn openmls_revision() -> &'static str {
    const MANIFEST: &str = include_str!("../../../../../Cargo.toml");
    MANIFEST
        .lines()
        .find_map(|line| {
            line.strip_prefix("openmls = { git = \"https://github.com/xmtp/openmls\", rev = \"")?
                .split_once('"')
                .map(|(revision, _)| revision)
        })
        .expect("the openmls pin in Cargo.toml")
}

/// The rows and columns of XMTP-shaped groups, as this build writes them.
///
/// Group 1 (Alice, Bob, Carol, and Frank, whose store is not kept):
/// - Alice alone first commits [`EPOCHS_BEFORE_JOIN`] self-updates, so every
///   stored epoch is above 255.
/// - Alice adds Bob, Carol and Frank; Alice and Bob send messages in that
///   epoch, so the message secrets hold decryption ratchets. Alice keeps one
///   of those messages for Bob unread.
/// - Alice removes Carol: Carol's group is inactive, and that epoch moves
///   into the past epoch trees.
/// - Alice sends [`BURST`] messages in the next epoch. Bob reads only the
///   last, so his ratchet is at a generation above 255 and holds the secrets
///   of skipped generations; Alice keeps one skipped message for Bob. Bob
///   replies.
/// - Alice proposes a self-update, an AppDataUpdate and the removal of
///   Frank. Both store the proposals, and Alice stores her own new leaf node
///   and its key pair.
///
/// Group 2 (Alice, and Erin, whose store is not kept): Alice proposes adding
/// Dave, then stages, and does not merge, a commit as XMTP does: the Add by
/// reference, an inline AppDataUpdate and an inline Remove of Erin, with the
/// dictionary update and an update path. Her store holds the own staged
/// commit, and the StagedCommit column is that commit.
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
    // Members whose stores are not kept.
    let throwaway = openmls_rust_crypto::OpenMlsRustCrypto::default();
    let unkept = |name: &str| {
        let signer = SignatureKeyPair::new(CIPHERSUITE.signature_algorithm()).unwrap();
        key_package(&throwaway, &signer, name)
    };

    let config = group_config();
    let mut group = MlsGroup::new(
        &alice,
        &alice_signer,
        &config,
        credential("alice", &alice_signer),
    )
    .unwrap();
    for _ in 0..EPOCHS_BEFORE_JOIN {
        group
            .self_update(&alice, &alice_signer, LeafNodeParameters::default())
            .unwrap();
        group.merge_pending_commit(&alice).unwrap();
    }
    let (_, welcome, _) = group
        .add_members(
            &alice,
            &alice_signer,
            &[
                bob_joining.key_package().clone(),
                carol_joining.key_package().clone(),
                unkept("frank").key_package().clone(),
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

    // Messages from Alice and Bob to the others.
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

    // Alice removes Carol.
    let (commit, _, _) = group
        .remove_members(&alice, &alice_signer, &[leaf_of(&group, "carol")])
        .unwrap();
    group.merge_pending_commit(&alice).unwrap();
    merge_commit(&bob, &mut bob_group, &tls(&commit));
    merge_commit(&carol, &mut carol_group, &tls(&commit));
    assert!(!carol_group.is_active());

    // A burst from Alice; Bob reads only its last message.
    let burst: Vec<Vec<u8>> = (0..BURST)
        .map(|index| {
            let text = format!("2 from alice, {index}");
            tls(&group
                .create_message(&alice, &alice_signer, text.as_bytes())
                .unwrap())
        })
        .collect();
    read_message(&bob, &mut bob_group, burst.last().unwrap());
    let skipped = burst[BURST - 2].clone();
    let from_bob = tls(&bob_group
        .create_message(&bob, &bob_signer, b"2 from bob")
        .unwrap());
    read_message(&alice, &mut group, &from_bob);

    // Alice's pending proposals, stored by both.
    let (update, _) = group
        .propose_self_update(&alice, &alice_signer, LeafNodeParameters::default())
        .unwrap();
    let (app_data, _) = group
        .propose_app_data_update(
            &alice,
            &alice_signer,
            GROUP_MEMBERSHIP,
            AppDataUpdateOperation::Update(PROPOSED_MEMBERSHIP.to_vec().into()),
        )
        .unwrap();
    let (remove, _) = group
        .propose_remove_member(&alice, &alice_signer, leaf_of(&group, "frank"))
        .unwrap();
    for proposal in [update, app_data, remove] {
        let ProcessedMessageContent::ProposalMessage(queued) =
            process(&bob, &mut bob_group, &tls(&proposal))
        else {
            panic!("not a proposal");
        };
        bob_group
            .store_pending_proposal(bob.storage(), *queued)
            .unwrap();
    }

    // Group 2: Alice's own staged commit, as XMTP stages one.
    let mut second = MlsGroup::new(
        &alice,
        &alice_signer,
        &config,
        credential("alice", &alice_signer),
    )
    .unwrap();
    second
        .add_members(
            &alice,
            &alice_signer,
            &[unkept("erin").key_package().clone()],
        )
        .unwrap();
    second.merge_pending_commit(&alice).unwrap();
    second
        .propose_add_member(&alice, &alice_signer, unkept("dave").key_package())
        .unwrap();
    let erin = leaf_of(&second, "erin");
    commit_pending(
        &alice,
        &mut second,
        &alice_signer,
        vec![Proposal::AppDataUpdate(Box::new(
            AppDataUpdateProposal::update(GROUP_MEMBERSHIP, COMMITTED_MEMBERSHIP.to_vec()),
        ))],
        vec![erin],
        true,
    );
    let staged = second.pending_commit().expect("an own staged commit");

    // The XMTP labels, through the writers `identity.rs` uses and the
    // commit-log signer writer older clients used.
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
    XmtpMlsStorageProvider::write_wrapper_private_key(
        storage,
        &encode_key(KEY_PACKAGE_WRAPPER_PRIVATE_KEY, &hash_ref).unwrap(),
        &[0x5a; 32],
    )
    .unwrap();
    XmtpMlsStorageProvider::write_commit_log_signer(
        storage,
        group.group_id().as_slice(),
        &[0x24; 32],
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
        "groups": {
            "first": hex::encode(group.group_id().as_slice()),
            "second": hex::encode(second.group_id().as_slice()),
        },
        "late_message": hex::encode(late),
        "skipped_message": hex::encode(skipped),
        "expect": {
            "first_members": group.members().count(),
            "first_pending_proposals": group.pending_proposals().count(),
            "first_members_after_commit": group.members().count()
                - group
                    .pending_proposals()
                    .filter(|queued| matches!(queued.proposal(), Proposal::Remove(_)))
                    .count(),
            "second_members_after_merge": second.members().count()
                + staged.add_proposals().count()
                - staged.remove_proposals().count(),
            "first_membership_after_commit": hex::encode(PROPOSED_MEMBERSHIP),
            "second_membership_after_merge": hex::encode(COMMITTED_MEMBERSHIP),
        },
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
    let mut golden = build().await;
    // Provenance only: no test reads it, so a repin to another source form
    // fails nothing but this writer.
    golden["openmls"] = format!("xmtp/openmls {}", openmls_revision()).into();
    let text = serde_json::to_string_pretty(&golden)? + "\n";
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
    golden_from_json(file.name, serde_json::from_str(file.text).unwrap())
}

/// A golden file's rows and columns, from its JSON.
fn golden_from_json(name: &'static str, json: Value) -> Golden {
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
        name,
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

/// The newest frozen file: the format this build writes.
fn newest() -> Golden {
    parse(GOLDEN_FILES.last().unwrap())
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

/// The stored fields that OpenMLS writes from a `HashMap`, as an array of
/// pairs in the map's iteration order, so two encodings of one value can
/// order them differently. Each is named with another field of its struct,
/// which tells it apart from fields of the same name in other structs:
/// `ProposalQueue.queued_proposals` (`vector_converter`) and the export
/// tree's `Pprf.nodes` (`serialize_hashmap`). The other stored maps are
/// `BTreeMap`s, written in key order.
const HASH_MAP_FIELDS: [(&str, &str); 2] = [
    ("queued_proposals", "proposal_references"),
    ("nodes", "width"),
];

/// The CBOR of `bytes` with every map's entries, and every array written
/// from a hash map ([`HASH_MAP_FIELDS`]), in a fixed order, so two
/// encodings of one value compare equal.
fn canonical(bytes: &[u8]) -> Vec<u8> {
    fn sort(value: Cbor) -> Cbor {
        match value {
            Cbor::Array(items) => Cbor::Array(items.into_iter().map(sort).collect()),
            Cbor::Map(entries) => {
                let has = |name: &str| {
                    entries
                        .iter()
                        .any(|(key, _)| matches!(key, Cbor::Text(text) if text == name))
                };
                let hash_maps: Vec<&str> = HASH_MAP_FIELDS
                    .into_iter()
                    .filter(|(field, sibling)| has(field) && has(sibling))
                    .map(|(field, _)| field)
                    .collect();
                let mut entries: Vec<(Vec<u8>, Cbor, Cbor)> = entries
                    .into_iter()
                    .map(|(key, value)| {
                        let key = sort(key);
                        let value = match (&key, sort(value)) {
                            (Cbor::Text(name), Cbor::Array(mut items))
                                if hash_maps.contains(&name.as_str()) =>
                            {
                                items.sort_by_cached_key(encode);
                                Cbor::Array(items)
                            }
                            (_, value) => value,
                        };
                        (encode(&key), key, value)
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

/// [`canonical`] makes two encodings of one value equal when OpenMLS writes
/// its hash maps in another order. A staged commit with two queued proposals
/// and an export tree with several PPRF nodes is decoded and encoded again
/// many times, each time with new hash map seeds.
#[xmtp_common::test(unwrap_try = true)]
async fn canonical_orders_hash_map_fields() {
    let (alice, bob, carol) = (provider().await, provider().await, provider().await);
    let signer = |provider: &Provider| {
        let signer = SignatureKeyPair::new(CIPHERSUITE.signature_algorithm()).unwrap();
        signer.store(provider.storage()).unwrap();
        signer
    };
    let alice_signer = signer(&alice);
    let mut group = MlsGroup::new(
        &alice,
        &alice_signer,
        &group_config(),
        credential("alice", &alice_signer),
    )?;
    for (provider, name) in [(&bob, "bob"), (&carol, "carol")] {
        let joining = key_package(provider, &signer(provider), name);
        group.propose_add_member(&alice, &alice_signer, joining.key_package())?;
    }
    group
        .commit_builder()
        .consume_proposal_store(true)
        .load_psks(alice.storage())?
        .build(alice.rand(), alice.crypto(), &alice_signer, |_| true)?
        .stage_commit(&alice)?;
    let mut staged: StagedCommit =
        value_from_slice(&db_serialize(group.pending_commit().unwrap())?)?;
    for component in [0x8001, 0x8002, 0x8003] {
        staged.safe_export_secret(alice.crypto(), component)?;
    }
    let bytes = value_to_vec(&staged)?;

    // Both hash maps hold more than one entry.
    fn longest(value: &Cbor, field: &str) -> usize {
        match value {
            Cbor::Map(entries) => entries
                .iter()
                .map(|(key, value)| match (key, value) {
                    (Cbor::Text(name), Cbor::Array(items)) if name == field => items.len(),
                    _ => longest(value, field),
                })
                .max()
                .unwrap_or(0),
            Cbor::Array(items) => items
                .iter()
                .map(|item| longest(item, field))
                .max()
                .unwrap_or(0),
            Cbor::Tag(_, inner) => longest(inner, field),
            _ => 0,
        }
    }
    for (field, _) in HASH_MAP_FIELDS {
        assert!(longest(&cbor(&bytes), field) > 1, "one {field} entry");
    }

    let expected = canonical(&bytes);
    let mut encodings = BTreeSet::new();
    for _ in 0..32 {
        let encoded = value_to_vec(&value_from_slice::<StagedCommit>(&bytes)?)?;
        assert_eq!(
            canonical(&encoded),
            expected,
            "an encoding differs after ordering"
        );
        encodings.insert(encoded);
    }
    assert!(encodings.len() > 1, "every encoding had the same order");
}

/// Every map key path in a stored value, with array items merged, every
/// text value with its path (a unit variant is stored as its name), and the
/// CBOR type of the item at each path. A list row is an array of byte
/// strings, each an encoded value: its items' paths count.
fn key_paths(row: &Row, paths: &mut BTreeSet<String>) {
    fn walk(value: &Cbor, prefix: &str, paths: &mut BTreeSet<String>) {
        let kind = match value {
            Cbor::Null => "null",
            Cbor::Bool(_) => "bool",
            Cbor::Integer(_) => "integer",
            Cbor::Float(_) => "float",
            Cbor::Bytes(_) => "bytes",
            Cbor::Text(_) => "text",
            Cbor::Array(_) => "array",
            Cbor::Map(_) => "map",
            Cbor::Tag(..) => "tag",
            _ => "other",
        };
        paths.insert(format!("{prefix}:{kind}"));
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
    match list_items(&row.value) {
        Some(items) => {
            for item in items {
                walk(&cbor(&item), "[]", paths);
            }
        }
        None => walk(&cbor(&row.value), "", paths),
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

/// The label parser reads every one-line byte string constant, whatever its
/// visibility, keyword, type length or trailing comment.
#[xmtp_common::test(unwrap_try = true)]
fn byte_string_constants_parse_in_any_form() {
    for (line, label) in [
        ("const A: &[u8] = b\"A\";", Some("A")),
        ("    pub const B: &[u8] = b\"B\";", Some("B")),
        (
            "pub(crate) const C: &[u8; 1] = b\"C\"; // a comment",
            Some("C"),
        ),
        (
            "pub(in crate::sql_key_store) static D: &[u8] = b\"D\";",
            Some("D"),
        ),
        ("const E: &str = \"E\";", None),
        ("let f: &[u8] = b\"f\";", None),
        ("pub fn g() -> &'static [u8] { b\"g\" }", None),
    ] {
        assert_eq!(byte_string_constant(line), label, "{line}");
    }
}

/// Every label the key store writes has a row in the newest frozen file, and
/// each row's label matches its key.
#[xmtp_common::test(unwrap_try = true)]
fn golden_rows_cover_every_label() {
    let labels = store_labels();
    assert_eq!(labels.len(), 21, "the store's labels: {labels:?}");
    let golden = newest();
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

/// Shapes XMTP groups store, as (label or column, exact key path): Unknown
/// and ApplicationId extensions and the AppData dictionary that carries the
/// last-resort mark in key packages; the AppData dictionary and
/// RequiredCapabilities in group contexts; decryption
/// ratchets with skipped-generation secrets and past epoch trees; inactive,
/// operational and pending-commit group states; standalone Update,
/// AppDataUpdate and Remove proposals; and an own staged commit with an Add
/// by reference, an inline AppDataUpdate and Remove, new key pairs, and the
/// committer's new leaf.
const PRODUCTION_SHAPES: &[(&str, &str)] = &[
    (
        "KeyPackage",
        ".key_package.payload.extensions.unique[].Unknown",
    ),
    (
        "KeyPackage",
        ".key_package.payload.extensions.unique[].AppDataDictionary",
    ),
    (
        "KeyPackage",
        ".key_package.payload.leaf_node.payload.extensions.unique[].ApplicationId",
    ),
    ("GroupContext", ".extensions.unique[].AppDataDictionary"),
    ("GroupContext", ".extensions.unique[].RequiredCapabilities"),
    (
        "MessageSecrets",
        ".message_secrets.secret_tree.application_sender_ratchets[].DecryptionRatchet.past_secrets[][].aead_mode",
    ),
    (
        "MessageSecrets",
        ".message_secrets.secret_tree.application_sender_ratchets[].EncryptionRatchet.generation",
    ),
    (
        "MessageSecrets",
        ".past_epoch_trees[].message_secrets.secret_tree.application_sender_ratchets[].DecryptionRatchet.ratchet_head.generation",
    ),
    ("GroupState", "=Inactive"),
    // libxmtp's own plain values are byte strings, not arrays of integers.
    ("KeyPackageWrapperPrivateKey", ":bytes"),
    ("CommitLogSignerPrivateKey", ":bytes"),
    ("GroupState", "=Operational"),
    (
        "GroupState",
        ".PendingCommit.Member.state.GroupMember.new_keypairs[].public_key",
    ),
    (
        "GroupState",
        ".PendingCommit.Member.state.GroupMember.update_path_leaf_node.payload",
    ),
    ("QueuedProposal", ".proposal.Update"),
    ("QueuedProposal", ".proposal.AppDataUpdate.operation.Update"),
    ("QueuedProposal", ".proposal.Remove.removed"),
    ("QueuedProposal", ".proposal_or_ref_type=Reference"),
    (
        "StagedCommit",
        ".staged_proposal_queue.queued_proposals[][].proposal.Add",
    ),
    (
        "StagedCommit",
        ".staged_proposal_queue.queued_proposals[][].proposal.AppDataUpdate.operation.Update",
    ),
    (
        "StagedCommit",
        ".staged_proposal_queue.queued_proposals[][].proposal.Remove.removed",
    ),
    (
        "StagedCommit",
        ".staged_proposal_queue.queued_proposals[][].proposal_or_ref_type=Proposal",
    ),
    (
        "StagedCommit",
        ".staged_proposal_queue.queued_proposals[][].proposal_or_ref_type=Reference",
    ),
    (
        "StagedCommit",
        ".state.GroupMember.new_keypairs[].public_key",
    ),
    (
        "StagedCommit",
        ".state.GroupMember.update_path_leaf_node.payload",
    ),
];

/// Every integer stored under the map key `field`, at any depth.
fn integers(value: &Cbor, field: &str, found: &mut Vec<i128>) {
    match value {
        Cbor::Map(entries) => {
            for (key, value) in entries {
                if let (Cbor::Text(name), Cbor::Integer(integer)) = (key, value)
                    && name == field
                {
                    found.push(*integer);
                }
                integers(value, field, found);
            }
        }
        Cbor::Array(items) => items.iter().for_each(|item| integers(item, field, found)),
        Cbor::Tag(_, inner) => integers(inner, field, found),
        _ => {}
    }
}

/// The newest frozen file holds the shapes XMTP groups store
/// ([`PRODUCTION_SHAPES`]), message secrets that keep XMTP's past epochs, and
/// an epoch and a generation above 255, so a stored integer narrowed to `u8`
/// fails to load.
#[xmtp_common::test(unwrap_try = true)]
fn golden_rows_hold_production_shapes() {
    let golden = newest();
    let shapes = shapes(&golden);
    for (label, path) in PRODUCTION_SHAPES {
        let found = shapes
            .iter()
            .any(|(key, paths)| (key.ends_with(&format!(" {label}"))) && paths.contains(*path));
        assert!(found, "{} has no {label} {path}", golden.name);
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

    for (label, field) in [("GroupContext", "epoch"), ("MessageSecrets", "generation")] {
        let mut found = vec![];
        for row in golden.rows().filter(|row| row.label == label) {
            integers(&cbor(&row.value), field, &mut found);
        }
        assert!(
            found.iter().any(|integer| *integer > 255),
            "{}: no {label} {field} above 255: {found:?}",
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
    let newest = newest();
    let fresh = golden_from_json("this build", build().await);
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

/// Shapes of an older frozen file that a newer format removed on purpose, as
/// (file, label or column key, key path), each with the reason in a comment.
/// Every other shape of an older file must stay in the newest file.
const REMOVED_SHAPES: &[(&str, &str, &str)] = &[];

type Shapes = BTreeMap<String, BTreeSet<String>>;

/// The shapes of the frozen file `file` (`older`) that `newest` lacks and
/// [`REMOVED_SHAPES`] does not list, as (label or column key, shape).
fn shapes_missing(file: &str, older: &Shapes, newest: &Shapes) -> Vec<(String, String)> {
    let mut missing = vec![];
    for (key, paths) in older {
        for path in paths {
            let kept = newest.get(key).is_some_and(|newer| newer.contains(path));
            let removed = REMOVED_SHAPES.contains(&(file, key.as_str(), path.as_str()));
            if !kept && !removed {
                missing.push((key.clone(), path.clone()));
            }
        }
    }
    missing
}

/// Every shape of an older frozen file is also in the newest one, so a
/// renamed or removed field or variant in a crate-private type, which rows of
/// the older file still hold, fails here even when it reads them with a
/// default. Removals on purpose go in [`REMOVED_SHAPES`].
///
/// Until a second file exists the loop below checks nothing, so the newest
/// file checks the comparison itself: it keeps all its own shapes, and a copy
/// without one shape lacks exactly that one.
#[xmtp_common::test(unwrap_try = true)]
fn older_golden_files_keep_their_shapes() {
    let newest_file = GOLDEN_FILES.last().unwrap().name;
    let newest = shapes(&newest());
    assert!(shapes_missing(newest_file, &newest, &newest).is_empty());
    let mut cut = newest.clone();
    assert!(cut.values_mut().any(|paths| paths.remove("=Inactive")));
    let missing = shapes_missing(newest_file, &newest, &cut);
    assert_eq!(
        missing
            .iter()
            .map(|(_, path)| path.as_str())
            .collect::<Vec<_>>(),
        ["=Inactive"]
    );

    for file in &GOLDEN_FILES[..GOLDEN_FILES.len() - 1] {
        let missing = shapes_missing(file.name, &shapes(&parse(file)), &newest);
        assert!(
            missing.is_empty(),
            "{} shapes are not in {newest_file}: {missing:?}",
            file.name
        );
    }
}

/// Rows and columns of the newest frozen file with a public type decode with
/// it and encode back to the same CBOR. An older file may hold a value this
/// build writes with a new defaulted field; it must still load and run.
#[xmtp_common::test(unwrap_try = true)]
fn golden_rows_round_trip_their_public_types() {
    let golden = newest();
    let mut checked = BTreeSet::new();
    for row in golden.rows() {
        let what = format!("{} {}", golden.name, row.label);
        match row.label.as_str() {
            "KeyPackage" => round_trips::<KeyPackageBundle>(&what, &row.value),
            "SignatureKeyPair" => round_trips::<SignatureKeyPair>(&what, &row.value),
            "GroupContext" => round_trips::<openmls::group::GroupContext>(&what, &row.value),
            "KeyPackageReferences" => round_trips::<KeyPackageRef>(&what, &row.value),
            "KeyPackageWrapperPrivateKey" | "CommitLogSignerPrivateKey" => {
                round_trips::<Secret>(&what, &row.value)
            }
            "OwnLeafNodeIndex" => round_trips::<openmls::prelude::LeafNodeIndex>(&what, &row.value),
            "MlsGroupJoinConfig" => round_trips::<MlsGroupJoinConfig>(&what, &row.value),
            "QueuedProposal" => round_trips::<openmls::prelude::QueuedProposal>(&what, &row.value),
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

/// A declared array length far past the end of a row is an error, not a
/// panic, also when the failure is logged. OpenMLS reads an export tree's
/// nodes into a HashMap sized from the declared length, so serde must never
/// read bytes that fail the codec's pre-scan: not for the value, and not for
/// the logged field path.
#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
async fn huge_declared_lengths_are_errors() {
    const NODES: &[u8] = b"\x65nodes";
    const HUGE: [u8; 9] = [0x9b, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff];
    let huge_nodes = |value: &[u8]| -> Vec<u8> {
        let at = value
            .windows(NODES.len())
            .position(|window| window == NODES)
            .expect("a nodes array")
            + NODES.len();
        assert_eq!(value[at], 0x81, "one export tree node");
        [&value[..at], &HUGE, &value[at + 1..]].concat()
    };
    let golden = newest();
    let rows: Vec<Row> = golden.stores["alice"]
        .iter()
        .map(|row| Row {
            label: row.label.clone(),
            key: row.key.clone(),
            value: match row.label.as_str() {
                "ApplicationExportTree" => huge_nodes(&row.value),
                _ => row.value.clone(),
            },
        })
        .collect();
    let alice = provider().await;
    restore(&alice, &rows);
    for name in ["first", "second"] {
        let (loaded, spans) = xmtp_logging::test_logging::with_trace_layer(true, || {
            tracing::error_span!("load")
                .in_scope(|| MlsGroup::load(alice.storage(), &golden.group(name)).map(drop))
        });
        assert!(loaded.is_err(), "{name} loaded");
        let logged = format!("{spans:?}");
        assert!(logged.contains("ApplicationExportTree"), "{logged}");
    }
    let staged = huge_nodes(&golden.columns["StagedCommit"]);
    assert!(value_from_slice::<StagedCommit>(&staged).is_err());
}

/// OpenMLS reads the crate-private rows itself. Restored into new stores:
/// - the key package and signature key rows read back through their keys,
///   and every encryption key pair reads back;
/// - Bob decrypts the held-back message from the past epoch trees, and the
///   skipped message from his ratchet's skipped-generation secrets;
/// - Bob commits Alice's stored proposals: her self-update, which Alice can
///   only process with her stored own leaf node and its key pair, the
///   AppDataUpdate, and the Remove;
/// - Alice and Bob exchange messages;
/// - Carol's group loads inactive;
/// - Alice's second group loads with its staged commit, and the staged
///   commit column merges into it with its dictionary update.
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
                        let bundle = bundle
                            .unwrap_or_else(|| panic!("{store}'s key package does not read back"));
                        // Still last resort, by the component with empty data: a
                        // build that stopped recognizing the stored mark would
                        // delete the published package after its first Welcome.
                        let package = bundle.key_package();
                        assert!(package.last_resort(), "{store}'s key package");
                        assert_eq!(
                            package
                                .extensions()
                                .app_data_dictionary()
                                .and_then(|extension| extension
                                    .dictionary()
                                    .get(&LAST_RESORT_COMPONENT)),
                            Some(&[][..]),
                            "{store}'s key package"
                        );
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

        // What the file's steps leave, recorded when it was written.
        let expect = |name: &str| golden.json["expect"][name].as_u64().unwrap() as usize;
        let expect_bytes =
            |name: &str| hex::decode(golden.json["expect"][name].as_str().unwrap()).unwrap();
        let message = |name: &str| hex::decode(golden.json[name].as_str().unwrap()).unwrap();

        let first = golden.group("first");
        let mut alice_group = MlsGroup::load(alice.storage(), &first)?.unwrap();
        let mut bob_group = MlsGroup::load(bob.storage(), &first)?.unwrap();
        assert_eq!(alice_group.members().count(), expect("first_members"));
        for group in [&alice_group, &bob_group] {
            assert_eq!(
                group.pending_proposals().count(),
                expect("first_pending_proposals")
            );
        }

        assert_eq!(
            read_message(bob, &mut bob_group, &message("late_message")),
            LATE_MESSAGE
        );
        let skipped = read_message(bob, &mut bob_group, &message("skipped_message"));
        assert!(skipped.starts_with(b"2 from alice"));

        // Bob commits Alice's stored proposals: her self-update, which Alice
        // can only process with her own leaf node, the AppDataUpdate and the
        // Remove.
        let commit = commit_pending(bob, &mut bob_group, &signers["bob"], vec![], vec![], false);
        bob_group.merge_pending_commit(bob)?;
        merge_commit(alice, &mut alice_group, &commit);
        assert_eq!(alice_group.epoch(), bob_group.epoch());
        for group in [&alice_group, &bob_group] {
            assert_eq!(
                group.members().count(),
                expect("first_members_after_commit")
            );
            assert_eq!(
                membership(group),
                Some(&*expect_bytes("first_membership_after_commit"))
            );
        }
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
        assert_eq!(
            second.members().count(),
            expect("second_members_after_merge")
        );
        assert_eq!(
            membership(&second),
            Some(&*expect_bytes("second_membership_after_merge"))
        );

        // The XMTP labels read back through their production keys and
        // readers, as `find_key_package_hash_ref`, `find_private_key` and
        // older clients' commit-log signer reader read them. `build` wrote
        // these values.
        let references = golden.stores["bob"]
            .iter()
            .find(|row| row.label == "KeyPackageReferences")
            .expect("bob's key package reference");
        let init_key = &references.key[KEY_PACKAGE_REFERENCES.len()..references.key.len() - 2];
        let hash_ref: KeyPackageRef =
            XmtpMlsStorageProvider::read(bob.storage(), KEY_PACKAGE_REFERENCES, init_key)?
                .expect("the hash ref reads back through the init key");
        let wrapper = XmtpMlsStorageProvider::wrapper_private_key(
            bob.storage(),
            &encode_key(KEY_PACKAGE_WRAPPER_PRIVATE_KEY, &hash_ref)?,
        )?
        .expect("the wrapper key reads back through its hash ref");
        assert_eq!(wrapper.as_slice(), [0x5a; 32]);
        let signer = XmtpMlsStorageProvider::commit_log_signer(bob.storage(), first.as_slice())?
            .expect("the commit-log signer reads back through the group id");
        assert_eq!(signer.as_slice(), [0x24; 32]);
        db_deserialize::<Credential>(&golden.columns["Credential"])?;
        db_deserialize::<XmtpInstallationCredential>(
            &golden.columns["XmtpInstallationCredential"],
        )?;
    }
}
