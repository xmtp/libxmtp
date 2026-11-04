//! The leaves of `groups/tests/test_proposals_pause.rs` that need the client.

use crate::{context::CoreContext, tester};
use xmtp_db::prelude::*;

/// A Welcome with a higher AppData version floor remains pending without
/// installing the group. An upgraded client can process the same saved input.
#[xmtp_common::test(unwrap_try = true)]
async fn test_welcome_on_dictionary_group_pauses_below_min_version() {
    use crate::builder::ClientBuilder;
    use crate::groups::test_support::increment_patch_version;
    use crate::utils::VersionInfo;
    use xmtp_cryptography::utils::generate_local_wallet;

    // Alix runs at a newer version. Before migrating, she bumps the
    // legacy GMM's `MinimumSupportedProtocolVersion` to her pkg_version
    // so the bootstrap synthesis carries that floor into the AppData
    // dict (synthesis reads `gmm.attributes` to seed dict entries).
    let mut alix_version = VersionInfo::default();
    alix_version.test_update_version(
        increment_patch_version(alix_version.pkg_version())
            .unwrap()
            .as_str(),
    );
    let alix_pkg_version = alix_version.pkg_version().to_string();
    let alix =
        ClientBuilder::new_test_client_with_version(&generate_local_wallet(), alix_version.clone())
            .await;

    // Bo joins PRE-migration so alix's bootstrap synthesis has resolved
    // member identities to work with. He's at the floor version so the
    // pre-migration min-version bump pauses him via legacy GMM — not
    // the subject of this test.
    tester!(bo);
    let alix_group = alix.create_group(None, None)?;
    alix_group
        .add_members(&[bo.context.identity.inbox_id()])
        .await?;

    // Bump min-version in legacy GMM (the only place to write it
    // before migration). Bootstrap synthesis will pull this value
    // forward into the AppData dict.
    crate::groups::test_support::update_group_min_version_to_match_self(&alix_group).await?;
    alix_group.sync().await?;

    // Alix migrates. Post-migration the legacy GMM is stripped and the
    // floor lives in the AppData dict only.

    // Carol's Welcome carries the version floor only in the AppData dict.
    tester!(carol, disable_workers);
    alix_group
        .add_members(&[carol.context.identity.inbox_id()])
        .await?;

    let error = carol.sync_welcomes().await.unwrap_err();
    let crate::groups::GroupError::StreamBarrier(error) = error else {
        panic!("expected an unsupported Welcome barrier, got {error:?}");
    };
    let topic = xmtp_proto::types::Topic::new_welcome_message(carol.context.installation_id());
    let status = crate::groups::test_support::assert_blocked_obligation(
        &error,
        &topic,
        xmtp_proto::types::Cursor(0),
        "welcome_blocked",
    );
    let target = status.target?;
    assert_eq!(status.unresolved_welcomes, vec![target]);
    let db_topic = xmtp_db::incoming_envelope::StreamTopic {
        entity_id: carol.context.installation_id().to_vec(),
        kind: xmtp_db::incoming_envelope::NetworkEntityKind::Welcome,
    };
    let pending = carol.context.db().pending_envelope(&db_topic, target)??;
    assert!(pending.blocked);
    assert!(
        carol
            .context
            .db()
            .find_group(&alix_group.group_id)?
            .is_none()
    );
    assert!(carol.context.db().read_last_rejection(&db_topic)?.is_none());

    let carol = ClientBuilder::from_client(carol.client)
        .version(alix_version)
        .with_disable_workers(true)
        .build()
        .await?;
    assert_eq!(carol.version_info().pkg_version(), alix_pkg_version);
    assert_eq!(
        carol
            .context
            .db()
            .pending_envelope(&db_topic, target)??
            .envelope,
        pending.envelope,
    );
    carol.sync_welcomes().await?;
    let carol_group = carol.group(&alix_group.group_id)?;
    assert!(carol_group.paused_for_version()?.is_none());
    assert_eq!(
        carol_group.epoch_authenticator().await?,
        alix_group.epoch_authenticator().await?
    );
    assert!(
        carol
            .context
            .db()
            .pending_envelope(&db_topic, target)?
            .is_none()
    );
}

