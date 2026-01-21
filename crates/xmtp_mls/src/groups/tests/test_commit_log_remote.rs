use crate::groups::MlsGroup;
use crate::groups::PolicySet;
use crate::groups::commit_log::{CommitLogTestFunction, CommitLogWorker};
use crate::groups::commit_log_key::{
    CommitLogKeyCrypto, CommitLogKeyStore, get_or_create_signing_key,
};
use crate::groups::send_message_opts::SendMessageOpts;
use crate::{context::XmtpSharedContext, tester};
use openmls::prelude::{OpenMlsCrypto, SignatureScheme};
use openmls_traits::OpenMlsProvider;
use prost::Message;
use rand::Rng;
use xmtp_configuration::Originators;
use xmtp_db::MlsProviderExt;
use xmtp_db::consent_record::ConsentState;
use xmtp_db::group::GroupQueryArgs;
use xmtp_db::group::{ConversationType, GroupMembershipState};
use xmtp_db::group_message::MsgQueryArgs;
use xmtp_db::local_commit_log::{CommitType, LocalCommitLog};
use xmtp_db::prelude::*;
use xmtp_db::remote_commit_log::CommitResult;
use xmtp_db::remote_commit_log::RemoteCommitLog;
use xmtp_mls_common::group::GroupMetadataOptions;
use xmtp_proto::mls_v1::{PublishCommitLogRequest, QueryCommitLogRequest};
use xmtp_proto::types::Cursor;
use xmtp_proto::xmtp::identity::associations::RecoverableEd25519Signature;
use xmtp_proto::xmtp::mls::message_contents::CommitLogEntry;
use xmtp_proto::xmtp::mls::message_contents::PlaintextCommitLogEntry;

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
        hex::encode(&group.group_id[0..4]),
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

