//! Commit log fork detection tests: authenticator guard.

use super::*;

/// Regression test for the `EpochAuthenticatorNotAdvanced` merge guard.
///
/// Invariant: a successful staged-commit merge by a member that remains in
/// the group always advances the epoch and therefore changes the epoch
/// authenticator. The only legitimate exception — a commit that removes us —
/// is handled separately (`CommitType::RemovedFromGroup`). If the
/// authenticator does not advance for an active member, the group state is
/// inconsistent. Production state writes serialize across processes. This
/// test writes inconsistent state directly to check the additional merge guard.
///
/// This test constructs such a torn state deterministically:
/// 1. Pass 1: process a received commit in a rolled-back transaction and
///    capture the `StagedCommit` + `ValidatedCommit` (what
///    `validate_and_process_external_message` does).
/// 2. "Concurrent process": apply the same commit via a normal `sync()` — the
///    DB is now consistently at epoch E+1.
/// 3. Tear the storage: write the stale epoch-E `GroupContext` (and epoch-E
///    keypair rows, if any) back over the E+1 state, leaving tree / epoch
///    secrets / message secrets at E+1.
/// 4. Reload the group: it reports epoch E but already carries E+1's epoch
///    secrets. Merging the stale staged commit then "succeeds" without
///    advancing the authenticator.
///
/// `merge_staged_commit_logged` must refuse to record that merge: it returns
/// the retryable `EpochAuthenticatorNotAdvanced` error and writes no row. In
/// the real receive path the cursor advance, merge, and log write share one
/// transaction, so the rollback is safe and the message converges via cursor
/// dedup on retry.

