//! The leaves of `groups/tests/test_min_version.rs` that need the client.

use crate::builder::ClientBuilder;
use crate::context::CoreContext;
use crate::groups::GroupError;
use crate::groups::send_message_opts::SendMessageOpts;
use crate::groups::test_support::{assert_blocked_obligation, increment_patch_version};
use crate::tester;
use crate::utils::VersionInfo;
use xmtp_db::group::GroupQueryArgs;
use xmtp_db::group_message::MsgQueryArgs;
use xmtp_db::prelude::*;
use xmtp_proto::types::{Cursor, Topic};

#[xmtp_common::test(unwrap_try = true)]
async fn test_client_on_old_version_blocks_welcome_until_upgrade() {
    let mut amal_version = VersionInfo::default();
    amal_version.test_update_version(
        increment_patch_version(amal_version.pkg_version())
            .unwrap()
            .as_str(),
    );

    // Step 1: Create three clients, amal and bo are one version ahead of caro
    tester!(amal, version: amal_version);

    let mut bo_version = VersionInfo::default();
    bo_version.test_update_version(
        increment_patch_version(bo_version.pkg_version())
            .unwrap()
            .as_str(),
    );
    tester!(bo, version: bo_version);
    tester!(caro, disable_workers);

    assert!(caro.version_info().pkg_version() != amal.version_info().pkg_version());
    assert!(bo.version_info().pkg_version() == amal.version_info().pkg_version());

    // Step 2: Amal creates a group and adds bo as a member
    let amal_group = amal.create_group(None, None).unwrap();
    amal_group
        .add_members(&[bo.context.identity.inbox_id()])
        .await
        .unwrap();

    // Step 3: Amal sends a message to the group
    amal_group
        .send_message("Hello, world!".as_bytes(), SendMessageOpts::default())
        .await
        .unwrap();

    // Step 4: Verify that bo can read the message
    bo.sync_welcomes().await.unwrap();
    let binding = bo.find_groups(GroupQueryArgs::default()).unwrap();
    let bo_group = binding.first().unwrap();
    bo_group.sync().await.unwrap();
    let messages = bo_group.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(messages.len(), 2);

    let message_text = String::from_utf8_lossy(&messages[1].decrypted_message_bytes);
    assert_eq!(message_text, "Hello, world!");

    // Step 5: Amal updates the group to have a min version of current version + 1
    amal_group
        .update_group_min_version_to_match_self()
        .await
        .unwrap();
    amal_group.sync().await.unwrap();
    amal_group
        .send_message("new version only!".as_bytes(), SendMessageOpts::default())
        .await
        .unwrap();

    // Step 6: Bo should still be able to sync messages for the group
    let _ = bo_group.sync().await;
    let messages = bo_group.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(messages.len(), 4);

    // Step 7: Amal adds caro as a member
    amal_group
        .add_members(&[caro.context.identity.inbox_id()])
        .await
        .unwrap();

    // The unsupported Welcome must keep its keys and leave the group uninstalled.
    let welcome = caro
        .context
        .api()
        .query_welcome_messages(caro.context.installation_id())
        .await
        .unwrap()
        .pop()
        .unwrap();
    let pending = crate::groups::welcome_sync::pending_welcome_for_test(&caro.context, &welcome)
        .await
        .unwrap();
    let result = crate::groups::XmtpWelcome::builder()
        .context(caro.context.clone())
        .welcome(&welcome)
        .pending(pending.clone())
        .validator(crate::groups::InitialMembershipValidator::new(
            caro.context.clone(),
        ))
        .process()
        .await;
    assert!(matches!(
        result,
        Err(GroupError::UnsupportedWelcomeVersion(version))
            if version == amal.version_info().pkg_version()
    ));
    assert!(
        caro.find_groups(GroupQueryArgs::default())
            .unwrap()
            .is_empty()
    );
    let db_topic = xmtp_db::incoming_envelope::StreamTopic {
        entity_id: caro.context.installation_id().to_vec(),
        kind: xmtp_db::incoming_envelope::NetworkEntityKind::Welcome,
    };
    let Err(GroupError::StreamBarrier(error)) = caro.sync_welcomes().await else {
        panic!("the unsupported Welcome must remain blocked");
    };
    let status = assert_blocked_obligation(
        &error,
        &Topic::new_welcome_message(caro.context.installation_id()),
        Cursor(0),
        "welcome_blocked",
    );
    assert_eq!(status.unresolved_welcomes, vec![welcome.cursor]);
    let retained = caro
        .context
        .db()
        .pending_envelope(&db_topic, welcome.cursor)
        .unwrap()
        .unwrap();
    assert!(retained.blocked);
    assert_eq!(retained.envelope, pending.envelope);
    assert!(
        caro.context
            .db()
            .find_group(&amal_group.group_id)
            .unwrap()
            .is_none()
    );

    // Caro updates to the supported version and retries the same Welcome.
    let mut caro_version = caro.version_info().clone();
    caro_version.test_update_version(
        increment_patch_version(caro_version.pkg_version())
            .unwrap()
            .as_str(),
    );

    let installation_id = caro.context.installation_id();
    let inbox_id = caro.inbox_id().to_string();
    let caro = ClientBuilder::from_client(caro.client)
        .version(caro_version)
        .with_disable_workers(true)
        .build()
        .await
        .unwrap();
    assert_eq!(caro.context.installation_id(), installation_id);
    assert_eq!(caro.inbox_id(), inbox_id);
    assert_eq!(
        caro.context
            .db()
            .pending_envelope(&db_topic, welcome.cursor)
            .unwrap()
            .unwrap()
            .envelope,
        pending.envelope,
    );
    caro.sync_welcomes().await.unwrap();
    let binding = caro.find_groups(GroupQueryArgs::default()).unwrap();
    let caro_group = binding.first().unwrap();
    assert!(caro_group.group_id == amal_group.group_id);
    caro_group.sync().await.unwrap();
    assert!(
        caro.context
            .db()
            .pending_envelope(&db_topic, welcome.cursor)
            .unwrap()
            .is_none()
    );
    let fresh_id = amal_group
        .send_message(b"Hello after Caro upgrade", SendMessageOpts::default())
        .await
        .unwrap();
    caro_group.sync().await.unwrap();
    let fresh_messages = caro_group.find_messages(&MsgQueryArgs::default()).unwrap();
    let fresh = fresh_messages.last().unwrap();
    assert_eq!(fresh.id, fresh_id);
    assert_eq!(fresh.decrypted_message_bytes, b"Hello after Caro upgrade");

    // Caro should now be able to send a message
    caro_group
        .send_message("Hello from Caro".as_bytes(), SendMessageOpts::default())
        .await
        .unwrap();
    amal_group.sync().await.unwrap();
    let messages = amal_group.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(
        messages[messages.len() - 1].decrypted_message_bytes,
        "Hello from Caro".as_bytes()
    );
}