#[xmtp_common::test(unwrap_try = true)]
async fn test_commit_log_signer_on_group_creation() {
    tester!(alix);
    tester!(bo);

    let a = alix
        .find_or_create_dm_by_inbox_id(bo.inbox_id(), None)
        .await?;
    let b = bo.sync_welcomes().await?.first()?.to_owned();
    let a_metadata = a.mutable_metadata()?;
    let b_metadata = b.mutable_metadata()?;
    let a_commit_log_signer = a_metadata.commit_log_signer();
    let b_commit_log_signer = b_metadata.commit_log_signer();

    assert!(a_commit_log_signer.is_some());
    assert!(b_commit_log_signer.is_some());
    assert_eq!(
        a_commit_log_signer.as_ref().unwrap().as_slice(),
        b_commit_log_signer.as_ref().unwrap().as_slice()
    );
    assert_eq!(
        a_commit_log_signer.unwrap().as_slice().len(),
        xmtp_cryptography::configuration::ED25519_KEY_LENGTH
    );

    let a = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let b = bo.sync_welcomes().await?.first()?.to_owned();
    let a_metadata = a.mutable_metadata()?;
    let b_metadata = b.mutable_metadata()?;
    let a_commit_log_signer = a_metadata.commit_log_signer();
    let b_commit_log_signer = b_metadata.commit_log_signer();

    assert!(a_commit_log_signer.is_some());
    assert!(b_commit_log_signer.is_some());
    assert_eq!(
        a_commit_log_signer.as_ref().unwrap().as_slice(),
        b_commit_log_signer.as_ref().unwrap().as_slice()
    );
    assert_eq!(
        a_commit_log_signer.unwrap().as_slice().len(),
        xmtp_cryptography::configuration::ED25519_KEY_LENGTH
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_device_sync_mutable_metadata_is_overwritten() {
    tester!(alix);
    tester!(bo);

    let a = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    // Pretend that Bo received the group via device sync
    // Currently, device sync creates a placeholder OpenMLS group with its own commit log secret
    MlsGroup::insert(
        &bo.context,
        Some(&a.group_id),
        GroupMembershipState::Restored,
        ConversationType::Group,
        PolicySet::default(),
        GroupMetadataOptions {
            ..Default::default()
        },
        None,
    )?;
    let b = bo.group(&a.group_id)?;
    let a_metadata = a.mutable_metadata()?;
    let b_metadata = b.mutable_metadata()?;
    let a_commit_log_signer = a_metadata.commit_log_signer();
    let b_commit_log_signer = b_metadata.commit_log_signer();
    assert_ne!(
        a_commit_log_signer.as_ref().map(|s| s.as_slice()),
        b_commit_log_signer.as_ref().map(|s| s.as_slice())
    );

    let b = bo.sync_welcomes().await?.first()?.to_owned();
    let b_metadata = b.mutable_metadata()?;
    let b_commit_log_signer = b_metadata.commit_log_signer();
    assert_eq!(
        a_commit_log_signer.as_ref().unwrap().as_slice(),
        b_commit_log_signer.as_ref().unwrap().as_slice()
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_commit_log_publish_and_query_apis() {
    use openmls::prelude::{OpenMlsCrypto, SignatureScheme};
    use xmtp_proto::xmtp::identity::associations::RecoverableEd25519Signature;

    tester!(alix);

    // There is no way to clear commit log on the local node between tests, so we'll just write to
    // a new random group_id for each test iteration in case local node state has not been cleared

    // Generate a random 20-byte group_id for this test
    let group_id: Vec<u8> = (0..20)
        .map(|_| rand::thread_rng().gen_range(0..=255))
        .collect();

    // Test publishing commit log
    let commit_log_entry = PlaintextCommitLogEntry {
        group_id: group_id.clone(),
        commit_sequence_id: 123,
        last_epoch_authenticator: vec![5, 6, 7, 8],
        commit_result: 1, // Success
        applied_epoch_number: 456,
        applied_epoch_authenticator: vec![9, 10, 11, 12],
    };

    // Sign the commit log entry since backend now requires signatures
    let provider = alix.context.mls_provider();
    let crypto = provider.crypto();

    // Generate a signing key for this test
    let (private_key_bytes, _) = crypto.signature_key_gen(SignatureScheme::ED25519)?;
    let private_key = xmtp_cryptography::Secret::new(private_key_bytes.clone());
    let public_key = xmtp_cryptography::signature::to_public_key(&private_key)?.to_vec();

    // Sign the serialized entry
    let serialized_entry = commit_log_entry.clone().encode_to_vec();
    let signature = crypto.sign(
        SignatureScheme::ED25519,
        &serialized_entry,
        &private_key_bytes,
    )?;

    let result = alix
        .context
        .api()
        .publish_commit_log(vec![PublishCommitLogRequest {
            group_id: group_id.clone(),
            serialized_commit_log_entry: serialized_entry,
            signature: Some(RecoverableEd25519Signature {
                bytes: signature,
                public_key: public_key.clone(),
            }),
        }])
        .await;
    assert!(result.is_ok());

    // Test querying commit log
    let query = QueryCommitLogRequest {
        group_id: group_id.clone(),
        ..Default::default()
    };

    let query_result = alix.context.api().query_commit_log(vec![query]).await;
    assert!(query_result.is_ok());

    // Extract the entries from the response
    let response = query_result.unwrap();
    assert_eq!(response.len(), 1);
    assert_eq!(response[0].commit_log_entries.len(), 1);

    let returned_entry = &response[0].commit_log_entries[0];
    let raw_bytes = &returned_entry.serialized_commit_log_entry;

    // Verify the backend preserved the signature
    assert!(
        returned_entry.signature.is_some(),
        "Backend should preserve signature"
    );
    let sig = returned_entry.signature.as_ref().unwrap();
    assert_eq!(sig.public_key, public_key, "Public key should match");

    // TODO(cvoell): this will require decryption once encrypted key is added
    let entry = PlaintextCommitLogEntry::decode(raw_bytes.as_slice()).unwrap();
    assert_eq!(entry.group_id, commit_log_entry.group_id);
    assert_eq!(
        entry.commit_sequence_id,
        commit_log_entry.commit_sequence_id
    );
    assert_eq!(
        entry.last_epoch_authenticator,
        commit_log_entry.last_epoch_authenticator
    );
    assert_eq!(entry.commit_result, commit_log_entry.commit_result);
    assert_eq!(
        entry.applied_epoch_number,
        commit_log_entry.applied_epoch_number
    );
    assert_eq!(
        entry.applied_epoch_authenticator,
        commit_log_entry.applied_epoch_authenticator
    );
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
async fn test_publish_commit_log_to_remote() {
    // Disable background CommitLogWorker for deterministic testing
    tester!(alix, with_commit_log_worker: false);
    tester!(bo);

    // Alix creates a group with Bo
    let alix_group = alix.create_group(None, None).unwrap();
    alix_group.add_members(&[bo.inbox_id()]).await.unwrap();
    bo.sync_all_welcomes_and_groups(None).await.unwrap();

    let binding = bo.list_conversations(GroupQueryArgs::default()).unwrap();
    let bo_group = binding.first().unwrap();
    assert_eq!(bo_group.group.group_id, alix_group.group_id);

    // Alix has two local commit log entry
    let commit_log_entries = alix
        .context
        .db()
        .get_group_logs(&alix_group.group_id)
        .unwrap();
    assert_eq!(commit_log_entries.len(), 2);

    // Since Alix has never written to the remote commit log, the last cursor should be 0
    let published_commit_log_cursor = alix
        .context
        .db()
        .get_last_cursor_for_originator(
            &alix_group.group_id,
            xmtp_db::refresh_state::EntityKind::CommitLogUpload,
            Originators::REMOTE_COMMIT_LOG,
        )
        .unwrap();
    assert_eq!(published_commit_log_cursor, Cursor::commit_log(0));

    // Alix runs the commit log worker, which will publish the commit log entry to the remote commit log
    let mut commit_log_worker = CommitLogWorker::new(alix.context.clone());
    let result = commit_log_worker
        .run_test(CommitLogTestFunction::PublishCommitLogsToRemote, Some(1))
        .await;
    assert!(result.is_ok());

    let published_commit_log_cursor = alix
        .context
        .db()
        .get_last_cursor_for_originator(
            &alix_group.group_id,
            xmtp_db::refresh_state::EntityKind::CommitLogUpload,
            Originators::REMOTE_COMMIT_LOG,
        )
        .unwrap();
    tracing::info!("{}", published_commit_log_cursor);
    assert!(published_commit_log_cursor > Cursor::commit_log(0));
    let last_commit_log_entry = commit_log_entries.last().unwrap();
    // Verify that the local cursor has now been updated to the last commit log entry's sequence id
    assert_eq!(
        Cursor::commit_log(last_commit_log_entry.rowid as u64),
        published_commit_log_cursor
    );

    // Query the remote commit log to make sure it matches the local commit log entry
    let query = QueryCommitLogRequest {
        group_id: alix_group.group_id.clone(),
        ..Default::default()
    };

    let query_result = alix.context.api().query_commit_log(vec![query]).await;
    assert!(query_result.is_ok());

    // Extract the entries from the response
    let response = query_result.unwrap();
    assert_eq!(response.len(), 1);
    assert_eq!(response[0].commit_log_entries.len(), 1);
    let raw_bytes = &response[0].commit_log_entries[0].serialized_commit_log_entry;

    // TODO: this will require decryption once encrypted key is added
    let entry = PlaintextCommitLogEntry::decode(raw_bytes.as_slice()).unwrap();
    assert_eq!(
        entry.commit_sequence_id,
        last_commit_log_entry.commit_sequence_id as u64
    );
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
        .get_last_cursor_for_originator(
            &alix_group.group_id,
            xmtp_db::refresh_state::EntityKind::CommitLogUpload,
            Originators::REMOTE_COMMIT_LOG,
        )
        .unwrap();
    assert_eq!(alix_group_1_cursor, Cursor::commit_log(0));

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
        test_results[0].publish_commit_log_results.clone().unwrap()[0].conversation_id,
        alix_group.group_id
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
        .get_last_cursor_for_originator(
            &alix_group.group_id,
            xmtp_db::refresh_state::EntityKind::CommitLogUpload,
            Originators::REMOTE_COMMIT_LOG,
        )
        .unwrap();

    let alix_group1_publish_result_upload_cursor =
        test_results[0].publish_commit_log_results.clone().unwrap()[0].last_entry_published_rowid;
    assert_eq!(
        alix_group_1_cursor,
        Cursor::new(
            alix_group1_publish_result_upload_cursor as u64,
            Originators::REMOTE_COMMIT_LOG
        )
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
            .get(&alix_group.group_id)
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
            .get(&alix_group.group_id)
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
            .conversation_id,
        alix_group.group_id
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
            .get(&alix_group.group_id)
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
            .get(&bo_group.group_id)
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
            .get(&bo_group.group_id)
            .unwrap(),
        4
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_should_skip_remote_log_entry() {
    // Disable background CommitLogWorker for deterministic testing
    tester!(alix, with_commit_log_worker: false);
    let commit_log_worker = CommitLogWorker::new(alix.context.clone());

    // Generate a signing key for the test
    let provider = alix.context.mls_provider();
    let crypto = provider.crypto();
    let (private_key_bytes, _) = crypto.signature_key_gen(SignatureScheme::ED25519)?;
    let private_key = xmtp_cryptography::Secret::new(private_key_bytes.clone());
    let public_key = xmtp_cryptography::signature::to_public_key(&private_key)?.to_vec();

    // Helper function to create a signed CommitLogEntry
    let create_signed_entry =
        |entry: &PlaintextCommitLogEntry| -> Result<CommitLogEntry, Box<dyn std::error::Error>> {
            let serialized_entry = entry.encode_to_vec();
            let signature = crypto.sign(
                SignatureScheme::ED25519,
                &serialized_entry,
                &private_key_bytes,
            )?;

            Ok(CommitLogEntry {
                sequence_id: 1, // This can be any value for the test
                serialized_commit_log_entry: serialized_entry,
                signature: Some(RecoverableEd25519Signature {
                    bytes: signature,
                    public_key: public_key.clone(),
                }),
            })
        };

    // Does not skip if entry meets all conditions
    let latest_saved_remote_log = RemoteCommitLog {
        rowid: 0,
        log_sequence_id: 0,
        group_id: vec![0x11, 0x22, 0x33],
        commit_sequence_id: 100,
        commit_result: CommitResult::Success,
        applied_epoch_number: 3,
        applied_epoch_authenticator: vec![0x01, 0x02, 0x03],
    };

    let entry = PlaintextCommitLogEntry {
        group_id: vec![0x11, 0x22, 0x33],
        commit_sequence_id: 101,
        last_epoch_authenticator: vec![0x01, 0x02, 0x03],
        commit_result: 1,
        applied_epoch_number: 4,
        applied_epoch_authenticator: vec![0x01, 0x02, 0x04],
    };
    let signed_entry = create_signed_entry(&entry)?;
    assert!(!commit_log_worker._should_skip_remote_commit_log_entry(
        &[0x11, 0x22, 0x33],
        Some(latest_saved_remote_log.clone()),
        &signed_entry,
        &entry,
        &public_key,
    ));

    // Skips if Group ID does not match
    let latest_saved_remote_log = RemoteCommitLog {
        rowid: 0,
        log_sequence_id: 0,
        group_id: vec![0x11, 0x22, 0x33],
        commit_sequence_id: 100,
        commit_result: CommitResult::Success,
        applied_epoch_number: 3,
        applied_epoch_authenticator: vec![0x01, 0x02, 0x03],
    };
    let entry = PlaintextCommitLogEntry {
        group_id: vec![0xff, 0x22, 0x33],
        commit_sequence_id: 101,
        last_epoch_authenticator: vec![0x01, 0x02, 0x03],
        commit_result: 1,
        applied_epoch_number: 4,
        applied_epoch_authenticator: vec![0x01, 0x02, 0x04],
    };
    let signed_entry = create_signed_entry(&entry)?;
    assert!(commit_log_worker._should_skip_remote_commit_log_entry(
        &[0x11, 0x22, 0x33],
        Some(latest_saved_remote_log),
        &signed_entry,
        &entry,
        &public_key,
    ));

    // Skips if commit_sequence_id of the entry is not greater than the most recently stored entry, if one exists.
    let latest_saved_remote_log = RemoteCommitLog {
        rowid: 0,
        log_sequence_id: 0,
        group_id: vec![0x11, 0x22, 0x33],
        commit_sequence_id: 100,
        commit_result: CommitResult::Success,
        applied_epoch_number: 3,
        applied_epoch_authenticator: vec![0x01, 0x02, 0x03],
    };
    let entry = PlaintextCommitLogEntry {
        group_id: vec![0x11, 0x22, 0x33],
        commit_sequence_id: 99,
        last_epoch_authenticator: vec![0x01, 0x02, 0x03],
        commit_result: 1,
        applied_epoch_number: 4,
        applied_epoch_authenticator: vec![0x01, 0x02, 0x04],
    };
    let signed_entry = create_signed_entry(&entry)?;
    assert!(commit_log_worker._should_skip_remote_commit_log_entry(
        &[0x11, 0x22, 0x33],
        Some(latest_saved_remote_log),
        &signed_entry,
        &entry,
        &public_key,
    ));

    // Skips if the last_epoch_authenticator does not match the epoch_authenticator of
    // the most recently stored entry with a CommitResult of COMMIT_RESULT_APPLIED, if one exists.
    let latest_saved_remote_log = RemoteCommitLog {
        rowid: 0,
        log_sequence_id: 0,
        group_id: vec![0x11, 0x22, 0x33],
        commit_sequence_id: 100,
        commit_result: CommitResult::Success,
        applied_epoch_number: 3,
        applied_epoch_authenticator: vec![0x01, 0x02, 0x03],
    };
    let entry = PlaintextCommitLogEntry {
        group_id: vec![0x11, 0x22, 0x33],
        commit_sequence_id: 101,
        last_epoch_authenticator: vec![0x01, 0x02, 0x05],
        commit_result: 1,
        applied_epoch_number: 4,
        applied_epoch_authenticator: vec![0x01, 0x02, 0x04],
    };
    let signed_entry = create_signed_entry(&entry)?;
    assert!(commit_log_worker._should_skip_remote_commit_log_entry(
        &[0x11, 0x22, 0x33],
        Some(latest_saved_remote_log),
        &signed_entry,
        &entry,
        &public_key,
    ));

    // Skips if the applied_epoch_number of the entry is not exactly 1 greater than
    // the latest_applied_epoch_number of the remote validation info. (skipped from 3 to 5)
    let latest_saved_remote_log = RemoteCommitLog {
        rowid: 0,
        log_sequence_id: 0,
        group_id: vec![0x11, 0x22, 0x33],
        commit_sequence_id: 100,
        commit_result: CommitResult::Success,
        applied_epoch_number: 3,
        applied_epoch_authenticator: vec![0x01, 0x02, 0x03],
    };
    let entry = PlaintextCommitLogEntry {
        group_id: vec![0x11, 0x22, 0x33],
        commit_sequence_id: 101,
        last_epoch_authenticator: vec![0x01, 0x02, 0x03],
        commit_result: 1,
        applied_epoch_number: 5,
        applied_epoch_authenticator: vec![0x01, 0x02, 0x04],
    };
    let signed_entry = create_signed_entry(&entry)?;
    assert!(commit_log_worker._should_skip_remote_commit_log_entry(
        &[0x11, 0x22, 0x33],
        Some(latest_saved_remote_log),
        &signed_entry,
        &entry,
        &public_key,
    ));

    // Skips if the applied_epoch_number of the entry is not exactly 1 greater than
    // the latest_applied_epoch_number of the remote validation info. (stayed at 3)
    let latest_saved_remote_log = RemoteCommitLog {
        rowid: 0,
        log_sequence_id: 0,
        group_id: vec![0x11, 0x22, 0x33],
        commit_sequence_id: 100,
        commit_result: CommitResult::Success,
        applied_epoch_number: 3,
        applied_epoch_authenticator: vec![0x01, 0x02, 0x03],
    };
    let entry = PlaintextCommitLogEntry {
        group_id: vec![0x11, 0x22, 0x33],
        commit_sequence_id: 101,
        last_epoch_authenticator: vec![0x01, 0x02, 0x03],
        commit_result: 1,
        applied_epoch_number: 3,
        applied_epoch_authenticator: vec![0x01, 0x02, 0x04],
    };
    let signed_entry = create_signed_entry(&entry)?;
    assert!(commit_log_worker._should_skip_remote_commit_log_entry(
        &[0x11, 0x22, 0x33],
        Some(latest_saved_remote_log),
        &signed_entry,
        &entry,
        &public_key,
    ));

    // Skips if the applied_epoch_number of the entry is not exactly 1 greater than
    // the latest_applied_epoch_number of the remote validation info. (decreased from 3 to 2)
    let latest_saved_remote_log = RemoteCommitLog {
        rowid: 0,
        log_sequence_id: 0,
        group_id: vec![0x11, 0x22, 0x33],
        commit_sequence_id: 100,
        commit_result: CommitResult::Success,
        applied_epoch_number: 3,
        applied_epoch_authenticator: vec![0x01, 0x02, 0x03],
    };
    let entry = PlaintextCommitLogEntry {
        group_id: vec![0x11, 0x22, 0x33],
        commit_sequence_id: 101,
        last_epoch_authenticator: vec![0x01, 0x02, 0x03],
        commit_result: 1,
        applied_epoch_number: 2,
        applied_epoch_authenticator: vec![0x01, 0x02, 0x04],
    };
    let signed_entry = create_signed_entry(&entry)?;
    assert!(commit_log_worker._should_skip_remote_commit_log_entry(
        &[0x11, 0x22, 0x33],
        Some(latest_saved_remote_log),
        &signed_entry,
        &entry,
        &public_key,
    ));

    // Skips if entry CommitResult is not COMMIT_RESULT_APPLIED, and the epoch authenticator or epoch number does not match the most recently applied values
    let latest_saved_remote_log = RemoteCommitLog {
        rowid: 0,
        log_sequence_id: 0,
        group_id: vec![0x11, 0x22, 0x33],
        commit_sequence_id: 100,
        commit_result: CommitResult::Success,
        applied_epoch_number: 3,
        applied_epoch_authenticator: vec![0x01, 0x02, 0x03],
    };
    let entry = PlaintextCommitLogEntry {
        group_id: vec![0x11, 0x22, 0x33],
        commit_sequence_id: 101,
        last_epoch_authenticator: vec![0x01, 0x02, 0x03],
        commit_result: 2,
        applied_epoch_number: 4,
        applied_epoch_authenticator: vec![0x01, 0x02, 0x03],
    };
    let signed_entry = create_signed_entry(&entry)?;
    assert!(commit_log_worker._should_skip_remote_commit_log_entry(
        &[0x11, 0x22, 0x33],
        Some(latest_saved_remote_log.clone()),
        &signed_entry,
        &entry,
        &public_key,
    ));
    let entry = PlaintextCommitLogEntry {
        group_id: vec![0x11, 0x22, 0x33],
        commit_sequence_id: 101,
        last_epoch_authenticator: vec![0x01, 0x02, 0x03],
        commit_result: 2,
        applied_epoch_number: 3,
        applied_epoch_authenticator: vec![0x01, 0x02, 0x04],
    };
    let signed_entry = create_signed_entry(&entry)?;
    assert!(commit_log_worker._should_skip_remote_commit_log_entry(
        &[0x11, 0x22, 0x33],
        Some(latest_saved_remote_log),
        &signed_entry,
        &entry,
        &public_key,
    ));
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_all_users_use_same_signing_key_for_publishing() {
    tester!(alix);
    tester!(bo);

    // Create a DM between alix and bo
    let alix_dm = alix
        .find_or_create_dm_by_inbox_id(bo.inbox_id(), None)
        .await?;
    let bo_dm = bo.sync_welcomes().await?.first()?.to_owned();

    // Both parties make commits to generate entries for publishing
    // Alix's first message should trigger a KeyUpdate commit (key rotation) first
    alix_dm
        .send_message("Hello from alix".as_bytes(), SendMessageOpts::default())
        .await?;
    bo_dm.sync().await?;
    let messages = bo_dm.find_messages(&MsgQueryArgs::default())?;
    // Should see 2 messages:
    // 1. System "group_updated" message (from UpdateGroupMembership commit)
    // 2. The actual "Hello from alix" application message
    assert_eq!(messages.len(), 2);
    // The last message should be our application message
    assert_eq!(messages[1].decrypted_message_bytes, b"Hello from alix");

    // Check alix's commit log - should have exact sequence: GroupCreation, UpdateGroupMembership
    // Note: KeyUpdate doesn't happen for first message in DMs apparently
    let alix_logs = alix_dm.local_commit_log().await?;
    print_commit_log_with_types(&alix_dm).await;
    assert_commit_sequence(
        &alix_logs,
        &[CommitType::GroupCreation, CommitType::UpdateGroupMembership],
    );

    // Bo's first message should also trigger a KeyUpdate commit (key rotation) first
    bo_dm
        .send_message("Hello from bo".as_bytes(), SendMessageOpts::default())
        .await?;
    alix_dm.sync().await?;
    let messages = alix_dm.find_messages(&MsgQueryArgs::default())?;
    // Should now have 3 messages: group_updated, "Hello from alix", "Hello from bo"
    assert_eq!(messages.len(), 3);
    assert_eq!(messages[2].decrypted_message_bytes, b"Hello from bo");

    // Check bo's commit log - should have exact sequence: Welcome, KeyUpdate
    let bo_logs = bo_dm.local_commit_log().await?;
    print_commit_log_with_types(&bo_dm).await;
    assert_commit_sequence(&bo_logs, &[CommitType::Welcome, CommitType::KeyUpdate]);

    // Get the signing keys that would be used by both parties for publishing
    let alix_conn = &alix.context.db();
    let bo_conn = &bo.context.db();

    let alix_conversation_keys = alix_conn.get_conversation_ids_for_remote_log_publish()?;
    let bo_conversation_keys = bo_conn.get_conversation_ids_for_remote_log_publish()?;

    // Find the DM conversation key for each party
    let alix_dm_key = alix_conversation_keys
        .iter()
        .find(|k| k.id == alix_dm.group_id)
        .expect("Alix should have DM key");
    let bo_dm_key = bo_conversation_keys
        .iter()
        .find(|k| k.id == bo_dm.group_id)
        .expect("Bo should have DM key");

    // Get the signing keys that would be used for publishing
    let alix_signing_key = get_or_create_signing_key(&alix.context, alix_dm_key)?
        .expect("Alix should have signing key");
    let bo_signing_key =
        get_or_create_signing_key(&bo.context, bo_dm_key)?.expect("Bo should have signing key");

    // Derive public keys from the private keys
    let alix_public_key = xmtp_cryptography::signature::to_public_key(&alix_signing_key)?;
    let bo_public_key = xmtp_cryptography::signature::to_public_key(&bo_signing_key)?;

    // Both parties should use the same signing key (same public key)
    assert_eq!(
        alix_public_key, bo_public_key,
        "Both parties in DM should use the same signing key for publishing commits"
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_consecutive_entries_verification_happy_case() {
    // Disable background CommitLogWorker for deterministic testing
    tester!(alix, with_commit_log_worker: false);
    tester!(bo, with_commit_log_worker: false);

    // Create a DM between alix and bo
    let alix_dm = alix
        .find_or_create_dm_by_inbox_id(bo.inbox_id(), None)
        .await?;
    let bo_dm = bo.sync_welcomes().await?.first()?.to_owned();

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
    assert_eq!(alix_publish_result[0].conversation_id, alix_dm.group_id);
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
    assert!(bo_download_result.contains_key(&bo_dm.group_id));
    assert_eq!(
        bo_download_result[&bo_dm.group_id], 1,
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
    assert_eq!(bo_publish_result[0].conversation_id, bo_dm.group_id);
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
    assert!(alix_download_result.contains_key(&alix_dm.group_id));
    assert_eq!(
        alix_download_result[&alix_dm.group_id], 2,
        "Alix should download 2 entries total: her original UpdateGroupMembership + Bo's new KeyUpdate"
    );

    // Verify that both parties can successfully verify consecutive entries published by each other
    // This demonstrates that the consensus public key derived from the first entry works for all subsequent entries
}

/// Test that bad signatures are properly rejected during commit log verification.
///
/// Covers two scenarios:
/// 1. Entry where signature doesn't match the claimed public key (should fail)
/// 2. Entry with valid signature but different public key than consensus (should fail against consensus key)
#[xmtp_common::test(unwrap_try = true)]
async fn test_bad_signature_handling() {
    use crate::groups::commit_log_key::CommitLogKeyCrypto;
    use openmls::prelude::OpenMlsCrypto;
    use openmls_traits::OpenMlsProvider;
    use prost::Message;
    use xmtp_proto::xmtp::identity::associations::RecoverableEd25519Signature;
    use xmtp_proto::xmtp::mls::message_contents::CommitLogEntry as CommitLogEntryProto;
    use xmtp_proto::xmtp::mls::message_contents::PlaintextCommitLogEntry;

    tester!(alice);
    tester!(bob);

    // Create a group with a member to get an UpdateGroupMembership commit
    let alice_group = alice
        .create_group_with_members(&[bob.inbox_id()], None, None)
        .await?;
    let alice_logs = alice_group.local_commit_log().await?;
    let valid_entry = PlaintextCommitLogEntry::from(&alice_logs[1]); // Use UpdateGroupMembership entry

    // Set up crypto provider and keys
    let provider = alice.context.mls_provider();
    let valid_signing_key = provider.crypto().generate_commit_log_key()?;
    let valid_public_key = xmtp_cryptography::signature::to_public_key(&valid_signing_key)?;
    let bad_signing_key = provider.crypto().generate_commit_log_key()?;
    let bad_public_key = xmtp_cryptography::signature::to_public_key(&bad_signing_key)?;

    let serialized_entry = valid_entry.encode_to_vec();

    // Test Case 1: Bad signature on first entry - public key doesn't match signature
    let bad_signature = provider.crypto().sign(
        openmls::prelude::SignatureScheme::ED25519,
        &serialized_entry,
        bad_signing_key.as_slice(),
    )?;

    let bad_entry = CommitLogEntryProto {
        sequence_id: 1,
        serialized_commit_log_entry: serialized_entry.clone(),
        signature: Some(RecoverableEd25519Signature {
            bytes: bad_signature,
            public_key: valid_public_key.to_vec(), // Wrong! Signature made with bad_signing_key but claims valid_public_key
        }),
    };

    // This should fail - signature doesn't match the claimed public key
    let bad_verification = provider
        .crypto()
        .verify_commit_log_signature(&bad_entry, &valid_public_key);
    assert!(
        bad_verification.is_err(),
        "Entry with mismatched signature should fail verification"
    );

    // Test Case 2: Valid signature but wrong public key for consensus
    let valid_signature = provider.crypto().sign(
        openmls::prelude::SignatureScheme::ED25519,
        &serialized_entry,
        valid_signing_key.as_slice(),
    )?;

    let valid_entry_with_correct_key = CommitLogEntryProto {
        sequence_id: 1,
        serialized_commit_log_entry: serialized_entry.clone(),
        signature: Some(RecoverableEd25519Signature {
            bytes: valid_signature.clone(),
            public_key: valid_public_key.to_vec(),
        }),
    };

    let valid_entry_with_wrong_key = CommitLogEntryProto {
        sequence_id: 2,
        serialized_commit_log_entry: serialized_entry,
        signature: Some(RecoverableEd25519Signature {
            bytes: provider.crypto().sign(
                openmls::prelude::SignatureScheme::ED25519,
                &valid_entry.encode_to_vec(),
                bad_signing_key.as_slice(),
            )?,
            public_key: bad_public_key.to_vec(), // This is the correct public key for the signature
        }),
    };

    // First entry should verify against its own public key
    let first_verification = provider
        .crypto()
        .verify_commit_log_signature(&valid_entry_with_correct_key, &valid_public_key);
    assert!(
        first_verification.is_ok(),
        "Valid entry should verify against correct public key"
    );

    // Second entry should fail when verified against the consensus public key (first entry's key)
    let second_verification = provider.crypto().verify_commit_log_signature(
        &valid_entry_with_wrong_key,
        &valid_public_key, // Using consensus key from first entry
    );
    assert!(
        second_verification.is_err(),
        "Entry with different public key should fail verification against consensus key"
    );

    // But second entry should pass when verified against its own public key
    let second_verification_correct = provider.crypto().verify_commit_log_signature(
        &valid_entry_with_wrong_key,
        &bad_public_key, // Using the entry's actual public key
    );
    assert!(
        second_verification_correct.is_ok(),
        "Entry should verify against its own public key"
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_update_commit_log_signer_sync_across_parties() {
    tester!(alix);
    tester!(bo);
    tester!(charlie);

    println!("Creating group with alix, bo, and charlie");
    let alix_group = alix
        .create_group_with_members(&[bo.inbox_id(), charlie.inbox_id()], None, None)
        .await
        .unwrap();

    // Bo and charlie sync welcomes to join the group
    let bo_welcomes = bo.sync_welcomes().await.unwrap();
    assert_eq!(bo_welcomes.len(), 1);
    let bo_group = bo_welcomes[0].clone();

    let charlie_welcomes = charlie.sync_welcomes().await.unwrap();
    assert_eq!(charlie_welcomes.len(), 1);
    let charlie_group = charlie_welcomes[0].clone();

    // Get initial commit log signer for all parties
    let initial_alix_signer = alix_group
        .mutable_metadata()
        .unwrap()
        .commit_log_signer()
        .unwrap();
    let initial_bo_signer = bo_group
        .mutable_metadata()
        .unwrap()
        .commit_log_signer()
        .unwrap();
    let initial_charlie_signer = charlie_group
        .mutable_metadata()
        .unwrap()
        .commit_log_signer()
        .unwrap();

    // All parties should have the same initial signer
    assert_eq!(initial_alix_signer.as_slice(), initial_bo_signer.as_slice());
    assert_eq!(
        initial_alix_signer.as_slice(),
        initial_charlie_signer.as_slice()
    );
    println!("✓ All parties have the same initial commit log signer");

    // Alix (super admin) updates the commit log signer
    let new_signer = alix
        .context
        .mls_provider()
        .crypto()
        .generate_commit_log_key()
        .unwrap();
    println!("Alix updating commit log signer...");
    alix_group
        .update_commit_log_signer(new_signer.clone())
        .await
        .unwrap();

    // Alix should see the new signer immediately
    let alix_updated_signer = alix_group
        .mutable_metadata()
        .unwrap()
        .commit_log_signer()
        .unwrap();
    assert_eq!(alix_updated_signer.as_slice(), new_signer.as_slice());
    assert_ne!(
        alix_updated_signer.as_slice(),
        initial_alix_signer.as_slice()
    );
    println!("✓ Alix sees the updated commit log signer immediately");

    // Bo and charlie shouldn't see the change yet (before sync)
    let bo_pre_sync_signer = bo_group
        .mutable_metadata()
        .unwrap()
        .commit_log_signer()
        .unwrap();
    let charlie_pre_sync_signer = charlie_group
        .mutable_metadata()
        .unwrap()
        .commit_log_signer()
        .unwrap();
    assert_eq!(
        bo_pre_sync_signer.as_slice(),
        initial_alix_signer.as_slice()
    );
    assert_eq!(
        charlie_pre_sync_signer.as_slice(),
        initial_alix_signer.as_slice()
    );
    println!("✓ Bo and Charlie still have the old signer before sync");

    // Now everyone syncs
    println!("All parties syncing...");
    alix_group.sync_with_conn().await.unwrap();
    bo_group.sync_with_conn().await.unwrap();
    charlie_group.sync_with_conn().await.unwrap();

    // After sync, everyone should see the new signer
    let final_alix_signer = alix_group
        .mutable_metadata()
        .unwrap()
        .commit_log_signer()
        .unwrap();
    let final_bo_signer = bo_group
        .mutable_metadata()
        .unwrap()
        .commit_log_signer()
        .unwrap();
    let final_charlie_signer = charlie_group
        .mutable_metadata()
        .unwrap()
        .commit_log_signer()
        .unwrap();

    // All parties should now have the new signer
    assert_eq!(final_alix_signer.as_slice(), new_signer.as_slice());
    assert_eq!(final_bo_signer.as_slice(), new_signer.as_slice());
    assert_eq!(final_charlie_signer.as_slice(), new_signer.as_slice());

    // None should have the old signer
    assert_ne!(final_alix_signer.as_slice(), initial_alix_signer.as_slice());
    assert_ne!(final_bo_signer.as_slice(), initial_alix_signer.as_slice());
    assert_ne!(
        final_charlie_signer.as_slice(),
        initial_alix_signer.as_slice()
    );

    println!("✓ After sync, all parties have the updated commit log signer");
    println!("Test passed: update_commit_log_signer properly syncs across all parties");
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_updating_group_name_preserves_commit_log_signer() {
    tester!(alix);
    tester!(bo);

    let group = alix.create_group(Some(PolicySet::default()), None).unwrap();

    // Add bo to the group
    group.add_members(&[bo.inbox_id()]).await.unwrap();

    let bo_groups = bo.sync_welcomes().await.unwrap();
    assert_eq!(bo_groups.len(), 1);
    let bo_group = &bo_groups[0];

    // Get the initial signing key from alix's perspective
    let alix_initial_metadata = group.mutable_metadata().unwrap();
    let initial_signer = alix_initial_metadata.commit_log_signer().unwrap();
    let bo_initial_metadata = bo_group.mutable_metadata().unwrap();
    let bo_initial_signer = bo_initial_metadata.commit_log_signer().unwrap();

    // Verify both parties have the same signing key initially
    assert_eq!(initial_signer.as_slice(), bo_initial_signer.as_slice());

    println!("✓ Both parties have the same initial commit log signer");

    // Update the group name
    group
        .update_group_name("New Group Name".to_string())
        .await
        .unwrap();

    // Sync bo to get the group name update
    bo_group.sync().await.unwrap();

    println!("✓ Updated group name and synced to all parties");

    // Verify the signing key is preserved on alix's side
    let alix_updated_metadata = group.mutable_metadata().unwrap();
    let alix_updated_signer = alix_updated_metadata.commit_log_signer().unwrap();

    // Verify the signing key is preserved on bo's side
    let bo_updated_metadata = bo_group.mutable_metadata().unwrap();
    let bo_updated_signer = bo_updated_metadata.commit_log_signer().unwrap();

    // Verify the group name was actually updated
    assert_eq!(
        alix_updated_metadata.attributes.get("group_name").unwrap(),
        "New Group Name"
    );
    assert_eq!(
        bo_updated_metadata.attributes.get("group_name").unwrap(),
        "New Group Name"
    );

    println!("✓ Group name was successfully updated");

    // The key assertion: signing keys should be identical before and after the update
    assert_eq!(initial_signer.as_slice(), alix_updated_signer.as_slice());
    assert_eq!(initial_signer.as_slice(), bo_updated_signer.as_slice());

    // Verify both parties still have the same signing key
    assert_eq!(alix_updated_signer.as_slice(), bo_updated_signer.as_slice());

    println!("✓ Commit log signer preserved after group name update");
    println!("Test passed: Updating group name preserves commit log signing key");
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_legacy_group_signing_key_discovery_via_remote_commit_log() {
    // Disable background CommitLogWorker for deterministic testing
    tester!(alix, with_commit_log_worker: false);
    tester!(bo, with_commit_log_worker: false);
    tester!(charlie, with_commit_log_worker: false);

    // Create a group - this will have a commit log signer in mutable metadata by default
    // We'll simulate a legacy group by having alix use a different key
    let group = alix.create_group(Some(PolicySet::default()), None).unwrap();

    // Add bo and charlie to the group
    group
        .add_members(&[bo.inbox_id(), charlie.inbox_id()])
        .await
        .unwrap();

    let bo_groups = bo.sync_welcomes().await.unwrap();
    assert_eq!(bo_groups.len(), 1);
    let bo_group = &bo_groups[0];

    let charlie_groups = charlie.sync_welcomes().await.unwrap();
    assert_eq!(charlie_groups.len(), 1);
    let charlie_group = &charlie_groups[0];

    println!("✓ Created group with all participants");

    // LEGACY GROUP SIMULATION: Generate a new signing key and store it only in the key store
    // This simulates a legacy group where signing keys weren't in mutable metadata
    let provider = alix.context.mls_provider();
    let key_store = provider.key_store();
    let crypto = provider.crypto();

    // Generate a new signing key
    let new_signing_key = crypto.generate_commit_log_key().unwrap();
    let new_public_key = xmtp_cryptography::signature::to_public_key(&new_signing_key)
        .unwrap()
        .to_vec();

    // Store this key in alix's key store (overwriting any existing key)
    key_store
        .write_commit_log_key(&group.group_id, &new_signing_key)
        .unwrap();

    println!("✓ Generated and stored new signing key for alix in key store");

    // Create commit log worker for alix
    let mut alix_worker = CommitLogWorker::new(alix.context.clone());

    // Alix publishes a commit log entry using the new key from the key store
    let alix_publish_results = alix_worker
        .run_test(CommitLogTestFunction::PublishCommitLogsToRemote, None)
        .await?;

    assert!(!alix_publish_results.is_empty());
    println!("✓ Alix published commit log entries with new signing key");

    // Now alix saves/processes the remote commit log
    // This should trigger maybe_share_private_key to update mutable metadata
    let alix_save_results = alix_worker
        .run_test(CommitLogTestFunction::SaveRemoteCommitLog, None)
        .await?;

    assert!(!alix_save_results.is_empty());
    println!("✓ Alix saved remote commit logs and processed them");

    // Verify alix now has the new signing key in mutable metadata
    let alix_metadata_after = group.mutable_metadata().unwrap();
    let alix_signer_after = alix_metadata_after.commit_log_signer();

    assert!(
        alix_signer_after.is_some(),
        "Alix should have signing key in mutable metadata after processing remote log"
    );
    assert_eq!(
        alix_signer_after.unwrap().as_slice(),
        new_signing_key.as_slice()
    );

    println!("✓ Alix's mutable metadata was updated with the new signing key");

    // Bo and Charlie sync to get the metadata update
    bo_group.sync().await.unwrap();
    charlie_group.sync().await.unwrap();

    println!("✓ Bo and Charlie synced to get metadata updates");

    // Verify bo and charlie now have the signing key in their mutable metadata
    let bo_metadata = bo_group.mutable_metadata().unwrap();
    let bo_signer = bo_metadata.commit_log_signer();

    let charlie_metadata = charlie_group.mutable_metadata().unwrap();
    let charlie_signer = charlie_metadata.commit_log_signer();

    assert!(
        bo_signer.is_some(),
        "Bo should have signing key in mutable metadata after sync"
    );
    assert!(
        charlie_signer.is_some(),
        "Charlie should have signing key in mutable metadata after sync"
    );

    // Verify all parties have the same new signing key
    assert_eq!(bo_signer.unwrap().as_slice(), new_signing_key.as_slice());
    assert_eq!(
        charlie_signer.unwrap().as_slice(),
        new_signing_key.as_slice()
    );

    println!("✓ All participants now have the new signing key in mutable metadata");

    // Additional verification: the consensus key should be set in the database
    let stored_group = alix.context.db().find_group(&group.group_id)?.unwrap();
    assert_eq!(
        stored_group.commit_log_public_key,
        Some(new_public_key.clone())
    );

    println!("✓ Consensus public key correctly stored in database");

    // Final verification: all parties should be able to verify signatures with this key
    let test_entry = xmtp_proto::xmtp::mls::message_contents::CommitLogEntry {
        sequence_id: 999,
        serialized_commit_log_entry: b"test message".to_vec(),
        signature: None,
    };

    let signature_bytes = crypto
        .sign(
            openmls::prelude::SignatureScheme::ED25519,
            &test_entry.serialized_commit_log_entry,
            new_signing_key.as_slice(),
        )
        .unwrap();

    let signed_entry = xmtp_proto::xmtp::mls::message_contents::CommitLogEntry {
        sequence_id: 999,
        serialized_commit_log_entry: test_entry.serialized_commit_log_entry.clone(),
        signature: Some(
            xmtp_proto::xmtp::identity::associations::RecoverableEd25519Signature {
                public_key: new_public_key.clone(),
                bytes: signature_bytes,
            },
        ),
    };

    // All parties should be able to verify this signature
    assert!(
        crypto
            .verify_commit_log_signature(&signed_entry, &new_public_key)
            .is_ok()
    );

    println!("✓ All parties can verify signatures with the discovered signing key");
    println!(
        "Test passed: Legacy group signing key discovery via remote commit log works correctly"
    );
}

// TODO(rich): E2E test for signing key creation and verification on legacy groups
