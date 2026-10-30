use crate::groups::MlsGroup;
use crate::groups::PolicySet;
use crate::groups::commit_log::{
    CommitLogTestFunction, CommitLogWorker, Factory, should_skip_remote_commit_log_entry,
};
use crate::groups::send_message_opts::SendMessageOpts;
use crate::{context::XmtpSharedContext, tester};
use openmls::prelude::{OpenMlsCrypto, SignatureScheme};
use openmls_rust_crypto::RustCrypto;
use prost::Message;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use xmtp_common::time::Duration;
use xmtp_db::consent_record::{ConsentState, ConsentType, StoredConsentRecord};
use xmtp_db::group::GroupQueryArgs;
use xmtp_db::group::{ConversationType, GroupMembershipState};
use xmtp_db::group_message::MsgQueryArgs;
use xmtp_db::local_commit_log::{CommitType, LocalCommitLog, NewLocalCommitLog};
use xmtp_db::prelude::*;
use xmtp_db::refresh_state::EntityKind;
use xmtp_db::remote_commit_log::{CommitResult, RemoteCommitLog, RemoteCommitLogOrder};
use xmtp_mls_common::app_data::group_root::{group_keys_from_extensions, read_group_root};
use xmtp_mls_common::commit_log::{open_entry, seal_entry, sign_entry_author};
use xmtp_mls_common::group::GroupMetadataOptions;
use xmtp_mls_common::group_keys::{GroupKeys, GroupRoot, SealError};
use xmtp_proto::backend_v1::{self as wire, SealedCommitLogEntry, client_envelope::Payload};
use xmtp_proto::types::Cursor;
use xmtp_proto::types::GroupId;
use xmtp_proto::types::Topic;
use xmtp_proto::xmtp::mls::message_contents::CommitResult as ProtoCommitResult;
use xmtp_proto::xmtp::mls::message_contents::PlaintextCommitLogEntry;

mod scripted;

// Helper functions for tracking commit types
fn get_commit_types_as_strings(logs: &[LocalCommitLog]) -> Vec<String> {
    logs.iter()
        .map(|l| l.commit_type.clone().unwrap_or_else(|| "None".to_string()))
        .collect()
}

async fn print_commit_log_with_types(group: &MlsGroup<impl XmtpSharedContext>) {
    let logs = group.local_commit_log().await.unwrap();
    println!(
        "Commit log for group {}: {:?}",
        hex::encode(&group.group_id.as_slice()[0..4]),
        get_commit_types_as_strings(&logs)
    );
}

fn assert_commit_sequence(logs: &[LocalCommitLog], expected: &[CommitType]) {
    let actual_types = get_commit_types_as_strings(logs);
    let expected_types: Vec<String> = expected.iter().map(|t| t.to_string()).collect();

    assert_eq!(
        actual_types, expected_types,
        "Commit sequence mismatch. Expected: {:?}, Got: {:?}",
        expected_types, actual_types
    );
}

/// The keys of a conversation's group root, as its commit-log worker derives
/// them.
fn keys_of(group: &MlsGroup<impl XmtpSharedContext>) -> GroupKeys {
    group_keys_from_extensions(group.load_group_context().unwrap().extensions()).unwrap()
}

fn root_of(group: &MlsGroup<impl XmtpSharedContext>) -> GroupRoot {
    read_group_root(group.load_group_context().unwrap().extensions())
        .unwrap()
        .unwrap()
}

/// The commit-log signing seed, derived from the root without `GroupKeys`, so
/// a test can sign an entry with any field lengths.
fn commit_log_seed(root: &GroupRoot) -> [u8; 32] {
    let hkdf = hkdf::Hkdf::<sha2::Sha256>::new(Some(b"xmtp group root"), root.as_bytes());
    let mut seed = [0; 32];
    hkdf.expand(b"xmtp commit log signing key", &mut seed)
        .unwrap();
    seed
}

/// A sealed entry with these fields, signed by `seed` over the
/// `CommitLogEntryTBS` of exactly these fields.
fn signed_entry(
    seed: &[u8; 32],
    commit_log_id: &[u8],
    nonce: Vec<u8>,
    ciphertext: Vec<u8>,
) -> SealedCommitLogEntry {
    use tls_codec::Serialize;
    let topic = Topic::new_commit_log(commit_log_id).cloned_vec();
    let mut tbs = Vec::new();
    for field in [
        b"xmtp commit log entry".as_slice(),
        &topic,
        &nonce,
        &ciphertext,
    ] {
        tls_codec::VLByteSlice(field)
            .tls_serialize(&mut tbs)
            .unwrap();
    }
    let signature = RustCrypto::default()
        .sign(SignatureScheme::ED25519, &tbs, seed)
        .unwrap();
    SealedCommitLogEntry {
        commit_log_id: commit_log_id.to_vec(),
        nonce,
        ciphertext,
        signature,
    }
}

fn server_envelope(topic: &Topic, sequence_id: u64, payload: Payload) -> wire::ServerEnvelope {
    wire::ServerEnvelope {
        meta: Some(wire::EnvelopeMeta {
            topic: Some(wire::Topic {
                topic: topic.cloned_vec(),
            }),
            cursor: Some(wire::Cursor { sequence_id }),
            server_ns: 1,
            message_hash: Some(wire::MessageHash {
                hash: Some(wire::message_hash::Hash::Sha256(vec![
                    sequence_id as u8;
                    32
                ])),
            }),
            ..Default::default()
        }),
        envelope: Some(wire::ClientEnvelope {
            payload: Some(payload),
        }),
    }
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

/// A conversation's local entries that have a commit sequence id: the ones
/// its worker publishes.
async fn publishable(group: &MlsGroup<impl XmtpSharedContext>) -> Vec<PlaintextCommitLogEntry> {
    group
        .local_commit_log()
        .await
        .unwrap()
        .iter()
        .filter(|log| log.commit_sequence_id > 0)
        .map(|log| {
            let mut entry = PlaintextCommitLogEntry::from(log);
            sign_entry_author(
                &keys_of(group),
                &group.context.identity().installation_keys,
                &mut entry,
            )
            .unwrap();
            entry
        })
        .collect()
}

/// Read a conversation's commit-log topic from the backend and open every
/// entry with its keys.
async fn remote_entries(
    client: &crate::utils::FullXmtpClient,
    group: &MlsGroup<impl XmtpSharedContext>,
) -> Vec<PlaintextCommitLogEntry> {
    let keys = keys_of(group);
    client
        .context
        .api()
        .query_commit_log([(Topic::new_commit_log(keys.commit_log_id()), Cursor(0))].into())
        .await
        .unwrap()
        .iter()
        .map(|read| {
            open_entry(
                &keys,
                group.group_id.as_slice(),
                read.cursor.0,
                read.sealed.as_ref().unwrap(),
            )
            .unwrap()
        })
        .collect()
}

fn upload_cursor(client: &crate::utils::FullXmtpClient, group_id: &GroupId) -> Cursor {
    client
        .context
        .db()
        .get_last_cursor(group_id, EntityKind::CommitLogUpload)
        .unwrap()
}

/// The conversations one publish turn advanced.
async fn publish_turn<C: XmtpSharedContext + 'static>(
    worker: &mut CommitLogWorker<C>,
) -> Vec<Vec<u8>> {
    worker
        .run_test(CommitLogTestFunction::PublishCommitLogsToRemote, None)
        .await
        .unwrap()[0]
        .publish_commit_log_results
        .as_ref()
        .unwrap()
        .iter()
        .map(|published| published.conversation_id.clone())
        .collect()
}

/// Every conversation stores its commit-log id at creation and at join. It
/// is the public key of the commit-log signing key the group root derives,
/// so every member stores the same one, and it is not the group id.
// verifies: GMOD-041
#[xmtp_common::test(unwrap_try = true)]
async fn the_commit_log_id_is_stored_at_creation_and_join() {
    tester!(alix, with_commit_log_worker: false);
    tester!(bo, with_commit_log_worker: false);

    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let dm = alix.find_or_create_dm(bo.inbox_id(), None).await?;
    bo.sync_welcomes().await?;
    for created in [&group, &dm] {
        let joined = bo.group(&created.group_id)?;
        let commit_log_id = keys_of(created).commit_log_id().to_vec();
        assert_ne!(commit_log_id, created.group_id.to_vec());
        assert_eq!(keys_of(&joined).commit_log_id().to_vec(), commit_log_id);
        let created_row = alix.context.db().find_group(&created.group_id)??;
        assert_eq!(
            created_row.commit_log_public_key,
            Some(commit_log_id.clone()),
            "stored at creation"
        );
        let joined_row = bo.context.db().find_group(&created.group_id)??;
        assert_eq!(
            joined_row.commit_log_public_key,
            Some(commit_log_id),
            "stored at join"
        );
    }
}

/// A restored placeholder holds no root and no commit-log id. The Welcome
/// that activates it stores the commit-log id its root derives.
#[xmtp_common::test(unwrap_try = true)]
async fn activating_a_placeholder_stores_its_commit_log_id() {
    tester!(alix, with_commit_log_worker: false);
    tester!(bo, with_commit_log_worker: false);

    let a = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    // Bo holds the group only as an archive placeholder.
    MlsGroup::insert(
        &bo.context,
        Some(a.group_id.as_slice()),
        GroupMembershipState::Restored,
        ConversationType::Group,
        PolicySet::default(),
        GroupMetadataOptions::default(),
        None,
        false,
    )?;
    let placeholder = bo.context.db().find_group(&a.group_id)??;
    assert_eq!(placeholder.commit_log_public_key, None);

    bo.sync_welcomes().await?;
    let activated = bo.context.db().find_group(&a.group_id)??;
    assert_ne!(activated.membership_state, GroupMembershipState::Restored);
    assert_eq!(
        activated.commit_log_public_key,
        Some(keys_of(&a).commit_log_id().to_vec())
    );
}

/// The worker publishes each local entry as a sealed entry on the topic its
/// commit-log id names, not its group id. The envelope holds neither the
/// group id nor the entry's authenticators, the entry opens to the local
/// one, and sealing it again gives the same bytes, which the backend stores
/// once.
// verifies: FORK-074
#[xmtp_common::test(unwrap_try = true)]
async fn test_publish_commit_log_to_remote() {
    // Disable background CommitLogWorker for deterministic testing
    tester!(alix, with_commit_log_worker: false);
    tester!(bo);

    // Alix creates a group with Bo
    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members(&[bo.inbox_id()]).await?;
    bo.sync_all_welcomes_and_groups(None).await?;

    // Alix has two local commit log entries; only the one with a commit
    // sequence id is published.
    let commit_log_entries = alix.context.db().get_group_logs(&alix_group.group_id)?;
    assert_eq!(commit_log_entries.len(), 2);
    assert_eq!(upload_cursor(&alix, &alix_group.group_id), Cursor(0));

    let mut commit_log_worker = CommitLogWorker::new(alix.context.clone());
    commit_log_worker
        .run_test(CommitLogTestFunction::PublishCommitLogsToRemote, Some(1))
        .await?;
    let last_commit_log_entry = commit_log_entries.last()?;
    assert_eq!(
        upload_cursor(&alix, &alix_group.group_id),
        Cursor(last_commit_log_entry.rowid as u64)
    );

    let keys = keys_of(&alix_group);
    let topic = Topic::new_commit_log(keys.commit_log_id());
    assert_ne!(topic.identifier(), alix_group.group_id.as_slice());
    let response = alix
        .context
        .api()
        .query_commit_log([(topic.clone(), Cursor(0))].into())
        .await?;
    assert_eq!(response.len(), 1);
    let sealed = response[0].sealed.clone()?;
    let entry = open_entry(
        &keys,
        alix_group.group_id.as_slice(),
        response[0].cursor.0,
        &sealed,
    )?;
    let mut expected = PlaintextCommitLogEntry::from(last_commit_log_entry);
    sign_entry_author(
        &keys,
        &alix.context.identity().installation_keys,
        &mut expected,
    )?;
    assert_eq!(entry, expected);

    let envelope = wire::ClientEnvelope {
        payload: Some(Payload::SealedCommitLogEntry(sealed.clone())),
    }
    .encode_to_vec();
    assert!(!contains(&envelope, alix_group.group_id.as_slice()));
    assert!(!contains(&envelope, &entry.last_epoch_authenticator));
    assert!(!contains(&envelope, &entry.applied_epoch_authenticator));

    // Sealing the entry again gives the same bytes, so a retry is a
    // duplicate: the backend returns the stored envelope's position.
    assert_eq!(seal_entry(&keys, &entry)?, sealed);
    let mut stored = Vec::new();
    alix.context
        .api()
        .publish_commit_log(vec![sealed], &mut stored)
        .await?;
    assert_eq!(stored[0].cursor.as_ref()?.sequence_id, response[0].cursor.0);
    let again = alix
        .context
        .api()
        .query_commit_log([(topic, Cursor(0))].into())
        .await?;
    assert_eq!(again.len(), 1);
}