/// Pause recovery: a client that's been pinned to `paused_for_version`
/// gets the flag cleared once their `pkg_version` catches up. Without
/// this sweep a paused group could stay paused indefinitely on quiet
/// installations (the per-group `handle_group_paused` re-evaluator only
/// fires when the group is actively synced, and the sync sweep filters
/// out groups with no new server messages).
///
/// Uses direct `set_group_paused` to install the pause flag rather
/// than driving a real cross-version migration: the pause-side flows
/// are pinned by `test_steady_state_pause_on_min_version_bump_via_app_data_update`
/// and friends, so this test focuses on the sweep logic.
#[xmtp_common::test(unwrap_try = true)]
async fn test_unstick_paused_groups_recovers_after_upgrade() {
    use xmtp_db::prelude::*;

    tester!(alix);
    let alix_pkg = alix.version_info().pkg_version().to_string();
    let alix_group = alix.create_group(None, None)?;
    let group_id_typed = &alix_group.group_id;

    // No paused groups initially → sweep is a no-op.
    assert_eq!(
        alix.unstick_paused_groups().await?,
        0,
        "no paused groups → sweep must return 0"
    );

    // Pin the floor above the client's own version → sweep stays
    // hands-off (an installation can't unstick itself by reading a
    // floor it can't yet satisfy).
    alix.context
        .db()
        .set_group_paused(group_id_typed, "999.0.0")?;
    assert_eq!(
        alix.unstick_paused_groups().await?,
        0,
        "current pkg_version below floor → sweep must NOT unstick"
    );
    assert_eq!(
        alix_group.paused_for_version()?.as_deref(),
        Some("999.0.0"),
        "pause flag must still be set"
    );

    // Pin the floor at or below the client's own version → sweep
    // clears the flag.
    alix.context
        .db()
        .set_group_paused(group_id_typed, &alix_pkg)?;
    assert_eq!(
        alix.unstick_paused_groups().await?,
        1,
        "current pkg_version == floor → sweep must unstick exactly one group"
    );
    assert!(
        alix_group.paused_for_version()?.is_none(),
        "pause flag must be cleared after the sweep"
    );

    // Idempotent: a second sweep on a clean state is a no-op.
    assert_eq!(
        alix.unstick_paused_groups().await?,
        0,
        "second sweep on clean state must be a no-op"
    );

    // Lenient on malformed stored bytes — skip that row, don't
    // poison the sweep for everything else.
    alix.context
        .db()
        .set_group_paused(group_id_typed, "not-a-version")?;
    let result = alix.unstick_paused_groups().await;
    assert!(
        result.is_ok(),
        "sweep must succeed even when a row carries unparseable bytes, got {result:?}",
    );
    assert_eq!(
        result.unwrap(),
        0,
        "unparseable rows are skipped, not unstuck"
    );
    assert_eq!(
        alix_group.paused_for_version()?.as_deref(),
        Some("not-a-version"),
        "unparseable pause row must be preserved verbatim"
    );
}

/// `membership_capabilities` reports raw per-installation extension support
/// plus the group context's extension types — generic facts the app filters.
/// This checks that new groups and their installations advertise
/// `AppDataDictionary`.
#[xmtp_common::test(unwrap_try = true)]
async fn test_membership_capabilities() {
    use crate::groups::{InstallationCapabilities, MlsExtensionType};

    tester!(alix);
    tester!(bo);
    tester!(caro);

    let alix_group = alix
        .create_group_with_members(&[bo.inbox_id(), caro.inbox_id()], None, None)
        .await?;
    bo.sync_welcomes().await?;
    caro.sync_welcomes().await?;

    // Check that each installation advertises dictionary support.
    let supports_proposals = |inst: &InstallationCapabilities| {
        inst.capabilities_known
            && inst
                .supported_extensions
                .contains(&MlsExtensionType::AppDataDictionary)
    };

    let caps = alix_group.membership_capabilities().await?;

    // A new group is dictionary-native.
    assert!(
        caps.context_extensions
            .contains(&MlsExtensionType::AppDataDictionary),
        "a new group's context includes AppDataDictionary"
    );

    assert_eq!(caps.members.len(), 3, "alix, bo, and caro");

    // Every member inbox is represented exactly once.
    let reported: std::collections::HashSet<&str> =
        caps.members.iter().map(|m| m.inbox_id.as_str()).collect();
    assert_eq!(reported.len(), caps.members.len(), "no duplicate inboxes");
    for inbox in [alix.inbox_id(), bo.inbox_id(), caro.inbox_id()] {
        assert!(reported.contains(inbox), "capabilities cover {inbox}");
    }

    // Exactly one installation — the local one (alix's) — is flagged is_own.
    let own_count = caps
        .members
        .iter()
        .flat_map(|m| &m.installations)
        .filter(|i| i.is_own)
        .count();
    assert_eq!(own_count, 1, "only the local installation is marked is_own");

    // All current-code installations advertise AppDataDictionary.
    for member in &caps.members {
        assert!(
            !member.installations.is_empty(),
            "{} should have at least one installation",
            member.inbox_id
        );
        for inst in &member.installations {
            assert!(
                inst.capabilities_known,
                "capabilities known for {}",
                member.inbox_id
            );
            assert!(!inst.installation_id.is_empty());
            assert!(
                supports_proposals(inst),
                "{} advertises AppDataDictionary",
                member.inbox_id
            );
        }
    }

    // All current-code installations advertise dictionary support.
    let blocking: Vec<&str> = caps
        .members
        .iter()
        .filter(|m| m.installations.iter().any(|i| !supports_proposals(i)))
        .map(|m| m.inbox_id.as_str())
        .collect();
    assert!(
        blocking.is_empty(),
        "no inbox lacks dictionary support: {blocking:?}"
    );

    let current = alix_group.membership_capabilities().await?;
    assert!(
        current
            .context_extensions
            .contains(&MlsExtensionType::AppDataDictionary),
        "context advertises AppDataDictionary"
    );
    assert_eq!(current.members.len(), 3);
}
