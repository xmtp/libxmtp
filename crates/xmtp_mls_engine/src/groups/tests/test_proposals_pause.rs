//! Pause and recovery paths.

use crate::groups::test_support::{EnginePeer, EngineTestPeer, EngineTestPeerOps};
use xmtp_db::prelude::*;
use xmtp_mls_core::context::CoreContext;

/// XIP §3 steady-state pause path: when a dictionary-native client bumps
/// `MIN_SUPPORTED_PROTOCOL_VERSION` on a dictionary-native group, the
/// floor flows as an `AppDataUpdate(MIN_SUPPORTED_PROTOCOL_VERSION)`
/// proposal carried inside a regular commit. The legacy GMM extension
/// is absent, so the validator cannot diff it. Instead it
/// must read the post-commit floor from the dict overlay (current dict
/// + any staged AppDataUpdate proposals targeting the component) and
/// raise `ProtocolVersionTooLow` against the receiver's pkg_version.
/// `mls_sync` then writes `paused_for_version`.
///
/// Sibling of `test_welcome_on_dictionary_group_pauses_below_min_version`
/// (welcome-time pause). The validator checks the proposed protocol floor
/// separately from membership authorization, after component authorization.
/// `MetadataChanges` is built afterward for transcripts and status updates;
/// it is not used to check the protocol floor.
#[xmtp_common::test(unwrap_try = true)]
async fn test_steady_state_pause_on_min_version_bump_via_app_data_update() {
    use crate::groups::tests::increment_patch_version;
    use xmtp_mls_core::version::VersionInfo;

    // Alix runs one patch ahead of the default pkg_version so she can
    // legitimately bump the floor to her own version — the send-side
    // clamp added in this change rejects `min_version > own_pkg_version`
    // (footgun guard). Bo stays at the default version so he ends up
    // below the new floor.
    let mut alix_version = VersionInfo::default();
    let bumped = increment_patch_version(alix_version.pkg_version()).expect("patch bump");
    alix_version.test_update_version(&bumped);
    let alix_pkg_version = alix_version.pkg_version().to_string();
    let alix = EngineTestPeer::new_with(|o| {
        o.version_info = alix_version;
        o.triggers = true;
    })
    .await;

    let bo = EngineTestPeer::new().await;

    let alix_group = alix.create_group(None, None)?;
    alix_group
        .add_members(&[bo.context.identity.inbox_id()])
        .await?;

    let bo_groups = bo.sync_welcomes().await?;
    let bo_group = bo_groups
        .iter()
        .find(|g| g.group_id == alix_group.group_id)
        .expect("bo should receive a welcome for alix_group");
    bo_group.sync().await?;

    // The creation floor permits both clients. Raising that committed floor
    // must still pause the client whose version is too low.
    for (label, group) in [("alix", &alix_group), ("bo", bo_group)] {
        assert!(
            group.paused_for_version()?.is_none(),
            "{label} must not be paused at the creation floor"
        );
    }

    let before = bo_group.epoch_authenticator().await?;
    let db_topic = xmtp_db::incoming_envelope::StreamTopic::group(bo_group.group_id);
    let processed = bo.context.db().topic_progress(&db_topic)?.processed;
    let paused_events = bo.context.events().subscribe(
        xmtp_events::EventFilter::new([xmtp_events::EventKind::ConversationPaused]),
        Some(10),
    );

    // Alix raises the floor to her own version, which is above bo's.
    // Send-side clamp is satisfied (alix's pkg_version == requested
    // floor). The bump flows as an
    // `AppDataUpdate(MIN_SUPPORTED_PROTOCOL_VERSION)` inside a commit —
    // The legacy GMM extension is absent, so the dictionary is
    // the only path the floor can ride on.
    alix_group
        .update_group_min_version(&alix_pkg_version)
        .await?;

    let envelopes = alix
        .context
        .api()
        .query_group_messages(alix_group.group_id)
        .await?;
    let added: Vec<_> = envelopes
        .iter()
        .filter(|message| message.cursor > processed)
        .collect();
    let [proposal, commit] = added.as_slice() else {
        panic!("expected the version proposal and its commit, got {added:?}");
    };
    assert_eq!(
        proposal.message.content_type(),
        openmls::prelude::ContentType::Proposal
    );
    assert!(commit.is_commit());
    let predecessor = proposal.cursor;
    let blocked_cursor = commit.cursor;

    super::assert_version_sync_blocked(
        bo_group.sync().await.unwrap_err(),
        &xmtp_proto::types::Topic::new_group_message(bo_group.group_id),
        predecessor,
    );
    assert_eq!(bo_group.epoch_authenticator().await?, before);
    assert_eq!(
        bo.context.db().topic_progress(&db_topic)?.processed,
        predecessor
    );
    let pending = bo.context.db().first_pending_envelope(&db_topic)??;
    assert_eq!(pending.sequence_id as u64, blocked_cursor.0);
    assert!(pending.blocked);
    assert_eq!(
        pending.error_code.as_deref(),
        Some("unsupported_protocol_version")
    );
    assert!(bo.context.db().read_last_rejection(&db_topic)?.is_none());
    let paused = bo_group.paused_for_version()?;
    assert_eq!(
        paused.as_deref(),
        Some(alix_pkg_version.as_str()),
        "bo must be paused at the new floor via the AppDataUpdate-driven path; \
         the legacy GMM extension is absent, so the dict overlay is the \
         only floor signal the validator can read"
    );
    assert!(matches!(
        paused_events.drain().as_slice(),
        [xmtp_events::EventEnvelope {
            client: Some(xmtp_events::ClientEvent::ConversationPaused(change)), ..
        }] if change.group_id == bo_group.group_id.as_slice()
            && change.floor == alix_pkg_version
    ));
}