/// A rate-limited publish is not retried at the next turn inside its delay.
// verifies: RATE-041
#[xmtp_common::test(unwrap_try = true)]
async fn a_rate_limited_publish_waits_out_its_delay() {
    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    group.update_group_name("limited".into()).await?;
    let (client, script) = scripted::scripted(&alix.client).await;
    script.lock().rate_limit = Some(xmtp_common::time::Duration::from_secs(300));
    let id = keys_of(&group).commit_log_id().to_vec();
    let mut worker = CommitLogWorker::new(client.context.clone());
    assert!(publish_turn(&mut worker).await.is_empty());
    assert_eq!(script.lock().publishes_for(&id), 1);
    assert!(publish_turn(&mut worker).await.is_empty());
    assert_eq!(
        script.lock().publishes_for(&id),
        1,
        "no publish inside the delay"
    );
}

/// A storage reconnect builds a new worker from the same factory, which keeps
/// the publish hold: the new worker does not publish inside the delay.
// verifies: RATE-041
#[xmtp_common::test(unwrap_try = true)]
async fn a_publish_hold_survives_a_new_worker() {
    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    group.update_group_name("limited".into()).await?;
    let (client, script) = scripted::scripted(&alix.client).await;
    script.lock().rate_limit = Some(Duration::from_secs(300));
    let id = keys_of(&group).commit_log_id().to_vec();
    let factory = Factory::new(client.context.clone());
    assert!(publish_turn(&mut factory.worker()).await.is_empty());
    assert_eq!(script.lock().publishes_for(&id), 1);
    assert!(publish_turn(&mut factory.worker()).await.is_empty());
    assert_eq!(
        script.lock().publishes_for(&id),
        1,
        "no publish inside the delay"
    );
}

/// A readd answer refused with a rate limit keeps the request and is not
/// answered again inside the delay, also by a worker a storage reconnect
/// builds from the same factory.
// verifies: RATE-041
#[xmtp_common::test(unwrap_try = true)]
async fn a_rate_limited_readd_waits_out_its_delay() {
    tester!(alix, disable_workers, enable_fork_recovery_requests);
    tester!(bo, disable_workers);
    let dm = alix
        .find_or_create_dm(bo.inbox_id().to_string(), None)
        .await?;
    bo.sync_all_welcomes_and_groups(None).await?;
    let bo_dm = bo.group(&dm.group_id)?;
    bo_dm.update_consent_state(ConsentState::Allowed)?;
    dm.key_update().await?;
    bo_dm.sync().await?;
    let mut alix_worker = CommitLogWorker::new(alix.context.clone());
    let (bo_client, script) = scripted::scripted(&bo.client).await;
    let factory = Factory::new(bo_client.context.clone());
    let mut bo_worker = factory.worker();
    alix_worker._tick().await?;
    bo_worker._tick().await?;
    alix.context
        .db()
        .set_group_commit_log_forked_status(&dm.group_id, Some(true))?;
    alix_worker._tick().await?;
    bo.sync_welcomes().await?;
    let installation = alix.context.installation_id();
    let bo_db = bo.context.db();
    assert!(bo_db.is_awaiting_readd(&dm.group_id, installation.as_slice())?);

    script.lock().group_message_rate_limit = Some(Duration::from_secs(300));
    let epoch = bo_dm.epoch().await?;
    bo_worker._tick().await?;
    let attempts = script.lock().group_message_publishes;
    assert!(attempts > 0, "the readd is sent");
    assert_eq!(bo_dm.epoch().await?, epoch);
    assert!(bo_db.is_awaiting_readd(&dm.group_id, installation.as_slice())?);
    bo_worker._tick().await?;
    factory.worker()._tick().await?;
    assert_eq!(
        script.lock().group_message_publishes,
        attempts,
        "no readd inside the delay"
    );
    assert!(bo_db.is_awaiting_readd(&dm.group_id, installation.as_slice())?);
}

/// A rate-limited conversation holds only its own commit-log publish: another
/// conversation still publishes inside the delay.
// verifies: RATE-041
#[xmtp_common::test(unwrap_try = true)]
async fn a_rate_limited_publish_holds_only_its_conversation() {
    tester!(alix, disable_workers);
    let first = alix.create_group(None, None)?;
    first.update_group_name("one".into()).await?;
    let second = alix.create_group(None, None)?;
    second.update_group_name("two".into()).await?;
    let ids = [
        keys_of(&first).commit_log_id().to_vec(),
        keys_of(&second).commit_log_id().to_vec(),
    ];
    let (client, script) = scripted::scripted(&alix.client).await;
    script.lock().rate_limit = Some(xmtp_common::time::Duration::from_secs(300));
    let mut worker = CommitLogWorker::new(client.context.clone());
    assert!(publish_turn(&mut worker).await.is_empty());
    let refused = ids
        .iter()
        .find(|id| script.lock().publishes_for(id) == 1)?
        .clone();
    let other = ids.iter().find(|id| **id != refused)?.clone();
    script.lock().rate_limit = None;
    publish_turn(&mut worker).await;
    assert_eq!(
        script.lock().publishes_for(&refused),
        1,
        "no publish of the refused conversation inside its delay"
    );
    assert!(
        script.lock().publishes_for(&other) >= 1,
        "the other conversation still publishes"
    );
}

/// A rate limit on one conversation's publish does not end the turn: the
/// next conversation still publishes in the same turn.
// verifies: RATE-041
#[xmtp_common::test(unwrap_try = true)]
async fn a_rate_limited_publish_does_not_end_the_turn() {
    tester!(alix, disable_workers);
    let first = alix.create_group(None, None)?;
    first.update_group_name("one".into()).await?;
    let second = alix.create_group(None, None)?;
    second.update_group_name("two".into()).await?;
    let ids = [
        keys_of(&first).commit_log_id().to_vec(),
        keys_of(&second).commit_log_id().to_vec(),
    ];
    let (client, script) = scripted::scripted(&alix.client).await;
    {
        let mut script = script.lock();
        script.rate_limit = Some(xmtp_common::time::Duration::from_secs(300));
        script.rate_limit_once = true;
    }
    let mut worker = CommitLogWorker::new(client.context.clone());
    let published = publish_turn(&mut worker).await;
    for id in &ids {
        assert_eq!(
            script.lock().publishes_for(id),
            1,
            "each conversation is sent once"
        );
    }
    assert_eq!(
        published.len(),
        1,
        "the conversation after the refused one publishes"
    );
}

/// Publication resumes once a rate-limit hold has ended.
// verifies: RATE-041
#[xmtp_common::test(unwrap_try = true)]
async fn a_commit_log_publish_resumes_after_its_hold() {
    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    group.update_group_name("limited".into()).await?;
    let (client, script) = scripted::scripted(&alix.client).await;
    script.lock().rate_limit = Some(xmtp_common::time::Duration::from_millis(50));
    let mut worker = CommitLogWorker::new(client.context.clone());
    assert!(publish_turn(&mut worker).await.is_empty());
    script.lock().rate_limit = None;
    xmtp_common::time::sleep(xmtp_common::time::Duration::from_millis(60)).await;
    assert_eq!(
        publish_turn(&mut worker).await,
        vec![group.group_id.to_vec()]
    );
}

/// The backend refuses one conversation's entries. Another conversation's
/// entries still publish in that turn and in the next, the refused
/// conversation keeps its entries pending, and once it is accepted it
/// publishes them from its first entry.
// verifies: FORK-021
#[xmtp_common::test(unwrap_try = true)]
async fn a_refused_conversation_does_not_hold_back_another() {
    tester!(alix, disable_workers);
    let refused = alix.create_group(None, None)?;
    let accepted = alix.create_group(None, None)?;
    refused.update_group_name("refused".into()).await?;
    accepted.update_group_name("accepted".into()).await?;

    let (client, script) = scripted::scripted(&alix.client).await;
    let refused_id = keys_of(&refused).commit_log_id().to_vec();
    script.lock().refuse.insert(refused_id.clone());
    let mut worker = CommitLogWorker::new(client.context.clone());

    for turn in 1..=2 {
        if turn == 2 {
            accepted.update_group_name("later".into()).await?;
        }
        assert_eq!(
            publish_turn(&mut worker).await,
            vec![accepted.group_id.to_vec()]
        );
        assert_eq!(worker.failed_publishes(), &[refused.group_id]);
        assert_eq!(
            upload_cursor(&alix, &refused.group_id),
            Cursor(0),
            "the refused entries stay pending"
        );
        let tip = accepted.local_commit_log().await?.last()?.rowid;
        assert_eq!(upload_cursor(&alix, &accepted.group_id), Cursor(tip as u64));
        assert_eq!(script.lock().publishes_for(&refused_id), turn);
    }
    assert_eq!(
        remote_entries(&alix, &accepted).await,
        publishable(&accepted).await
    );

    // Once the backend accepts it, the held conversation publishes from its
    // first entry.
    script.lock().refuse.clear();
    assert_eq!(
        publish_turn(&mut worker).await,
        vec![refused.group_id.to_vec()]
    );
    assert!(worker.failed_publishes().is_empty());
    let tip = refused.local_commit_log().await?.last()?.rowid;
    assert_eq!(upload_cursor(&alix, &refused.group_id), Cursor(tip as u64));
    assert_eq!(
        remote_entries(&alix, &refused).await,
        publishable(&refused).await
    );
}