#[xmtp_common::test(unwrap_try = true)]
async fn test_merge_staged_commit_logged_rejects_non_advancing_authenticator()
-> Result<(), Box<dyn std::error::Error>> {
    use diesel::{RunQueryDsl, sql_query};
    use openmls_traits::storage::CURRENT_VERSION;

    // Raw access to the openmls kv store. Rows are located by label prefix +
    // raw group id containment rather than by reconstructing the exact
    // bincode-encoded storage key, so the test does not depend on
    // xmtp_db::sql_key_store's key encoding. Restores use the exact key_bytes
    // returned by the scan.
    const KV_SCAN: &str = "SELECT key_bytes, value_bytes FROM openmls_key_value \
         WHERE substr(key_bytes, 1, ?) = ? AND instr(key_bytes, ?) > 0 AND version = ?";
    const KV_REPLACE: &str =
        "REPLACE INTO openmls_key_value (key_bytes, version, value_bytes) VALUES (?, ?, ?)";
    const GROUP_CONTEXT_LABEL: &[u8] = b"GroupContext";
    const EPOCH_KEY_PAIRS_LABEL: &[u8] = b"EpochKeyPairs";

    #[derive(diesel::QueryableByName)]
    struct KvRow {
        #[diesel(sql_type = diesel::sql_types::Binary)]
        key_bytes: Vec<u8>,
        #[diesel(sql_type = diesel::sql_types::Binary)]
        value_bytes: Vec<u8>,
    }

    /// (key_bytes, value_bytes) pairs from the openmls kv store.
    type KvRows = Vec<(Vec<u8>, Vec<u8>)>;

    fn kv_scan(
        db: &impl xmtp_db::ConnectionExt,
        label: &[u8],
        group_id: &[u8],
    ) -> Result<KvRows, Box<dyn std::error::Error>> {
        let rows: Vec<KvRow> = db.raw_query(|conn| {
            sql_query(KV_SCAN)
                .bind::<diesel::sql_types::Integer, _>(label.len() as i32)
                .bind::<diesel::sql_types::Binary, _>(label)
                .bind::<diesel::sql_types::Binary, _>(group_id)
                .bind::<diesel::sql_types::Integer, _>(CURRENT_VERSION as i32)
                .load(conn)
        })?;
        Ok(rows
            .into_iter()
            .map(|r| (r.key_bytes, r.value_bytes))
            .collect())
    }

    fn kv_replace(
        db: &impl xmtp_db::ConnectionExt,
        storage_key: &[u8],
        value: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        db.raw_query(|conn| {
            sql_query(KV_REPLACE)
                .bind::<diesel::sql_types::Binary, _>(storage_key)
                .bind::<diesel::sql_types::Integer, _>(CURRENT_VERSION as i32)
                .bind::<diesel::sql_types::Binary, _>(value)
                .execute(conn)
        })?;
        Ok(())
    }

    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let caro = EngineTestPeer::new().await;

    // Alix creates a group and adds Bo.
    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members(&[bo.inbox_id()]).await?;
    bo.sync_welcomes().await?;
    let bo_group = bo.group(&alix_group.group_id)?;
    bo_group.sync().await?;

    // Record the current cursor, then have Alix publish an
    // UpdateGroupMembership commit that Bo has not yet processed.
    let cursor_before_update = bo
        .context
        .api()
        .query_group_messages(bo_group.group_id)
        .await?
        .into_iter()
        .map(|message| message.cursor.0)
        .max()
        .unwrap_or_default();
    alix_group.add_members(&[caro.inbox_id()]).await?;

    // The dictionary-native membership update is a proposal-by-reference.
    // Replay every proposal from this update before its commit so the copied
    // group has the proposal store that the commit references.
    let mut update_messages = bo
        .context
        .api()
        .query_group_messages(bo_group.group_id)
        .await?
        .into_iter()
        .filter(|message| message.cursor.0 > cursor_before_update)
        .collect::<Vec<_>>();
    update_messages.sort_by_key(|message| message.cursor.0);
    let commit_envelope = update_messages
        .pop()
        .expect("the add-caro commit must be on the network");
    assert!(
        !update_messages.is_empty(),
        "the add-caro commit must have its proposal envelopes"
    );
    let commit_sequence_id = commit_envelope.cursor.0 as i64;

    // Bo's group is at epoch E; snapshot the raw epoch-E GroupContext and
    // epoch-keypair kv rows so they can be written back after the sync.
    let mut group_copy =
        OpenMlsGroup::load(bo.context.mls_storage(), &bo_group.group_id.to_openmls())?
            .expect("bo's group must exist");
    let epoch_e = group_copy.epoch();
    let auth_e = group_copy.epoch_authenticator().as_slice().to_vec();

    let db = bo.context.db();
    let ctx_rows_e = kv_scan(&db, GROUP_CONTEXT_LABEL, bo_group.group_id.as_ref())?;
    assert_eq!(
        ctx_rows_e.len(),
        1,
        "expected exactly one GroupContext kv row for the group"
    );
    // A welcome-joined member that has not yet merged a commit has no
    // epoch-keypair row yet — openmls only writes that row in merge_commit
    // (store_epoch_keypairs), and reads of a missing row gracefully return an
    // empty set. Snapshot whatever rows exist (possibly none) so the torn
    // state mirrors Bo's real epoch-E state.
    let ekp_rows_e = kv_scan(&db, EPOCH_KEY_PAIRS_LABEL, bo_group.group_id.as_ref())?;

    // --- Pass 1 (the second process's view; mirrors the receive path). ---
    // Process the commit against a copy of Bo's group inside a transaction
    // that is intentionally rolled back, capturing the staged + validated
    // commit.
    let provider = bo.context.mls_provider();
    let mut processed_message = None;
    let result = provider.key_store().transaction(|conn| {
        let storage = conn.key_store();
        let provider = XmtpOpenMlsProviderRef::new(&storage);
        for proposal_envelope in &update_messages {
            let proposal = process_message_with_app_data(
                &mut group_copy,
                &provider,
                proposal_envelope.message.clone(),
                bo.context.version_info().pkg_semver(),
            )
            .expect("processing the proposal before its commit succeeds");
            let ProcessedMessageContent::ProposalMessage(proposal) = proposal.into_content() else {
                panic!("the commit prefix must contain only proposals");
            };
            group_copy
                .store_pending_proposal(&storage, *proposal)
                .expect("persisting the proposal before its commit succeeds");
        }
        processed_message = Some(process_message_with_app_data(
            &mut group_copy,
            &provider,
            commit_envelope.message.clone(),
            bo.context.version_info().pkg_semver(),
        ));
        Ok::<TransactionOutcome<()>, StorageError>(Rollback)
    });
    assert!(matches!(result, Ok(Rollback)));
    let processed_message = processed_message
        .expect("set in the transaction above")
        .expect("processing the commit at the old epoch succeeds");

    let committer_leaf_index = match processed_message.sender() {
        Sender::Member(idx) => *idx,
        _ => panic!("commit must come from a member"),
    };
    let staged_commit_ref = match processed_message.content() {
        ProcessedMessageContent::StagedCommitMessage(staged) => staged,
        _ => panic!("expected a staged commit message"),
    };
    let validated_commit = ValidatedCommit::from_staged_commit(
        &bo_group.context,
        staged_commit_ref,
        committer_leaf_index,
        &group_copy,
    )
    .await
    .expect("commit validation succeeds");
    let staged_commit = match processed_message.into_content() {
        ProcessedMessageContent::StagedCommitMessage(staged) => *staged,
        _ => unreachable!(),
    };
    drop(group_copy);

    // --- The "concurrent process" applies the same commit normally. ---
    bo_group.sync().await?;

    let consistent = OpenMlsGroup::load(bo.context.mls_storage(), &bo_group.group_id.to_openmls())?
        .expect("bo's group must exist");
    assert_eq!(consistent.epoch(), staged_commit.group_context().epoch());
    let auth_e1 = consistent.epoch_authenticator().as_slice().to_vec();
    assert_ne!(
        auth_e, auth_e1,
        "the applied commit advanced the authenticator"
    );
    drop(consistent);

    // --- Tear the storage. ---
    // Write the stale epoch-E GroupContext (and any epoch-E keypair rows)
    // back over the E+1 state, leaving tree / epoch secrets / message secrets
    // at E+1. This simulates a second process's interleaved writes landing on
    // the shared kv store without cross-process mutual exclusion.
    for (key, value) in ctx_rows_e.iter().chain(ekp_rows_e.iter()) {
        kv_replace(&db, key, value)?;
    }

    // --- The reload hands back the torn group: it reports epoch E, but its
    // epoch secrets (and therefore its authenticator) are already E+1's.
    let mut torn = OpenMlsGroup::load(bo.context.mls_storage(), &bo_group.group_id.to_openmls())?
        .expect("bo's group must exist");
    assert_eq!(torn.epoch(), epoch_e, "torn group reports the stale epoch");
    assert_eq!(
        torn.epoch_authenticator().as_slice(),
        auth_e1.as_slice(),
        "torn group already carries the post-commit epoch secrets"
    );

    let logs_before = bo.context.db().get_group_logs(&bo_group.group_id)?.len();

    // --- Pass 2: merge the stale staged commit on the torn group. ---
    // From the torn group's perspective this is an ordinary merge (epoch E ->
    // E+1), so openmls accepts it — but the authenticator cannot advance.
    let merge_result = torn.merge_staged_commit_logged(
        &provider,
        staged_commit,
        &validated_commit,
        commit_sequence_id,
        xmtp_configuration::ENABLE_COMMIT_LOG,
    );

    assert!(
        merge_result.is_err(),
        "merge_staged_commit_logged must refuse to log a Success commit whose \
         merge did not advance the epoch authenticator (applied == last); \
         persisting it would corrupt the local commit log"
    );

    // And the corrupt row must not have been written.
    let logs = bo.context.db().get_group_logs(&bo_group.group_id)?;
    assert_eq!(
        logs.len(),
        logs_before,
        "no local commit log entry may be written for the rejected merge"
    );
    for log in &logs {
        if log.commit_result == CommitResult::Success
            && !log.last_epoch_authenticator.is_empty()
            && log.commit_type != Some(CommitType::RemovedFromGroup.to_string())
        {
            assert_ne!(
                log.applied_epoch_authenticator, log.last_epoch_authenticator,
                "successful commit at applied_epoch_number={} recorded an epoch \
                 authenticator that did not advance",
                log.applied_epoch_number
            );
        }
    }

    Ok(())
}