/// Downgrade safety: pausing normally happens at the floor-*bump* commit
/// (the post-policy `ProtocolVersionTooLow` check), but a client that
/// processed the bump while ABOVE the floor and then downgrades never
/// took that pause — its DB holds a migrated group whose committed
/// floor exceeds its (new, lower) version, with `paused_for_version`
/// unset. The next sync must pause the group from the committed dict
/// floor (the pause-before-parse guards) rather than surface commit
/// processing errors: a non-retryable rejection here is a fork, since
/// above-floor peers accept the same commits.
///
/// Emulates the downgrade by re-opening the same persistent store with
/// a client built at a lower pkg_version, mirroring the restart pattern
/// in `test::builder::identity_persistence_test`. The post-bump commit
/// in this test uses a format both versions understand, so it pins the
/// end state (paused, at the right floor, no error) rather than
/// distinguishing which floor check fired first; the guard ordering
/// (pre-dispatch, before any payload is interpreted) is pinned by the
/// unit tests on `committed_floor_exceeding_in_extensions` plus the
/// guard's placement in `process_message_with_app_data`.
#[xmtp_common::test(unwrap_try = true)]
async fn test_downgraded_client_pauses_on_dictionary_group_with_higher_floor() {
    use crate::groups::tests::increment_patch_version;
    use xmtp_mls_core::version::VersionInfo;

    // Both clients start one patch above the default version so the
    // floor can be raised to a value the default version doesn't meet.
    let mut high_version = VersionInfo::default();
    let bumped = increment_patch_version(high_version.pkg_version()).expect("patch bump");
    high_version.test_update_version(&bumped);

    let alix = EngineTestPeer::new_with(|o| {
        o.version_info = high_version.clone();
        o.triggers = true;
    })
    .await;

    // Bo lives on a persistent store so the same identity can be
    // re-opened at a lower version below.
    let bo = EngineTestPeer::persistent_with(|options| {
        options.version_info = high_version.clone();
        options.query_only = true;
    })
    .await;
    let bo_db_path = match bo.context.store().opts() {
        xmtp_db::StorageOption::Persistent(path) => path.clone(),
        xmtp_db::StorageOption::Ephemeral => panic!("downgrade test needs a persistent database"),
    };

    let alix_group = alix.create_group(None, None)?;
    alix_group
        .add_members(&[bo.context.identity.inbox_id()])
        .await?;
    let bo_groups = bo.sync_welcomes().await?;
    let bo_group = bo_groups
        .iter()
        .find(|g| g.group_id == alix_group.group_id)
        .expect("bo should receive a welcome for alix_group");
    bo_group.sync().await?;

    // Migrate with the 0.0.0 test floor so nobody pauses at bootstrap.

    bo_group.sync().await?;

    bo_group.sync().await?;

    // Raise the floor to the bumped version. Bo — at that same version —
    // processes the bump WITHOUT pausing: he meets the floor.
    alix_group.update_group_min_version(&bumped).await?;
    bo_group.sync().await?;
    assert!(
        bo_group.paused_for_version()?.is_none(),
        "bo meets the floor pre-downgrade and must not be paused"
    );

    // Traffic lands after the bump; the downgraded client below will
    // meet this commit as the first thing it processes.
    alix_group
        .update_group_name("post-bump name".to_string())
        .await?;

    // The downgrade: re-open bo's store with a client at the DEFAULT
    // (lower) pkg_version.
    let bo_group_id = bo_group.group_id;
    let bo_owner = bo.owner.clone();
    let bo_strategy = xmtp_mls_identity::identity_lifecycle::identity_setup(&bo_owner);
    drop(bo);
    let bo_downgraded =
        EngineTestPeer::open_persistent(bo_owner, &bo_db_path, bo_strategy, |options| {
            options.version_info = VersionInfo::default()
        })
        .await;

    let bo_group = bo_downgraded.group(&bo_group_id)?;
    // The sync may surface the pause as a per-message outcome; the
    // assertion below is the contract, not the sync result.
    let _ = bo_group.sync().await;

    assert_eq!(
        bo_group.paused_for_version()?.as_deref(),
        Some(bumped.as_str()),
        "a downgraded client below the committed floor must pause the group \
         (deferring all commits for post-upgrade reprocessing), never surface \
         processing errors that would advance past peers"
    );
}

/// Extension type 0xf000 reports as `Unknown(0xf000)`. The xmtp OpenMLS fork
/// names it `ImmutableMetadata`; upstream OpenMLS does not.
#[xmtp_common::test]
fn extension_type_ids_map_to_mls_extension_types() {
    use crate::groups::MlsExtensionType;
    use openmls::prelude::ExtensionType;

    for (id, expected) in [
        (0xf000, MlsExtensionType::Unknown(0xf000)),
        (0x1234, MlsExtensionType::Unknown(0x1234)),
        (6, MlsExtensionType::AppDataDictionary),
        (10, MlsExtensionType::LastResort),
    ] {
        assert_eq!(
            MlsExtensionType::from(ExtensionType::from(id)),
            expected,
            "{id:#06x}"
        );
    }
}