/// One conversation's entry fails the local admission parse: its nonce is a
/// byte short. Nothing of that conversation is sent, and the other
/// conversation still publishes in the same turn and in the next one.
// verifies: FORK-021
#[xmtp_common::test(unwrap_try = true)]
async fn a_conversation_that_fails_the_local_parse_does_not_hold_back_another() {
    tester!(alix, disable_workers);
    let broken = alix.create_group(None, None)?;
    let healthy = alix.create_group(None, None)?;
    broken.update_group_name("broken".into()).await?;
    healthy.update_group_name("healthy".into()).await?;

    let (client, script) = scripted::scripted(&alix.client).await;
    let broken_group = broken.group_id;
    let mut worker = CommitLogWorker::new(client.context.clone()).with_seal_fault(Arc::new(
        move |group_id, sealed| {
            let mut sealed = sealed?;
            if *group_id == broken_group {
                sealed.nonce.pop();
            }
            Ok(sealed)
        },
    ));

    for turn in 1..=2 {
        if turn == 2 {
            healthy.update_group_name("later".into()).await?;
        }
        assert_eq!(
            publish_turn(&mut worker).await,
            vec![healthy.group_id.to_vec()]
        );
        assert_eq!(worker.failed_publishes(), &[broken.group_id]);
        assert_eq!(upload_cursor(&alix, &broken.group_id), Cursor(0));
        assert_eq!(
            script
                .lock()
                .publishes_for(&keys_of(&broken).commit_log_id()),
            0,
            "the local parse refuses the entry before any request"
        );
        let tip = healthy.local_commit_log().await?.last()?.rowid;
        assert_eq!(upload_cursor(&alix, &healthy.group_id), Cursor(tip as u64));
        assert_eq!(
            remote_entries(&alix, &healthy).await,
            publishable(&healthy).await
        );
    }
}

/// A conversation whose stored group state does not load holds back only
/// itself: the other conversation still publishes in the same turn, and the
/// turn does not fail.
// verifies: FORK-021
#[xmtp_common::test(unwrap_try = true)]
async fn a_conversation_whose_state_does_not_load_does_not_hold_back_another() {
    use openmls_traits::storage::StorageProvider as _;

    tester!(alix, disable_workers);
    let broken = alix.create_group(None, None)?;
    let healthy = alix.create_group(None, None)?;
    broken.update_group_name("broken".into()).await?;
    healthy.update_group_name("healthy".into()).await?;
    let broken_id = keys_of(&broken).commit_log_id().to_vec();
    alix.context
        .mls_storage()
        .delete_context(&openmls::group::GroupId::from_slice(
            broken.group_id.as_slice(),
        ))?;

    let (client, script) = scripted::scripted(&alix.client).await;
    let mut worker = CommitLogWorker::new(client.context.clone());
    assert_eq!(
        publish_turn(&mut worker).await,
        vec![healthy.group_id.to_vec()]
    );
    assert_eq!(upload_cursor(&alix, &broken.group_id), Cursor(0));
    assert_eq!(script.lock().publishes_for(&broken_id), 0);
    assert_eq!(
        remote_entries(&alix, &healthy).await,
        publishable(&healthy).await
    );
}

/// A transport failure ends the publishing part of the turn. Once the first
/// conversation's publish has failed through its retries, no other
/// conversation is tried in that turn, so a turn costs one retry cycle, not
/// one per conversation. The next turn publishes every conversation.
// verifies: FORK-021
#[xmtp_common::test(unwrap_try = true)]
async fn a_transport_failure_ends_the_publishing_turn() {
    tester!(alix, disable_workers);
    let first = alix.create_group(None, None)?;
    let second = alix.create_group(None, None)?;
    first.update_group_name("first".into()).await?;
    second.update_group_name("second".into()).await?;

    let (client, script) = scripted::scripted(&alix.client).await;
    script.lock().unavailable = true;
    let mut worker = CommitLogWorker::new(client.context.clone());
    assert!(publish_turn(&mut worker).await.is_empty());
    let attempted: std::collections::HashSet<Vec<u8>> =
        script.lock().published.iter().flatten().cloned().collect();
    assert_eq!(attempted.len(), 1, "one conversation is tried");
    assert_eq!(worker.failed_publishes().len(), 1);

    script.lock().unavailable = false;
    let mut published = publish_turn(&mut worker).await;
    published.sort();
    let mut expected = vec![first.group_id.to_vec(), second.group_id.to_vec()];
    expected.sort();
    assert_eq!(published, expected);
    assert!(worker.failed_publishes().is_empty());
}

/// A publish that stores only part of a conversation's backlog moves the
/// upload cursor past the stored entries. The next turn starts at the first
/// entry that was not stored. The backlog is split by a size refusal; the
/// first half is stored and the second half is refused.
// verifies: FORK-021
#[xmtp_common::test(unwrap_try = true)]
async fn a_partly_stored_publish_resumes_from_its_first_unstored_entry() {
    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    for name in ["one", "two", "three"] {
        group.update_group_name(name.into()).await?;
    }
    let rows: Vec<i64> = group
        .local_commit_log()
        .await?
        .iter()
        .map(|log| log.rowid as i64)
        .collect();
    let count = rows.len();
    assert!(count >= 2, "{count} entries");

    let (client, script) = scripted::scripted(&alix.client).await;
    let commit_log_id = keys_of(&group).commit_log_id().to_vec();
    {
        let mut script = script.lock();
        script.too_large_once.insert(commit_log_id.clone());
        script.refuse_after.insert(commit_log_id.clone(), 1);
    }
    let mut worker = CommitLogWorker::new(client.context.clone());
    assert!(publish_turn(&mut worker).await.is_empty());
    let stored = count / 2;
    assert_eq!(
        upload_cursor(&alix, &group.group_id),
        Cursor(rows[stored - 1] as u64),
        "the cursor moves past the stored half only"
    );

    script.lock().refuse_after.clear();
    let sent_before = script.lock().published.len();
    assert_eq!(
        publish_turn(&mut worker).await,
        vec![group.group_id.to_vec()]
    );
    let requests = script.lock().published[sent_before..].to_vec();
    assert_eq!(
        requests.iter().map(Vec::len).collect::<Vec<_>>(),
        vec![count - stored],
        "the next turn sends only the unstored entries"
    );
    assert_eq!(
        upload_cursor(&alix, &group.group_id),
        Cursor(*rows.last()? as u64)
    );
    assert_eq!(
        remote_entries(&alix, &group).await,
        publishable(&group).await
    );
}

/// A seal that fails its self-check holds back only its own conversation for
/// the turn. None of its entries is sent, and the log names the failure but
/// none of the entry's sealed bytes. The next turn seals it again.
// verifies: FORK-074
#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
async fn a_seal_failure_skips_only_that_conversation() {
    use xmtp_logging::{Level, test_logging::LogCapture};

    tester!(alix, disable_workers);
    let faulty = alix.create_group(None, None)?;
    let healthy = alix.create_group(None, None)?;
    faulty.update_group_name("faulty".into()).await?;
    healthy.update_group_name("healthy".into()).await?;

    let (client, script) = scripted::scripted(&alix.client).await;
    let faulty_group = faulty.group_id;
    let failing = Arc::new(AtomicBool::new(true));
    let fault = failing.clone();
    let mut worker = CommitLogWorker::new(client.context.clone()).with_seal_fault(Arc::new(
        move |group_id, sealed| {
            if *group_id == faulty_group && fault.load(Ordering::SeqCst) {
                return Err(SealError::SelfCheck);
            }
            sealed
        },
    ));

    let capture = LogCapture::new(Level::Info);
    let published = {
        // The current-thread runtime polls spawned tasks on this thread too.
        let _logging = tracing::dispatcher::set_default(&capture.dispatch());
        publish_turn(&mut worker).await
    };
    assert_eq!(published, vec![healthy.group_id.to_vec()]);
    assert_eq!(upload_cursor(&alix, &faulty.group_id), Cursor(0));
    let faulty_keys = keys_of(&faulty);
    assert_eq!(script.lock().publishes_for(&faulty_keys.commit_log_id()), 0);
    let output = capture.output();
    assert!(output.contains("self-check failed"), "{output}");
    for entry in publishable(&faulty).await {
        let sealed = seal_entry(&faulty_keys, &entry)?;
        for bytes in [&sealed.signature, &sealed.nonce, &sealed.ciphertext] {
            assert!(!output.contains(&hex::encode(bytes)), "{output}");
        }
    }

    failing.store(false, Ordering::SeqCst);
    assert_eq!(
        publish_turn(&mut worker).await,
        vec![faulty.group_id.to_vec()]
    );
    let tip = faulty.local_commit_log().await?.last()?.rowid;
    assert_eq!(upload_cursor(&alix, &faulty.group_id), Cursor(tip as u64));
}

/// A failed publish ends the worker's turn, not the worker. The worker keeps
/// running, and it does not seal or send the refused entries again before
/// its next interval tick.
// verifies: FORK-021
#[xmtp_common::test(unwrap_try = true)]
async fn a_failed_publish_ends_the_turn_not_the_worker() {
    use futures::future::{Either, select};

    // The first tick runs at once on native and one interval after the start
    // on wasm, so the interval bounds both the wait for the first tick and
    // the room after it. The refused publish must be observed, and the window
    // after it must close, before the next tick.
    const INTERVAL: Duration = Duration::from_secs(10);
    const AFTER_PUBLISH: Duration = Duration::from_secs(2);
    tester!(
        alix,
        disable_workers,
        fork_recovery_opts: crate::builder::ForkRecoveryOpts {
            worker_interval_ns: Some(INTERVAL.as_nanos() as u64),
            ..Default::default()
        }
    );
    let group = alix.create_group(None, None)?;
    group.update_group_name("refused".into()).await?;
    let pending = publishable(&group).await.len();
    assert!(pending > 0);

    let (client, script) = scripted::scripted(&alix.client).await;
    let commit_log_id = keys_of(&group).commit_log_id().to_vec();
    script.lock().refuse.insert(commit_log_id.clone());
    let seals = Arc::new(AtomicUsize::new(0));
    let counted = seals.clone();
    let mut worker =
        CommitLogWorker::new(client.context.clone()).with_seal_fault(Arc::new(move |_, sealed| {
            counted.fetch_add(1, Ordering::SeqCst);
            sealed
        }));

    let window = async {
        xmtp_common::time::timeout(INTERVAL * 2, async {
            while script.lock().publishes_for(&commit_log_id) == 0 {
                xmtp_common::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("the first tick publishes");
        xmtp_common::time::sleep(AFTER_PUBLISH).await;
    };
    let run = worker._run();
    futures::pin_mut!(window, run);
    if let Either::Right((stopped, _)) = select(window, run).await {
        panic!("the worker stopped: {stopped:?}");
    }
    assert_eq!(seals.load(Ordering::SeqCst), pending, "sealed once");
    assert_eq!(script.lock().publishes_for(&commit_log_id), 1, "sent once");
}

/// The read path opens each envelope under the conversation's stored
/// commit-log id, with no first-entry selection. A valid entry sealed under
/// another root's commit-log id, an entry signed by another key, one that
/// does not decrypt, one that names another group, entries with a 0-, 11-,
/// or 13-byte nonce or a 15-byte ciphertext, and an envelope that holds no
/// sealed entry are each discarded without a panic. The valid chain after
/// them in the same read is accepted from its first entry, and the chain
/// rules then discard an entry with the wrong last epoch authenticator and a
/// repeated entry. A later read extends the chain. Discard logs never name
/// the group id.
// verifies: FORK-077, FORK-083
#[xmtp_common::test(unwrap_try = true)]
async fn a_reader_discards_each_entry_that_fails_fork_075() {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let alix_group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    bo.sync_welcomes().await?;
    let bo_group = bo.group(&alix_group.group_id)?;
    bo_group.update_consent_state(ConsentState::Allowed)?;

    alix_group
        .update_group_name("processed prefix".into())
        .await?;
    bo_group.sync().await?;

    let keys = keys_of(&bo_group);
    let seed = commit_log_seed(&root_of(&bo_group));
    let id = keys.commit_log_id();
    let group_id = bo_group.group_id.to_vec();
    let entry = |commit_sequence_id, epoch: u8| {
        let mut entry = PlaintextCommitLogEntry {
            group_id: group_id.clone(),
            commit_sequence_id,
            last_epoch_authenticator: vec![epoch; 32],
            commit_result: ProtoCommitResult::Applied as i32,
            applied_epoch_number: u64::from(epoch) + 1,
            applied_epoch_authenticator: vec![epoch + 1; 32],
            ..Default::default()
        };
        sign_entry_author(
            &keys,
            &alix.context.identity().installation_keys,
            &mut entry,
        )
        .unwrap();
        entry
    };
    let first = seal_entry(&keys, &entry(1, 1))?;
    let second = seal_entry(&keys, &entry(2, 2))?;
    // The test signer signs exactly as the commit-log signing key does.
    assert_eq!(
        signed_entry(&seed, &id, first.nonce.clone(), first.ciphertext.clone()),
        first
    );

    // Accepted, either of these would start another chain, and the valid
    // chain would then be discarded by the chain rules.
    let forged = seal_entry(&keys, &entry(1, 40))?;
    let other_root = GroupRoot::generate();
    let other_seed = commit_log_seed(&other_root);
    let mut other_group = entry(1, 30);
    other_group.group_id = vec![9; 32];
    sign_entry_author(
        &keys,
        &alix.context.identity().installation_keys,
        &mut other_group,
    )?;
    let discarded = [
        seal_entry(&GroupKeys::derive(&other_root), &entry(1, 50))?,
        signed_entry(&other_seed, &id, forged.nonce, forged.ciphertext),
        signed_entry(&seed, &id, vec![1; 12], vec![2; 32]),
        seal_entry(&keys, &other_group)?,
        signed_entry(&seed, &id, vec![], first.ciphertext.clone()),
        signed_entry(&seed, &id, vec![1; 11], first.ciphertext.clone()),
        signed_entry(&seed, &id, vec![1; 13], first.ciphertext.clone()),
        signed_entry(&seed, &id, first.nonce.clone(), vec![3; 15]),
    ];
    // Each opens under the conversation's keys; the chain rules discard both.
    let wrong_last_authenticator = seal_entry(&keys, &entry(3, 7))?;
    let repeated = seal_entry(&keys, &entry(2, 2))?;
    let topic = Topic::new_commit_log(id);
    let mut envelopes: Vec<_> = discarded
        .into_iter()
        .map(Payload::SealedCommitLogEntry)
        .chain([Payload::GroupMessage(Default::default())])
        .chain(
            [first, second, wrong_last_authenticator, repeated].map(Payload::SealedCommitLogEntry),
        )
        .enumerate()
        .map(|(i, payload)| server_envelope(&topic, i as u64 + 1, payload))
        .collect();
    let last_sequence_id = envelopes.len() as u64;

    let (client, script) = scripted::scripted(&bo.client).await;
    script
        .lock()
        .reads
        .insert(topic.cloned_vec(), envelopes.clone());
    let mut worker = CommitLogWorker::new(client.context.clone());
    #[cfg(not(target_arch = "wasm32"))]
    let capture = xmtp_logging::test_logging::LogCapture::new(xmtp_logging::Level::Info);
    let results = {
        #[cfg(not(target_arch = "wasm32"))]
        let _logging = tracing::dispatcher::set_default(&capture.dispatch());
        worker
            .run_test(CommitLogTestFunction::SaveRemoteCommitLog, None)
            .await?
    };
    #[cfg(not(target_arch = "wasm32"))]
    {
        let output = capture.output();
        assert!(output.contains("Discarding"), "{output}");
        assert!(
            !output.contains(&hex::encode(bo_group.group_id.as_slice())),
            "a discard log names the group id: {output}"
        );
    }
    let saved = results[0].save_remote_commit_log_results.as_ref()?;
    assert_eq!(saved[bo_group.group_id.as_slice()], 2);
    let remote = bo.context.db().get_remote_commit_log_after_cursor(
        &bo_group.group_id,
        0,
        RemoteCommitLogOrder::AscendingByRowid,
    )?;
    assert_eq!(
        remote
            .iter()
            .map(|log| (log.commit_sequence_id, log.applied_epoch_number))
            .collect::<Vec<_>>(),
        vec![(1, 2), (2, 3)]
    );
    // The discarded envelopes are read past, never evaluated again.
    assert_eq!(
        bo.context
            .db()
            .get_last_cursor(bo_group.group_id, EntityKind::CommitLogDownload)?,
        Cursor(last_sequence_id)
    );
    assert_eq!(script.lock().read, vec![vec![topic.cloned_vec()]]);

    // A later read extends the accepted chain.
    envelopes.push(server_envelope(
        &topic,
        last_sequence_id + 1,
        Payload::SealedCommitLogEntry(seal_entry(&keys, &entry(3, 3))?),
    ));
    script.lock().reads.insert(topic.cloned_vec(), envelopes);
    let results = worker
        .run_test(CommitLogTestFunction::SaveRemoteCommitLog, None)
        .await?;
    let saved = results[0].save_remote_commit_log_results.as_ref()?;
    assert_eq!(saved[bo_group.group_id.as_slice()], 1);
}

/// Restored placeholders hold no root, so a worker turn neither publishes
/// nor reads a commit log for them, even when one is allowed and has local
/// entries, and no request names a topic built from their ids.
// verifies: FORK-030
#[xmtp_common::test(unwrap_try = true)]
async fn a_restored_placeholder_is_skipped_by_publish_and_read() {
    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    group.update_group_name("live".into()).await?;
    let db = alix.context.db();
    let mut placeholders = Vec::new();
    for archived_id in [vec![0x42; 16], xmtp_common::rand_vec::<32>()] {
        let row = MlsGroup::insert(
            &alix.context,
            Some(&archived_id),
            GroupMembershipState::Restored,
            ConversationType::Group,
            PolicySet::default(),
            GroupMetadataOptions::default(),
            None,
            false,
        )?;
        StoredConsentRecord::new(
            ConsentType::ConversationId,
            ConsentState::Allowed,
            hex::encode(&archived_id),
        )
        .merge_into(&db)?;
        NewLocalCommitLog {
            group_id: row.id,
            commit_sequence_id: 7,
            last_epoch_authenticator: vec![1; 32],
            commit_result: CommitResult::Success,
            error_message: None,
            applied_epoch_number: 2,
            applied_epoch_authenticator: vec![2; 32],
            sender_inbox_id: None,
            sender_installation_id: None,
            commit_type: None,
        }
        .store(&db)?;
        placeholders.push(row.id);
    }

    let (client, script) = scripted::scripted(&alix.client).await;
    CommitLogWorker::new(client.context.clone())._tick().await?;

    let commit_log_id = keys_of(&group).commit_log_id().to_vec();
    let script = script.lock();
    assert!(!script.read.is_empty());
    for topic in script.read.iter().flatten() {
        assert_eq!(topic[1..], commit_log_id[..], "read a placeholder's topic");
    }
    for id in script.published.iter().flatten() {
        assert_eq!(*id, commit_log_id, "published a placeholder's entry");
    }
    for id in &placeholders {
        for kind in [EntityKind::CommitLogUpload, EntityKind::CommitLogDownload] {
            assert_eq!(db.get_last_cursor(id, kind)?, Cursor(0));
        }
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_should_publish_commit_log() {
    tester!(alix);
    tester!(bo);

    let alix_group = alix.create_group(None, None).unwrap();
    alix_group.add_members(&[bo.inbox_id()]).await.unwrap();
    bo.sync_all_welcomes_and_groups(None).await.unwrap();

    let binding = bo.list_conversations(GroupQueryArgs::default()).unwrap();
    let bo_group = binding.first().unwrap();
    assert_eq!(bo_group.group.group_id, alix_group.group_id);

    let alix_should_publish_commit_log_groups = alix
        .find_groups(GroupQueryArgs {
            should_publish_commit_log: Some(true),
            ..Default::default()
        })
        .unwrap();

    let bo_should_publish_commit_log_groups = bo
        .find_groups(GroupQueryArgs {
            should_publish_commit_log: Some(true),
            ..Default::default()
        })
        .unwrap();

    assert_eq!(alix_should_publish_commit_log_groups.len(), 1);
    assert_eq!(bo_should_publish_commit_log_groups.len(), 0);
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_download_commit_log_from_remote() {
    // Disable background CommitLogWorker for deterministic testing
    tester!(alix, with_commit_log_worker: false);
    tester!(bo, with_commit_log_worker: false);

    // Alix creates a group with Bo (1 commit)
    let alix_group = alix.create_group(None, None).unwrap();
    alix_group.add_members(&[bo.inbox_id()]).await.unwrap();

    // Alix updates the group name (2 commits)
    alix_group
        .update_group_name("foo".to_string())
        .await
        .unwrap();

    // Alix updates the group name again (3 commits)
    alix_group
        .update_group_name("bar".to_string())
        .await
        .unwrap();

    bo.sync_all_welcomes_and_groups(None).await.unwrap();
    let binding = bo.find_groups(GroupQueryArgs::default()).unwrap();
    let bo_group = binding.first().unwrap();
    bo_group.sync().await.unwrap();

    // Bo sends a message which updates the group to be consent state allowed
    // and queues a key update intent (4 commits)
    bo_group
        .send_message(b"foo", SendMessageOpts::default())
        .await
        .unwrap();

    // Bo updates the group name (5 commits)
    bo_group
        .update_group_name("bo group name".to_string())
        .await
        .unwrap();
    alix_group.sync().await.unwrap();

    // Before Alix publishes commits upload commit cursor should be 0 for both groups:
    let alix_group_1_cursor = alix
        .context
        .db()
        .get_last_cursor(
            alix_group.group_id,
            xmtp_db::refresh_state::EntityKind::CommitLogUpload,
        )
        .unwrap();
    assert_eq!(alix_group_1_cursor, Cursor(0));

    // Verify that publish works as expected
    let mut commit_log_worker = CommitLogWorker::new(alix.context.clone());
    let test_results = commit_log_worker
        .run_test(CommitLogTestFunction::PublishCommitLogsToRemote, Some(1))
        .await
        .unwrap();
    // We only ran the worker one time
    assert_eq!(test_results.len(), 1);
    // We published commit log entries for one group
    assert_eq!(
        test_results[0]
            .publish_commit_log_results
            .as_ref()
            .unwrap()
            .len(),
        1
    );
    // We have saved zero remote commit log entries so far
    assert!(test_results[0].save_remote_commit_log_results.is_none());

    // Running for bo  should have no publish commit log results since they are not super admins
    let mut commit_log_worker = CommitLogWorker::new(bo.context.clone());
    let bo_test_results = commit_log_worker
        .run_test(CommitLogTestFunction::PublishCommitLogsToRemote, Some(1))
        .await
        .unwrap();
    assert_eq!(bo_test_results.len(), 1);
    assert!(
        bo_test_results[0]
            .publish_commit_log_results
            .as_ref()
            .unwrap()
            .is_empty()
    );

    // Verify the number of commits published results for alix group 1
    assert_eq!(
        test_results[0].publish_commit_log_results.clone().unwrap()[0]
            .conversation_id
            .as_slice(),
        alix_group.group_id.as_slice()
    );
    // We should have published 4 commits
    assert_eq!(
        test_results[0].publish_commit_log_results.clone().unwrap()[0].num_entries_published,
        5
    );

    // After Alix publishes commits upload commit cursor should be equal to publish results last rowid:
    let alix_group_1_cursor = alix
        .context
        .db()
        .get_last_cursor(
            alix_group.group_id,
            xmtp_db::refresh_state::EntityKind::CommitLogUpload,
        )
        .unwrap();

    let alix_group1_publish_result_upload_cursor =
        test_results[0].publish_commit_log_results.clone().unwrap()[0].last_entry_published_rowid;
    assert_eq!(
        alix_group_1_cursor,
        Cursor(alix_group1_publish_result_upload_cursor as u64)
    );

    // Verify that when we save remote commit log entries for alix and bo, that we get the same results
    let mut commit_log_worker_alix = CommitLogWorker::new(alix.context.clone());
    let alix_test_results = commit_log_worker_alix
        .run_test(CommitLogTestFunction::SaveRemoteCommitLog, None)
        .await
        .unwrap();
    let mut commit_log_worker_bo = CommitLogWorker::new(bo.context.clone());
    let bo_test_results = commit_log_worker_bo
        .run_test(CommitLogTestFunction::SaveRemoteCommitLog, None)
        .await
        .unwrap();
    assert_eq!(alix_test_results.len(), 1);
    assert_eq!(
        alix_test_results[0]
            .save_remote_commit_log_results
            .as_ref()
            .unwrap()
            .len(),
        1
    );

    assert_eq!(
        *alix_test_results[0]
            .save_remote_commit_log_results
            .as_ref()
            .unwrap()
            .get(alix_group.group_id.as_slice())
            .unwrap(),
        5
    );

    assert_eq!(bo_test_results.len(), 1);
    assert_eq!(
        bo_test_results[0]
            .save_remote_commit_log_results
            .as_ref()
            .unwrap()
            .len(),
        1
    );

    assert_eq!(
        *bo_test_results[0]
            .save_remote_commit_log_results
            .as_ref()
            .unwrap()
            .get(alix_group.group_id.as_slice())
            .unwrap(),
        5
    );
    // Verify that cursor works as expected for saving new remote commit log entries
    // Alix updates the group name (1 new commit (6 total))
    alix_group
        .update_group_name("one".to_string())
        .await
        .unwrap();

    // Alix updates the group name again (2 new commits (8 total))
    alix_group
        .update_group_name("two".to_string())
        .await
        .unwrap();
    alix_group.sync().await.unwrap();
    bo_group.sync().await.unwrap();

    let mut commit_log_worker_alix = CommitLogWorker::new(alix.context.clone());
    // Alix publishes commits and saves remote commit log entries
    // We run the worker twice since publish happens after save
    let alix_test_results = commit_log_worker_alix
        .run_test(CommitLogTestFunction::All, Some(2))
        .await
        .unwrap();

    // Alix should published for one conversation
    assert_eq!(
        alix_test_results[0]
            .publish_commit_log_results
            .as_ref()
            .unwrap()
            .len(),
        1
    );
    // The publish matches the conversation id
    assert_eq!(
        alix_test_results[0]
            .publish_commit_log_results
            .as_ref()
            .unwrap()[0]
            .conversation_id
            .as_slice(),
        alix_group.group_id.as_slice()
    );
    // We published 2 new commits
    assert_eq!(
        alix_test_results[0]
            .publish_commit_log_results
            .as_ref()
            .unwrap()[0]
            .num_entries_published,
        2
    );
    // We saved results for one conversation on the second worker run
    assert_eq!(
        alix_test_results[1]
            .save_remote_commit_log_results
            .as_ref()
            .unwrap()
            .len(),
        1
    );

    // We should have saved 2 new entries...
    assert_eq!(
        *alix_test_results[1]
            .save_remote_commit_log_results
            .as_ref()
            .unwrap()
            .get(alix_group.group_id.as_slice())
            .unwrap(),
        2
    );

    alix_group
        .update_group_name("three".to_string())
        .await
        .unwrap();
    alix_group
        .update_group_name("four".to_string())
        .await
        .unwrap();
    alix_group.sync().await.unwrap();
    bo_group.sync().await.unwrap();

    let mut commit_log_worker_alix = CommitLogWorker::new(alix.context.clone());
    let alix_test_results = commit_log_worker_alix
        .run_test(CommitLogTestFunction::All, Some(2))
        .await
        .unwrap();

    let mut commit_log_worker_bo = CommitLogWorker::new(bo.context.clone());
    let bo_test_results = commit_log_worker_bo
        .run_test(CommitLogTestFunction::All, None)
        .await
        .unwrap();

    // Alix should have saved 2 new entries, while bo should have saved 4
    assert_eq!(
        alix_test_results[1]
            .save_remote_commit_log_results
            .as_ref()
            .unwrap()
            .len(),
        1
    );

    assert_eq!(
        *alix_test_results[1]
            .save_remote_commit_log_results
            .as_ref()
            .unwrap()
            .get(bo_group.group_id.as_slice())
            .unwrap(),
        2
    );

    assert_eq!(
        bo_test_results[0]
            .save_remote_commit_log_results
            .as_ref()
            .unwrap()
            .len(),
        1
    );

    assert_eq!(
        *bo_test_results[0]
            .save_remote_commit_log_results
            .as_ref()
            .unwrap()
            .get(bo_group.group_id.as_slice())
            .unwrap(),
        4
    );
}

/// The chain conditions, on entries that already opened.
// verifies: FORK-083
#[xmtp_common::test(unwrap_try = true)]
fn test_should_skip_remote_log_entry() {
    // Does not skip if entry meets all conditions
    let latest_saved_remote_log = RemoteCommitLog {
        rowid: 0,
        log_sequence_id: 0,
        group_id: GroupId::from([0x11u8; 16]),
        commit_sequence_id: 100,
        commit_result: CommitResult::Success,
        applied_epoch_number: 3,
        applied_epoch_authenticator: vec![0x01, 0x02, 0x03],
        author_installation_key: vec![],
    };

    let entry = PlaintextCommitLogEntry {
        group_id: vec![0x11, 0x22, 0x33],
        commit_sequence_id: 101,
        last_epoch_authenticator: vec![0x01, 0x02, 0x03],
        commit_result: 1,
        applied_epoch_number: 4,
        applied_epoch_authenticator: vec![0x01, 0x02, 0x04],
        ..Default::default()
    };
    assert!(!should_skip_remote_commit_log_entry(
        Some(&latest_saved_remote_log),
        &entry,
        true,
    ));

    // Skips if commit_sequence_id of the entry is not greater than the most recently stored entry, if one exists.
    let latest_saved_remote_log = RemoteCommitLog {
        rowid: 0,
        log_sequence_id: 0,
        group_id: GroupId::from([0x11u8; 16]),
        commit_sequence_id: 100,
        commit_result: CommitResult::Success,
        applied_epoch_number: 3,
        applied_epoch_authenticator: vec![0x01, 0x02, 0x03],
        author_installation_key: vec![],
    };
    let entry = PlaintextCommitLogEntry {
        group_id: vec![0x11, 0x22, 0x33],
        commit_sequence_id: 99,
        last_epoch_authenticator: vec![0x01, 0x02, 0x03],
        commit_result: 1,
        applied_epoch_number: 4,
        applied_epoch_authenticator: vec![0x01, 0x02, 0x04],
        ..Default::default()
    };
    assert!(should_skip_remote_commit_log_entry(
        Some(&latest_saved_remote_log),
        &entry,
        true,
    ));

    // Skips if the last_epoch_authenticator does not match the epoch_authenticator of
    // the most recently stored entry with a CommitResult of COMMIT_RESULT_APPLIED, if one exists.
    let latest_saved_remote_log = RemoteCommitLog {
        rowid: 0,
        log_sequence_id: 0,
        group_id: GroupId::from([0x11u8; 16]),
        commit_sequence_id: 100,
        commit_result: CommitResult::Success,
        applied_epoch_number: 3,
        applied_epoch_authenticator: vec![0x01, 0x02, 0x03],
        author_installation_key: vec![],
    };
    let entry = PlaintextCommitLogEntry {
        group_id: vec![0x11, 0x22, 0x33],
        commit_sequence_id: 101,
        last_epoch_authenticator: vec![0x01, 0x02, 0x05],
        commit_result: 1,
        applied_epoch_number: 4,
        applied_epoch_authenticator: vec![0x01, 0x02, 0x04],
        ..Default::default()
    };
    assert!(should_skip_remote_commit_log_entry(
        Some(&latest_saved_remote_log),
        &entry,
        true,
    ));

    // Skips if the applied_epoch_number of the entry is not exactly 1 greater than
    // the latest_applied_epoch_number of the remote validation info. (skipped from 3 to 5)
    let latest_saved_remote_log = RemoteCommitLog {
        rowid: 0,
        log_sequence_id: 0,
        group_id: GroupId::from([0x11u8; 16]),
        commit_sequence_id: 100,
        commit_result: CommitResult::Success,
        applied_epoch_number: 3,
        applied_epoch_authenticator: vec![0x01, 0x02, 0x03],
        author_installation_key: vec![],
    };
    let entry = PlaintextCommitLogEntry {
        group_id: vec![0x11, 0x22, 0x33],
        commit_sequence_id: 101,
        last_epoch_authenticator: vec![0x01, 0x02, 0x03],
        commit_result: 1,
        applied_epoch_number: 5,
        applied_epoch_authenticator: vec![0x01, 0x02, 0x04],
        ..Default::default()
    };
    assert!(should_skip_remote_commit_log_entry(
        Some(&latest_saved_remote_log),
        &entry,
        true,
    ));

    // Skips if the applied_epoch_number of the entry is not exactly 1 greater than
    // the latest_applied_epoch_number of the remote validation info. (stayed at 3)
    let latest_saved_remote_log = RemoteCommitLog {
        rowid: 0,
        log_sequence_id: 0,
        group_id: GroupId::from([0x11u8; 16]),
        commit_sequence_id: 100,
        commit_result: CommitResult::Success,
        applied_epoch_number: 3,
        applied_epoch_authenticator: vec![0x01, 0x02, 0x03],
        author_installation_key: vec![],
    };
    let entry = PlaintextCommitLogEntry {
        group_id: vec![0x11, 0x22, 0x33],
        commit_sequence_id: 101,
        last_epoch_authenticator: vec![0x01, 0x02, 0x03],
        commit_result: 1,
        applied_epoch_number: 3,
        applied_epoch_authenticator: vec![0x01, 0x02, 0x04],
        ..Default::default()
    };
    assert!(should_skip_remote_commit_log_entry(
        Some(&latest_saved_remote_log),
        &entry,
        true,
    ));

    // Skips if the applied_epoch_number of the entry is not exactly 1 greater than
    // the latest_applied_epoch_number of the remote validation info. (decreased from 3 to 2)
    let latest_saved_remote_log = RemoteCommitLog {
        rowid: 0,
        log_sequence_id: 0,
        group_id: GroupId::from([0x11u8; 16]),
        commit_sequence_id: 100,
        commit_result: CommitResult::Success,
        applied_epoch_number: 3,
        applied_epoch_authenticator: vec![0x01, 0x02, 0x03],
        author_installation_key: vec![],
    };
    let entry = PlaintextCommitLogEntry {
        group_id: vec![0x11, 0x22, 0x33],
        commit_sequence_id: 101,
        last_epoch_authenticator: vec![0x01, 0x02, 0x03],
        commit_result: 1,
        applied_epoch_number: 2,
        applied_epoch_authenticator: vec![0x01, 0x02, 0x04],
        ..Default::default()
    };
    assert!(should_skip_remote_commit_log_entry(
        Some(&latest_saved_remote_log),
        &entry,
        true,
    ));

    // Skips if entry CommitResult is not COMMIT_RESULT_APPLIED, and the epoch authenticator or epoch number does not match the most recently applied values
    let latest_saved_remote_log = RemoteCommitLog {
        rowid: 0,
        log_sequence_id: 0,
        group_id: GroupId::from([0x11u8; 16]),
        commit_sequence_id: 100,
        commit_result: CommitResult::Success,
        applied_epoch_number: 3,
        applied_epoch_authenticator: vec![0x01, 0x02, 0x03],
        author_installation_key: vec![],
    };
    let entry = PlaintextCommitLogEntry {
        group_id: vec![0x11, 0x22, 0x33],
        commit_sequence_id: 101,
        last_epoch_authenticator: vec![0x01, 0x02, 0x03],
        commit_result: 2,
        applied_epoch_number: 4,
        applied_epoch_authenticator: vec![0x01, 0x02, 0x03],
        ..Default::default()
    };
    assert!(should_skip_remote_commit_log_entry(
        Some(&latest_saved_remote_log),
        &entry,
        true,
    ));
    let entry = PlaintextCommitLogEntry {
        group_id: vec![0x11, 0x22, 0x33],
        commit_sequence_id: 101,
        last_epoch_authenticator: vec![0x01, 0x02, 0x03],
        commit_result: 2,
        applied_epoch_number: 3,
        applied_epoch_authenticator: vec![0x01, 0x02, 0x04],
        ..Default::default()
    };
    assert!(should_skip_remote_commit_log_entry(
        Some(&latest_saved_remote_log),
        &entry,
        true,
    ));
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_consecutive_entries_verification_happy_case() {
    // Disable background CommitLogWorker for deterministic testing
    tester!(alix, with_commit_log_worker: false);
    tester!(bo, with_commit_log_worker: false);

    // Create a DM between alix and bo
    let alix_dm = alix.find_or_create_dm(bo.inbox_id(), None).await?;
    let bo_dm = bo.wait_for_welcomes().await?.first()?.to_owned();

    // Sync messages to bo
    bo_dm.sync().await?;
    // Only consented DM's are checked in the commit log
    bo_dm.update_consent_state(ConsentState::Allowed)?;
    let messages = bo_dm.find_messages(&MsgQueryArgs::default())?;
    // Should see 1 messages: group_updated
    assert_eq!(messages.len(), 1);

    // Check alix's commit log - should have exact sequence: GroupCreation, UpdateGroupMembership
    let alix_logs_before_publish = alix_dm.local_commit_log().await?;
    print_commit_log_with_types(&alix_dm).await;
    assert_commit_sequence(
        &alix_logs_before_publish,
        &[CommitType::GroupCreation, CommitType::UpdateGroupMembership],
    );

    // Setup commit log workers for both parties
    let mut alix_worker = CommitLogWorker::new(alix.context.clone());
    let mut bo_worker = CommitLogWorker::new(bo.context.clone());

    // Alix (creator) publishes the commit log entries first
    let alix_publish_results = alix_worker
        .run_test(CommitLogTestFunction::PublishCommitLogsToRemote, None)
        .await?;

    // Verify alix published entries successfully
    assert!(!alix_publish_results.is_empty());
    let alix_publish_result = alix_publish_results[0]
        .publish_commit_log_results
        .as_ref()
        .unwrap();
    assert_eq!(alix_publish_result.len(), 1);
    assert_eq!(
        alix_publish_result[0].conversation_id,
        alix_dm.group_id.to_vec()
    );
    // Only UpdateGroupMembership gets published (GroupCreation is not published to remote log)
    assert_eq!(alix_publish_result[0].num_entries_published, 1);

    // Now bo tries to download and verify the consecutive entries
    let bo_download_results = bo_worker
        .run_test(CommitLogTestFunction::SaveRemoteCommitLog, None)
        .await?;

    // Verify bo successfully downloaded and verified all entries
    assert!(!bo_download_results.is_empty());
    let bo_download_result = bo_download_results[0]
        .save_remote_commit_log_results
        .as_ref()
        .unwrap();
    assert_eq!(bo_download_result.len(), 1);
    assert!(bo_download_result.contains_key(bo_dm.group_id.as_slice()));
    assert_eq!(
        bo_download_result[bo_dm.group_id.as_slice()],
        1,
        "Bo should successfully verify the 1 entry from creator (UpdateGroupMembership)"
    );

    // Check bo's commit log after downloading and verifying entries
    let bo_logs_after_download = bo_dm.local_commit_log().await?;
    print_commit_log_with_types(&bo_dm).await;
    // Bo should only have Welcome at this point (before sending any messages)
    assert_commit_sequence(&bo_logs_after_download, &[CommitType::Welcome]);

    // Bo's first message will generate a KeyUpdate commit (key rotation)
    bo_dm
        .send_message("Message from bo".as_bytes(), SendMessageOpts::default())
        .await?;

    // Sync to alix
    alix_dm.sync().await?;
    let messages = alix_dm.find_messages(&MsgQueryArgs::default())?;
    // Should now have 2 messages: group_updated + 1 actual messages
    assert_eq!(messages.len(), 2);

    // Check bo's commit log after sending messages - should now have KeyUpdate commit
    let bo_logs_after_messages = bo_dm.local_commit_log().await?;
    print_commit_log_with_types(&bo_dm).await;
    // Bo should now have exact sequence: Welcome, KeyUpdate
    assert_commit_sequence(
        &bo_logs_after_messages,
        &[CommitType::Welcome, CommitType::KeyUpdate],
    );

    // Bo publishes his entries
    let bo_publish_results = bo_worker
        .run_test(CommitLogTestFunction::PublishCommitLogsToRemote, None)
        .await?;

    // Verify bo published entries successfully
    assert!(!bo_publish_results.is_empty());
    let bo_publish_result = bo_publish_results[0]
        .publish_commit_log_results
        .as_ref()
        .unwrap();
    assert_eq!(bo_publish_result.len(), 1);
    assert_eq!(
        bo_publish_result[0].conversation_id,
        bo_dm.group_id.to_vec()
    );
    // Bo should publish 1 commit log entry: KeyUpdate (Welcome is not published, it's only received)
    assert_eq!(bo_publish_result[0].num_entries_published, 1);

    // Alix downloads the new entries from bo
    let alix_download_results = alix_worker
        .run_test(CommitLogTestFunction::SaveRemoteCommitLog, None)
        .await?;

    // Verify alix successfully downloaded and verified bo's entries
    assert!(!alix_download_results.is_empty());
    let alix_download_result = alix_download_results[0]
        .save_remote_commit_log_results
        .as_ref()
        .unwrap();
    assert_eq!(alix_download_result.len(), 1);
    assert!(alix_download_result.contains_key(alix_dm.group_id.as_slice()));
    assert_eq!(
        alix_download_result[alix_dm.group_id.as_slice()],
        2,
        "Alix should download 2 entries total: her original UpdateGroupMembership + Bo's new KeyUpdate"
    );

    // Both parties verify consecutive entries published by each other under
    // the one commit-log id their shared root derives.
}

// verifies: FORK-087
#[xmtp_common::test(unwrap_try = true)]
async fn a_member_cannot_poison_the_commit_log() {
    tester!(alix, with_commit_log_worker: false);
    tester!(bo, with_commit_log_worker: false);

    let a = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let b = bo.wait_for_welcomes().await?.first()?.to_owned();
    a.update_group_name("shared".into()).await?;
    b.sync().await?;
    assert_eq!(
        a.epoch_authenticator().await?,
        b.epoch_authenticator().await?
    );

    // The honest prefix establishes the reader's accepted chain.
    let mut publisher = CommitLogWorker::new(alix.context.clone());
    publish_turn(&mut publisher).await;
    let mut reader = CommitLogWorker::new(alix.context.clone());
    reader
        .run_test(CommitLogTestFunction::SaveRemoteCommitLog, None)
        .await?;
    reader
        .run_test(CommitLogTestFunction::CheckForkedState, None)
        .await?;
    assert_eq!(
        alix.context
            .db()
            .get_group_commit_log_forked_status(&a.group_id)?,
        Some(false)
    );

    a.update_group_name("next".into()).await?;
    b.sync().await?;
    assert_eq!(
        a.epoch_authenticator().await?,
        b.epoch_authenticator().await?
    );
    let honest = PlaintextCommitLogEntry::from(a.local_commit_log().await?.last()?);
    let mut forged = PlaintextCommitLogEntry::from(b.local_commit_log().await?.last()?);
    assert_eq!(forged, honest);
    forged.applied_epoch_authenticator[0] ^= 1;
    assert_ne!(
        forged.applied_epoch_authenticator,
        honest.applied_epoch_authenticator
    );

    sign_entry_author(
        &keys_of(&b),
        &bo.context.identity().installation_keys,
        &mut forged,
    )?;

    // Bo uses only the shared root in his own group state and his authenticated
    // backend session. The backend stores this entry before Alix's worker runs.
    let mut stored = Vec::new();
    bo.context
        .api()
        .publish_commit_log(vec![seal_entry(&keys_of(&b), &forged)?], &mut stored)
        .await?;
    assert_eq!(stored.len(), 1);
    publish_turn(&mut publisher).await;

    let downloaded = reader
        .run_test(CommitLogTestFunction::SaveRemoteCommitLog, None)
        .await?;
    assert_eq!(
        downloaded[0].save_remote_commit_log_results.as_ref()?[a.group_id.as_slice()],
        1,
        "only the honest publisher entry is accepted"
    );
    let accepted = alix.context.db().get_remote_commit_log_after_cursor(
        &a.group_id,
        0,
        RemoteCommitLogOrder::DescendingByRowid,
    )?;
    assert_eq!(
        accepted[0].commit_sequence_id,
        honest.commit_sequence_id as i64
    );
    assert_eq!(
        accepted[0].applied_epoch_authenticator,
        honest.applied_epoch_authenticator
    );
    assert_eq!(
        accepted[0].author_installation_key,
        alix.context.installation_id().to_vec()
    );
    reader
        .run_test(CommitLogTestFunction::CheckForkedState, None)
        .await?;
    assert_eq!(
        alix.context
            .db()
            .get_group_commit_log_forked_status(&a.group_id)?,
        Some(false),
        "a non-publisher cannot forge a fork verdict"
    );
    assert_eq!(
        a.epoch_authenticator().await?,
        b.epoch_authenticator().await?
    );
}

// verifies: FORK-085
#[xmtp_common::test(unwrap_try = true)]
async fn the_publisher_flag_follows_the_super_admin_list() {
    use crate::groups::UpdateAdminListType::{AddSuper, RemoveSuper};
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let a = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    bo.sync_welcomes().await?;
    let b = bo.group(&a.group_id)?;
    assert!(
        !bo.context
            .db()
            .find_group(&a.group_id)??
            .should_publish_commit_log
    );
    a.update_admin_list(AddSuper, bo.inbox_id().to_string())
        .await?;
    b.sync().await?;
    assert!(
        bo.context
            .db()
            .find_group(&a.group_id)??
            .should_publish_commit_log
    );
    b.update_admin_list(RemoveSuper, alix.inbox_id().to_string())
        .await?;
    a.sync().await?;
    assert!(
        !alix
            .context
            .db()
            .find_group(&a.group_id)??
            .should_publish_commit_log
    );

    // Model the cached role left by an older client before replacing its MLS state.
    crate::state_tx::state_write(alix.context.mls_storage(), |tx| {
        use xmtp_db::XmtpMlsStorageProvider;
        tx.storage()
            .db()
            .set_should_publish_commit_log(&a.group_id, true)?;
        Ok::<_, xmtp_db::ConnectionError>(xmtp_db::TransactionOutcome::Continue(()))
    })?;
    b.readd_installations(vec![alix.context.installation_id().to_vec()])
        .await?;
    a.sync_with_conn().await?;
    alix.sync_welcomes().await?;
    assert!(
        !alix
            .context
            .db()
            .find_group(&a.group_id)??
            .should_publish_commit_log
    );
}

// verifies: FORK-085
#[xmtp_common::test(unwrap_try = true)]
async fn a_readded_publisher_does_not_publish_its_earlier_membership() {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let a = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    bo.sync_welcomes().await?;
    let b = bo.group(&a.group_id)?;
    b.update_consent_state(ConsentState::Allowed)?;
    a.update_admin_list(
        crate::groups::UpdateAdminListType::AddSuper,
        bo.inbox_id().to_string(),
    )
    .await?;
    a.update_group_name("unpublished before rejoin".into())
        .await?;
    b.sync().await?;
    let previous = b.local_commit_log().await?.last()?.commit_sequence_id as u64;
    a.readd_installations(vec![bo.context.installation_id().to_vec()])
        .await?;
    b.sync_with_conn().await?;
    bo.sync_welcomes().await?;
    a.update_group_name("after rejoin".into()).await?;
    b.sync().await?;
    publish_turn(&mut CommitLogWorker::new(bo.context.clone())).await;
    let entries = remote_entries(&bo, &b).await;
    assert!(!entries.is_empty());
    assert!(entries.iter().all(|entry| entry.author_installation_key
        == bo.context.installation_id().to_vec()
        && entry.commit_sequence_id > previous));
}

// verifies: FORK-087
#[xmtp_common::test(unwrap_try = true)]
async fn an_entry_ahead_of_the_reader_is_held() {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    tester!(caro, disable_workers);
    let a = alix
        .create_group_with_members(&[bo.inbox_id(), caro.inbox_id()], None, None)
        .await?;
    bo.sync_welcomes().await?;
    caro.sync_welcomes().await?;
    let b = bo.group(&a.group_id)?;
    let c = caro.group(&a.group_id)?;
    b.update_consent_state(ConsentState::Allowed)?;
    c.update_consent_state(ConsentState::Allowed)?;
    a.update_admin_list(
        crate::groups::UpdateAdminListType::AddSuper,
        bo.inbox_id().to_string(),
    )
    .await?;
    b.sync().await?;
    publish_turn(&mut CommitLogWorker::new(bo.context.clone())).await;
    let mut reader = CommitLogWorker::new(caro.context.clone());
    reader
        .run_test(CommitLogTestFunction::SaveRemoteCommitLog, None)
        .await?;
    assert!(
        caro.context
            .db()
            .get_latest_remote_log_for_group(&a.group_id)?
            .is_none()
    );
    assert_eq!(
        caro.context
            .db()
            .get_last_cursor(a.group_id, EntityKind::CommitLogDownload)?
            .0,
        0
    );
    c.sync().await?;
    reader
        .run_test(CommitLogTestFunction::SaveRemoteCommitLog, None)
        .await?;
    assert_eq!(
        caro.context
            .db()
            .get_latest_remote_log_for_group(&a.group_id)??
            .author_installation_key,
        bo.context.installation_id().to_vec()
    );
}

// verifies: FORK-087
#[xmtp_common::test(unwrap_try = true)]
async fn an_inactive_reader_does_not_hold() {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let a = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    bo.sync_welcomes().await?;
    let b = bo.group(&a.group_id)?;
    b.update_consent_state(ConsentState::Allowed)?;
    a.remove_members(&[bo.inbox_id()]).await?;
    b.sync_with_conn().await?;
    assert!(!b.is_active()?);
    a.update_group_name("after removal".into()).await?;
    publish_turn(&mut CommitLogWorker::new(alix.context.clone())).await;
    let topic = Topic::new_commit_log(keys_of(&a).commit_log_id());
    let entries = alix
        .context
        .api()
        .query_commit_log([(topic, Cursor(0))].into())
        .await?;
    let last = entries.last()?.cursor;
    CommitLogWorker::new(bo.context.clone())
        .run_test(CommitLogTestFunction::SaveRemoteCommitLog, None)
        .await?;
    assert_eq!(
        bo.context
            .db()
            .get_last_cursor(a.group_id, EntityKind::CommitLogDownload)?,
        last
    );
}

// verifies: FORK-030, FORK-087
#[xmtp_common::test(unwrap_try = true)]
async fn a_held_entry_stops_later_pages_of_its_topic() {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let a = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    bo.sync_welcomes().await?;
    let b = bo.group(&a.group_id)?;
    b.update_consent_state(ConsentState::Allowed)?;
    a.update_group_name("reader processed prefix".into())
        .await?;
    b.sync().await?;
    a.update_group_name("reader has not processed this commit".into())
        .await?;
    let future = a.local_commit_log().await?.last()?.commit_sequence_id as u64;
    let keys = keys_of(&a);
    let topic = Topic::new_commit_log(keys.commit_log_id());
    let entry = |sequence, epoch: u64, author: &xmtp_cryptography::XmtpInstallationCredential| {
        let mut entry = PlaintextCommitLogEntry {
            group_id: a.group_id.to_vec(),
            commit_sequence_id: sequence,
            commit_result: ProtoCommitResult::Applied as i32,
            last_epoch_authenticator: vec![epoch as u8; 32],
            applied_epoch_number: epoch + 1,
            applied_epoch_authenticator: vec![epoch as u8 + 1; 32],
            ..Default::default()
        };
        sign_entry_author(&keys, author, &mut entry).unwrap();
        seal_entry(&keys, &entry).unwrap()
    };
    let envelope_base = future + 100;
    let entries = [
        entry(1, 1, &alix.context.identity().installation_keys),
        entry(future, 2, &bo.context.identity().installation_keys),
        entry(2, 2, &alix.context.identity().installation_keys),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, entry)| {
        server_envelope(
            &topic,
            envelope_base + index as u64,
            Payload::SealedCommitLogEntry(entry),
        )
    })
    .collect();
    let (client, script) = scripted::scripted_with_page_size(&bo.client, 2).await;
    script.lock().reads.insert(topic.cloned_vec(), entries);
    let mut reader = CommitLogWorker::new(client.context.clone());
    reader
        .run_test(CommitLogTestFunction::SaveRemoteCommitLog, None)
        .await?;
    assert_eq!(
        script.lock().read.len(),
        1,
        "held topics must not request page two"
    );
    assert_eq!(
        bo.context
            .db()
            .get_last_cursor(b.group_id, EntityKind::CommitLogDownload)?
            .0,
        envelope_base
    );
    assert_eq!(
        bo.context
            .db()
            .get_latest_remote_log_for_group(&b.group_id)??
            .commit_sequence_id,
        1
    );
    b.sync().await?;
    reader
        .run_test(CommitLogTestFunction::SaveRemoteCommitLog, None)
        .await?;
    let accepted = bo.context.db().get_remote_commit_log_after_cursor(
        &b.group_id,
        0,
        RemoteCommitLogOrder::AscendingByRowid,
    )?;
    assert_eq!(
        accepted
            .iter()
            .map(|entry| entry.commit_sequence_id)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert_eq!(
        bo.context
            .db()
            .get_last_cursor(b.group_id, EntityKind::CommitLogDownload)?
            .0,
        envelope_base + 2
    );
}

/// A download stores each page and moves the cursor past it before it
/// requests the next one. A reader stopped after three pages keeps their
/// entries, and the next download starts with the fourth page and checks it
/// against the entries already stored.
// verifies: FORK-030, FORK-083
#[xmtp_common::test(unwrap_try = true)]
async fn commit_log_download_commits_each_page() {
    const ENTRIES: u64 = 16;
    const PAGE_ROWS: u32 = 2;
    const STORED_PAGES: usize = 3;
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let a = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    bo.sync_welcomes().await?;
    let b = bo.group(&a.group_id)?;
    b.update_consent_state(ConsentState::Allowed)?;
    // An active reader holds an entry for a commit it has not processed, so
    // it first processes more envelopes than there are entries.
    for _ in 0..ENTRIES {
        a.send_message_optimistic(b"padding", SendMessageOpts::default())?;
    }
    a.publish_messages().await?;
    b.sync().await?;
    let processed = bo
        .context
        .db()
        .latest_cursor_for_id(b.group_id, &[EntityKind::ApplicationMessage])?
        .0;
    assert!(processed >= ENTRIES);

    let keys = keys_of(&a);
    let topic = Topic::new_commit_log(keys.commit_log_id());
    let envelope_base = processed + 100;
    // The first envelope after the stored pages replays the last stored commit
    // with another result, which the chain rules discard.
    let stored = STORED_PAGES as u64 * u64::from(PAGE_ROWS);
    let mut commits: Vec<_> = (1..=ENTRIES)
        .map(|sequence| (sequence, sequence as u8 + 1))
        .collect();
    commits.insert(stored as usize, (stored, 0xee));
    let envelopes = commits.len() as u64;
    let entries = commits
        .into_iter()
        .zip(1..)
        .map(|((sequence, applied), position)| {
            let mut entry = PlaintextCommitLogEntry {
                group_id: a.group_id.to_vec(),
                commit_sequence_id: sequence,
                commit_result: ProtoCommitResult::Applied as i32,
                last_epoch_authenticator: vec![sequence as u8; 32],
                applied_epoch_number: sequence + 1,
                applied_epoch_authenticator: vec![applied; 32],
                ..Default::default()
            };
            sign_entry_author(
                &keys,
                &alix.context.identity().installation_keys,
                &mut entry,
            )
            .unwrap();
            server_envelope(
                &topic,
                envelope_base + position,
                Payload::SealedCommitLogEntry(seal_entry(&keys, &entry).unwrap()),
            )
        })
        .collect();
    let (client, script) = scripted::scripted_with_page_size(&bo.client, PAGE_ROWS).await;
    {
        let mut script = script.lock();
        script.reads.insert(topic.cloned_vec(), entries);
        script.stop_reads_after = Some(STORED_PAGES);
    }
    let accepted = || {
        bo.context
            .db()
            .get_remote_commit_log_after_cursor(
                &b.group_id,
                0,
                RemoteCommitLogOrder::AscendingByRowid,
            )
            .unwrap()
            .iter()
            .map(|entry| entry.commit_sequence_id as u64)
            .collect::<Vec<_>>()
    };
    let download_cursor = || {
        bo.context
            .db()
            .get_last_cursor(b.group_id, EntityKind::CommitLogDownload)
            .unwrap()
            .0
    };

    assert!(
        CommitLogWorker::new(client.context.clone())
            .run_test(CommitLogTestFunction::SaveRemoteCommitLog, None)
            .await
            .is_err()
    );
    assert_eq!(accepted(), (1..=stored).collect::<Vec<_>>());
    assert_eq!(download_cursor(), envelope_base + stored);

    {
        let mut script = script.lock();
        script.stop_reads_after = None;
        script.read.clear();
    }
    CommitLogWorker::new(client.context.clone())
        .run_test(CommitLogTestFunction::SaveRemoteCommitLog, None)
        .await?;
    assert_eq!(
        script.lock().read.len() as u64,
        (envelopes - stored).div_ceil(u64::from(PAGE_ROWS)),
        "the resumed download starts after the stored pages"
    );
    assert_eq!(accepted(), (1..=ENTRIES).collect::<Vec<_>>());
    assert_eq!(download_cursor(), envelope_base + envelopes);
}

// verifies: FORK-087, FORK-083, FORK-085
#[xmtp_common::test(unwrap_try = true)]
async fn a_publisher_handover_keeps_the_log_readable() {
    use crate::groups::UpdateAdminListType::{AddSuper, RemoveSuper};
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    tester!(caro, disable_workers);
    tester!(dan, disable_workers);
    let a = alix
        .create_group_with_members(
            &[bo.inbox_id(), caro.inbox_id(), dan.inbox_id()],
            None,
            None,
        )
        .await?;
    bo.sync_welcomes().await?;
    caro.sync_welcomes().await?;
    dan.sync_welcomes().await?;
    let b = bo.group(&a.group_id)?;
    let c = caro.group(&a.group_id)?;
    let d = dan.group(&a.group_id)?;
    for group in [&b, &c, &d] {
        group.update_consent_state(ConsentState::Allowed)?;
    }
    a.update_group_name("accepted old publisher prefix".into())
        .await?;
    c.sync().await?;
    publish_turn(&mut CommitLogWorker::new(alix.context.clone())).await;
    let mut reader = CommitLogWorker::new(caro.context.clone());
    reader
        .run_test(CommitLogTestFunction::SaveRemoteCommitLog, None)
        .await?;
    let last_old = caro
        .context
        .db()
        .get_latest_remote_log_for_group(&a.group_id)??
        .commit_sequence_id;
    // This unpublished entry leaves a gap before Bo's promotion.
    a.update_group_name("unpublished gap".into()).await?;
    a.update_admin_list(AddSuper, bo.inbox_id().to_string())
        .await?;
    b.sync().await?;
    let promotion = b.local_commit_log().await?.last()?.commit_sequence_id as u64;
    b.update_admin_list(RemoveSuper, alix.inbox_id().to_string())
        .await?;
    a.sync().await?;
    c.sync().await?;
    d.sync().await?;
    let mut demoted = PlaintextCommitLogEntry::from(a.local_commit_log().await?.last()?);
    let keys = keys_of(&a);
    sign_entry_author(
        &keys,
        &alix.context.identity().installation_keys,
        &mut demoted,
    )?;
    let mut stored = Vec::new();
    alix.context
        .api()
        .publish_commit_log(vec![seal_entry(&keys, &demoted)?], &mut stored)
        .await?;
    publish_turn(&mut CommitLogWorker::new(bo.context.clone())).await;
    let published = remote_entries(&bo, &b).await;
    let bo_entries = published
        .iter()
        .filter(|entry| entry.author_installation_key == bo.context.installation_id().to_vec())
        .collect::<Vec<_>>();
    assert!(
        !bo_entries.is_empty(),
        "the promoted publisher must publish"
    );
    assert!(
        bo_entries
            .iter()
            .all(|entry| entry.commit_sequence_id >= promotion),
        "the new publisher must not publish its earlier history"
    );
    assert!(
        bo_entries
            .iter()
            .any(|entry| entry.commit_sequence_id == promotion),
        "the promotion commit must be published"
    );
    for mut reader in [reader, CommitLogWorker::new(dan.context.clone())] {
        reader
            .run_test(CommitLogTestFunction::SaveRemoteCommitLog, None)
            .await?;
        reader
            .run_test(CommitLogTestFunction::CheckForkedState, None)
            .await?;
    }
    for client in [&caro, &dan] {
        let entries = client.context.db().get_remote_commit_log_after_cursor(
            &a.group_id,
            0,
            RemoteCommitLogOrder::AscendingByRowid,
        )?;
        assert!(entries.iter().any(|entry| entry.author_installation_key == bo.context.installation_id().to_vec()));
        assert!(entries.iter().filter(|entry| entry.commit_sequence_id > last_old)
            .all(|entry| entry.author_installation_key == bo.context.installation_id().to_vec()));
        assert_eq!(
            client
                .context
                .db()
                .get_group_commit_log_forked_status(&a.group_id)?,
            Some(false)
        );
    }
}

enum PublisherReturn {
    Role,
    Readd,
    Continuous,
}

async fn assert_returning_publisher_progresses(change: PublisherReturn) {
    use crate::groups::UpdateAdminListType::{AddSuper, RemoveSuper};
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    #[cfg(not(target_arch = "wasm32"))]
    tester!(caro, persistent_db, disable_workers);
    #[cfg(target_arch = "wasm32")]
    tester!(caro, disable_workers);
    tester!(dan, disable_workers);
    tester!(eve, disable_workers);
    let a = alix
        .create_group_with_members(
            &[bo.inbox_id(), caro.inbox_id(), dan.inbox_id()],
            None,
            None,
        )
        .await
        .unwrap();
    bo.sync_welcomes().await.unwrap();
    caro.sync_welcomes().await.unwrap();
    dan.sync_welcomes().await.unwrap();
    let b = bo.group(&a.group_id).unwrap();
    let c = caro.group(&a.group_id).unwrap();
    let d = dan.group(&a.group_id).unwrap();
    for group in [&b, &c, &d] {
        group.update_consent_state(ConsentState::Allowed).unwrap();
    }
    a.update_group_name("accepted prefix before publisher return".into())
        .await
        .unwrap();
    c.sync().await.unwrap();
    publish_turn(&mut CommitLogWorker::new(alix.context.clone())).await;
    CommitLogWorker::new(caro.context.clone())
        .run_test(CommitLogTestFunction::SaveRemoteCommitLog, None)
        .await
        .unwrap();
    let prefix = caro
        .context
        .db()
        .get_latest_remote_log_for_group(&a.group_id)
        .unwrap()
        .unwrap();
    assert_eq!(
        prefix.author_installation_key,
        alix.context.installation_id().to_vec()
    );
    a.update_group_name("unpublished gap before publisher return".into())
        .await
        .unwrap();
    let gap = a
        .local_commit_log()
        .await
        .unwrap()
        .last()
        .unwrap()
        .commit_sequence_id;
    a.update_admin_list(AddSuper, bo.inbox_id().to_string())
        .await
        .unwrap();
    b.sync().await.unwrap();
    match change {
        PublisherReturn::Readd => {
            b.readd_installations(vec![alix.context.installation_id().to_vec()])
                .await
                .unwrap();
            a.sync_with_conn().await.unwrap();
            alix.sync_welcomes().await.unwrap();
        }
        PublisherReturn::Role => {
            b.update_admin_list(RemoveSuper, alix.inbox_id().to_string())
                .await
                .unwrap();
            b.update_admin_list(AddSuper, alix.inbox_id().to_string())
                .await
                .unwrap();
        }
        PublisherReturn::Continuous => {
            a.update_admin_list(RemoveSuper, bo.inbox_id().to_string())
                .await
                .unwrap();
            a.update_admin_list(AddSuper, bo.inbox_id().to_string())
                .await
                .unwrap();
            b.sync().await.unwrap();
        }
    }
    let return_commit = b
        .local_commit_log()
        .await
        .unwrap()
        .last()
        .unwrap()
        .commit_sequence_id;
    // Eve joins after the transitions; her Welcome is her only boundary evidence.
    b.add_members(&[eve.inbox_id()]).await.unwrap();
    let join_commit = b
        .local_commit_log()
        .await
        .unwrap()
        .last()
        .unwrap()
        .commit_sequence_id;
    eve.sync_welcomes().await.unwrap();
    let e = eve.group(&a.group_id).unwrap();
    e.update_consent_state(ConsentState::Allowed).unwrap();
    b.update_group_name("latest commit after publisher return".into())
        .await
        .unwrap();
    a.sync().await.unwrap();
    c.sync().await.unwrap();
    d.sync().await.unwrap();
    e.sync().await.unwrap();
    let latest = c
        .local_commit_log()
        .await
        .unwrap()
        .last()
        .unwrap()
        .commit_sequence_id;
    publish_turn(&mut CommitLogWorker::new(bo.context.clone())).await;
    publish_turn(&mut CommitLogWorker::new(alix.context.clone())).await;
    if !matches!(change, PublisherReturn::Continuous) {
        assert!(
            remote_entries(&bo, &b)
                .await
                .iter()
                .all(|entry| entry.commit_sequence_id != gap as u64)
        );
    }
    // Reopen Caro's complete database, including the old accepted prefix, before
    // reading. A process-local boundary would be lost here.
    #[cfg(not(target_arch = "wasm32"))]
    let caro = {
        let path = match caro.context.store().opts() {
            xmtp_db::StorageOption::Persistent(path) => path.clone(),
            xmtp_db::StorageOption::Ephemeral => panic!("restart test needs a persistent database"),
        };
        drop((c, caro));
        super::test_commit_log_upgrade::reopen_commit_log_client(&path).await
    };
    #[cfg(target_arch = "wasm32")]
    let caro = caro.client;
    let dan = dan.client;
    let eve = eve.client;
    // Caro retains Alix's prefix; Dan downloads it after the return; Eve's
    // Welcome follows the return. All three must reach the same local tip.
    for (label, client) in [("returning", &caro), ("delayed", &dan), ("joined", &eve)] {
        let mut reader = CommitLogWorker::new(client.context.clone());
        reader
            .run_test(CommitLogTestFunction::SaveRemoteCommitLog, None)
            .await
            .unwrap();
        let tip = client
            .context
            .db()
            .get_latest_remote_log_for_group(&a.group_id)
            .unwrap()
            .unwrap();
        assert_eq!(
            tip.commit_sequence_id, latest,
            "{label}: publisher return must not strand the accepted chain"
        );
        let accepted = client
            .context
            .db()
            .get_remote_commit_log_after_cursor(
                &a.group_id,
                0,
                RemoteCommitLogOrder::AscendingByRowid,
            )
            .unwrap();
        if client.context.installation_id() == eve.context.installation_id() {
            assert!(
                accepted
                    .iter()
                    .any(|entry| entry.commit_sequence_id == join_commit),
                "a fresh reader must accept the entry at its Welcome's group-message anchor"
            );
        }
        if !matches!(change, PublisherReturn::Continuous) {
            assert!(
                accepted.iter().all(|entry| {
                    entry.commit_sequence_id <= prefix.commit_sequence_id
                        || entry.commit_sequence_id >= return_commit
                }),
                "a pre-boundary candidate must not restart the old chain"
            );
        } else if client.context.installation_id() != eve.context.installation_id() {
            assert!(
                accepted.iter().any(|entry| entry.commit_sequence_id == gap),
                "unrelated publisher churn must not reset a continuous author's chain"
            );
        }
        reader
            .run_test(CommitLogTestFunction::CheckForkedState, None)
            .await
            .unwrap();
        assert_eq!(
            client
                .context
                .db()
                .get_group_commit_log_forked_status(&a.group_id)
                .unwrap(),
            Some(false)
        );
    }
}

// verifies: FORK-083, FORK-089
#[xmtp_common::test(unwrap_try = true)]
async fn a_repromoted_publisher_keeps_old_and_new_readers_progressing() {
    assert_returning_publisher_progresses(PublisherReturn::Role).await;
}

// verifies: FORK-083, FORK-089
#[xmtp_common::test(unwrap_try = true)]
async fn a_readded_publisher_keeps_old_and_new_readers_progressing() {
    assert_returning_publisher_progresses(PublisherReturn::Readd).await;
}

// verifies: FORK-083
#[xmtp_common::test(unwrap_try = true)]
async fn a_continuous_publisher_keeps_its_chain_across_unrelated_role_changes() {
    assert_returning_publisher_progresses(PublisherReturn::Continuous).await;
}
