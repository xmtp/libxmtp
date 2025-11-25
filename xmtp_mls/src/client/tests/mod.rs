mod test_readonly_mode;

#[cfg(target_arch = "wasm32")]
wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

use super::Client;
use crate::context::XmtpSharedContext;
use crate::groups::send_message_opts::SendMessageOpts;
use crate::identity::IdentityError;
use crate::subscriptions::StreamMessages;
use crate::tester;
use crate::utils::{LocalTester, LocalTesterBuilder, Tester};
use crate::{builder::ClientBuilder, identity::serialize_key_package_hash_ref};
use diesel::RunQueryDsl;
use futures::TryStreamExt;
use futures::stream::StreamExt;
use prost::Message;
use std::time::Duration;
use xmtp_common::time::now_ns;
use xmtp_common::{NS_IN_SEC, toxiproxy_test};
use xmtp_content_types::ContentCodec;
use xmtp_content_types::text::TextCodec;
use xmtp_cryptography::utils::generate_local_wallet;
use xmtp_db::consent_record::{ConsentType, StoredConsentRecord};
use xmtp_db::identity::StoredIdentity;
use xmtp_db::prelude::*;
use xmtp_db::{
    ConnectionExt, Fetch, consent_record::ConsentState, group::GroupQueryArgs,
    group_message::MsgQueryArgs, schema::identity_updates,
};
use xmtp_id::associations::test_utils::WalletTestExt;

#[xmtp_common::test]
async fn test_group_member_recovery() {
    let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let bola_wallet = generate_local_wallet();
    // Add two separate installations for Bola
    let bola_a = ClientBuilder::new_test_client(&bola_wallet).await;
    let bola_b = ClientBuilder::new_test_client(&bola_wallet).await;

    let group = amal.create_group(None, None).unwrap();

    // Add both of Bola's installations to the group
    group
        .add_members_by_inbox_id(&[bola_a.inbox_id(), bola_b.inbox_id()])
        .await
        .unwrap();

    let conn = amal.context.store().conn();
    conn.raw_query_write(|conn| diesel::delete(identity_updates::table).execute(conn))
        .unwrap();

    let members = group.members().await.unwrap();
    // The three installations should count as two members
    assert_eq!(members.len(), 2);
}

#[xmtp_common::test]
async fn test_mls_error() {
    let client = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let result = client
        .context
        .api()
        .upload_key_package(vec![1, 2, 3], false)
        .await;

    assert!(result.is_err());
    let error_string = result.err().unwrap().to_string();
    assert!(error_string.contains("invalid identity") || error_string.contains("EndOfStream"));
}

#[xmtp_common::test]
async fn test_register_installation() {
    let wallet = generate_local_wallet();
    let client = ClientBuilder::new_test_client(&wallet).await;
    let client_2 = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    // Make sure the installation is actually on the network
    let association_state = client_2
        .identity_updates()
        .get_latest_association_state(&client_2.context.db(), client.inbox_id())
        .await
        .unwrap();

    assert_eq!(association_state.installation_ids().len(), 1);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(
    not(target_arch = "wasm32"),
    tokio::test(flavor = "multi_thread", worker_threads = 1)
)]
async fn test_rotate_key_package() {
    let wallet = generate_local_wallet();
    let client = ClientBuilder::new_test_client(&wallet).await;

    let installation_public_key = client.installation_public_key().to_vec();
    // Get original KeyPackage.
    let mut kp1 = client
        .get_key_packages_for_installation_ids(vec![installation_public_key.clone()])
        .await
        .unwrap();
    assert_eq!(kp1.len(), 1);
    let binding = kp1.remove(&installation_public_key).unwrap().unwrap();
    let init1 = binding.inner.hpke_init_key();
    let fetched_identity: StoredIdentity = client.context.db().fetch(&()).unwrap().unwrap();
    assert!(fetched_identity.next_key_package_rotation_ns.is_some());
    // Rotate and fetch again.
    client.queue_key_rotation().await.unwrap();
    //check the rotation value has been set
    let fetched_identity: StoredIdentity = client.context.db().fetch(&()).unwrap().unwrap();
    assert!(fetched_identity.next_key_package_rotation_ns.is_some());

    xmtp_common::time::sleep(std::time::Duration::from_secs(11)).await;

    let mut kp2 = client
        .get_key_packages_for_installation_ids(vec![installation_public_key.clone()])
        .await
        .unwrap();
    assert_eq!(kp2.len(), 1);
    let binding = kp2.remove(&installation_public_key).unwrap().unwrap();
    let init2 = binding.inner.hpke_init_key();

    assert_ne!(init1, init2);
}

#[xmtp_common::test]
async fn test_find_groups() {
    let client = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let group_1 = client.create_group(None, None).unwrap();
    let group_2 = client.create_group(None, None).unwrap();

    let groups = client.find_groups(GroupQueryArgs::default()).unwrap();
    assert_eq!(groups.len(), 2);
    assert!(groups.iter().any(|g| g.group_id == group_1.group_id));
    assert!(groups.iter().any(|g| g.group_id == group_2.group_id));
}

#[xmtp_common::test]
async fn test_find_inbox_id() {
    let wallet = generate_local_wallet();
    let client = ClientBuilder::new_test_client(&wallet).await;
    assert_eq!(
        client
            .find_inbox_id_from_identifier(&client.context.db(), wallet.identifier())
            .await
            .unwrap(),
        Some(client.inbox_id().to_string())
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn double_dms() {
    tester!(alice);
    tester!(bob);

    let alice_dm = alice
        .create_dm_by_inbox_id(bob.inbox_id().to_string(), None)
        .await?;
    alice_dm
        .send_message(b"Welcome 1", SendMessageOpts::default())
        .await?;

    let bob_dm = bob
        .create_dm_by_inbox_id(alice.inbox_id().to_string(), None)
        .await?;

    tester!(alice2, from: alice);
    let alice_dm2 = alice
        .create_dm_by_inbox_id(bob.inbox_id().to_string(), None)
        .await?;
    alice_dm2
        .send_message(b"Welcome 2", SendMessageOpts::default())
        .await?;

    alice_dm.update_installations().await?;
    alice.sync_welcomes().await?;
    bob.sync_welcomes().await?;

    alice_dm
        .send_message(b"Welcome from 1", SendMessageOpts::default())
        .await?;

    // This message will set bob's dm as the primary DM for all clients
    bob_dm
        .send_message(b"Bob says hi 1", SendMessageOpts::default())
        .await?;
    // Alice will sync, pulling in Bob's DM message, which will cause
    // a database trigger to update `last_message_ns`, putting bob's DM to the top.
    alice_dm.sync().await?;

    alice2.sync_welcomes().await?;
    let mut groups = alice2.find_groups(GroupQueryArgs::default())?;

    assert_eq!(groups.len(), 1);
    let group = groups.pop()?;

    group.sync().await?;
    let messages = group.find_messages(&MsgQueryArgs::default())?;

    assert_eq!(messages.len(), 4);

    // Reload alice's DM. This will load the DM that Bob just created and sent a message on.
    let new_alice_dm = alice.stitched_group(&alice_dm.group_id)?;

    // The group_id should not be what we asked for because it was stitched
    assert_ne!(alice_dm.group_id, new_alice_dm.group_id);
    // They should be the same, due the the message that Bob sent above.
    assert_eq!(new_alice_dm.group_id, bob_dm.group_id);
}

#[rstest::rstest]
#[xmtp_common::test(flavor = "multi_thread")]
async fn only_test_sync_welcomes() {
    let alice = ClientBuilder::new_test_client_vanilla(&generate_local_wallet()).await;
    let bob = ClientBuilder::new_test_client_vanilla(&generate_local_wallet()).await;

    let alice_bob_group = alice.create_group(None, None).unwrap();
    alice_bob_group
        .add_members_by_inbox_id(&[bob.inbox_id()])
        .await
        .unwrap();

    let bob_received_groups = bob.sync_welcomes().await.unwrap();
    assert_eq!(bob_received_groups.len(), 1);
    assert_eq!(
        bob_received_groups.first().unwrap().group_id,
        alice_bob_group.group_id
    );

    let duplicate_received_groups = bob.sync_welcomes().await.unwrap();
    assert_eq!(duplicate_received_groups.len(), 0);
}

#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(flavor = "multi_thread")]
async fn test_leaf_node_lifetime_validation_disabled() {
    use crate::utils::test_mocks_helpers::set_test_mode_limit_key_package_lifetime;

    // Create a client with default KP lifetime
    let alice = ClientBuilder::new_test_client(&generate_local_wallet()).await;

    // Create a client with default KP lifetime
    set_test_mode_limit_key_package_lifetime(false, 0);
    let cat = ClientBuilder::new_test_client(&generate_local_wallet()).await;

    let alice_bob_group = alice.create_group(None, None).unwrap();
    alice_bob_group
        .add_members_by_inbox_id(&[cat.inbox_id()])
        .await
        .unwrap();

    let cat_received_groups = cat.sync_welcomes().await.unwrap();
    assert_eq!(cat_received_groups.len(), 1);
    assert_eq!(
        cat_received_groups.first().unwrap().group_id,
        alice_bob_group.group_id
    );

    // Create a client with a KP that expires in 5 seconds
    set_test_mode_limit_key_package_lifetime(true, 5);
    let bob = ClientBuilder::new_test_client(&generate_local_wallet()).await;

    // Alice invites Bob with short living KP
    alice_bob_group
        .add_members_by_inbox_id(&[bob.inbox_id()])
        .await
        .unwrap();

    // Since Bob's KP is still valid, Bob should successfully process the Welcome
    let bob_received_groups = bob.sync_welcomes().await.unwrap();

    // Wait for Bob's KP and their leafnode's lifetime to expire
    xmtp_common::time::sleep(Duration::from_secs(7)).await;

    assert_eq!(bob_received_groups.len(), 1);
    assert_eq!(
        bob_received_groups.first().unwrap().group_id,
        alice_bob_group.group_id
    );

    let bob_duplicate_received_groups = bob.sync_welcomes().await.unwrap();
    let cat_duplicate_received_groups = cat.sync_welcomes().await.unwrap();
    assert_eq!(bob_duplicate_received_groups.len(), 0);
    assert_eq!(cat_duplicate_received_groups.len(), 0);

    set_test_mode_limit_key_package_lifetime(false, 0);
    let dave = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    alice_bob_group
        .add_members_by_inbox_id(&[dave.inbox_id()])
        .await
        .unwrap();
    // Dave should be okay receiving a welcome where members of the group are expired
    let dave_received_groups = dave.sync_welcomes().await.unwrap();
    assert_eq!(dave_received_groups.len(), 1);
    assert_eq!(
        dave_received_groups.first().unwrap().group_id,
        alice_bob_group.group_id
    );
    let dave_duplicate_received_groups = dave.sync_welcomes().await.unwrap();
    assert_eq!(dave_duplicate_received_groups.len(), 0);

    // Cat receives commits to add expired group members, they should pass validation and be added
    let cat_group = cat_received_groups.first().unwrap();
    cat_group.sync().await.unwrap();
    assert_eq!(cat_group.members().await.unwrap().len(), 4);
}

#[rstest::rstest]
#[xmtp_common::test(flavor = "multi_thread", worker_threads = 10)]
async fn test_sync_all_groups() {
    let alix = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let bo = ClientBuilder::new_test_client(&generate_local_wallet()).await;

    let alix_bo_group1 = alix.create_group(None, None).unwrap();
    let alix_bo_group2 = alix.create_group(None, None).unwrap();
    alix_bo_group1
        .add_members_by_inbox_id(&[bo.inbox_id()])
        .await
        .unwrap();
    alix_bo_group2
        .add_members_by_inbox_id(&[bo.inbox_id()])
        .await
        .unwrap();

    let bob_received_groups = bo.sync_welcomes().await.unwrap();
    assert_eq!(bob_received_groups.len(), 2);

    let bo_groups = bo.find_groups(GroupQueryArgs::default()).unwrap();
    let bo_group1 = bo.group(&alix_bo_group1.clone().group_id).unwrap();
    let bo_messages1 = bo_group1.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(bo_messages1.len(), 1);
    let bo_group2 = bo.group(&alix_bo_group2.clone().group_id).unwrap();
    let bo_messages2 = bo_group2.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(bo_messages2.len(), 1);
    alix_bo_group1
        .send_message(vec![1, 2, 3].as_slice(), SendMessageOpts::default())
        .await
        .unwrap();
    alix_bo_group2
        .send_message(vec![1, 2, 3].as_slice(), SendMessageOpts::default())
        .await
        .unwrap();

    let summary = bo.sync_all_groups(bo_groups).await.unwrap();
    assert_eq!(summary.num_synced, 2);

    let bo_messages1 = bo_group1.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(bo_messages1.len(), 2);
    let bo_group2 = bo.group(&alix_bo_group2.clone().group_id).unwrap();
    let bo_messages2 = bo_group2.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(bo_messages2.len(), 2);
}

#[xmtp_common::test(flavor = "multi_thread")]
async fn test_sync_all_groups_and_welcomes() {
    tester!(alix);
    tester!(bo, passkey);

    // Create two groups and add Bob
    let alix_bo_group1 = alix.create_group(None, None).unwrap();
    let alix_bo_group2 = alix.create_group(None, None).unwrap();

    alix_bo_group1
        .add_members_by_inbox_id(&[bo.inbox_id()])
        .await
        .unwrap();
    alix_bo_group2
        .add_members_by_inbox_id(&[bo.inbox_id()])
        .await
        .unwrap();

    // Initial sync (None): Bob should fetch both groups
    let bob_received_groups = bo.sync_all_welcomes_and_groups(None).await.unwrap();
    assert_eq!(bob_received_groups.num_synced, 0);

    xmtp_common::time::sleep(Duration::from_millis(100)).await;

    // Verify Bo initially has no messages
    let bo_group1 = bo.group(&alix_bo_group1.group_id.clone()).unwrap();
    assert_eq!(
        bo_group1
            .find_messages(&MsgQueryArgs::default())
            .unwrap()
            .len(),
        1
    );
    let bo_group2 = bo.group(&alix_bo_group2.group_id.clone()).unwrap();
    assert_eq!(
        bo_group2
            .find_messages(&MsgQueryArgs::default())
            .unwrap()
            .len(),
        1
    );

    // Alix sends a message to both groups
    alix_bo_group1
        .send_message(vec![1, 2, 3].as_slice(), SendMessageOpts::default())
        .await
        .unwrap();
    alix_bo_group2
        .send_message(vec![4, 5, 6].as_slice(), SendMessageOpts::default())
        .await
        .unwrap();

    // Sync with `Unknown`: Bob should not fetch new messages
    let bob_received_groups_unknown = bo
        .sync_all_welcomes_and_groups(Some([ConsentState::Allowed].to_vec()))
        .await
        .unwrap();
    assert_eq!(bob_received_groups_unknown.num_synced, 0);

    // Verify Bob still has no messages
    assert_eq!(
        bo_group1
            .find_messages(&MsgQueryArgs::default())
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        bo_group2
            .find_messages(&MsgQueryArgs::default())
            .unwrap()
            .len(),
        1
    );

    // Alix sends another message to both groups
    alix_bo_group1
        .send_message(vec![7, 8, 9].as_slice(), SendMessageOpts::default())
        .await
        .unwrap();
    alix_bo_group2
        .send_message(vec![10, 11, 12].as_slice(), SendMessageOpts::default())
        .await
        .unwrap();

    // Sync with `None`: Bob should fetch all messages
    let bo_sync_summary = bo
        .sync_all_welcomes_and_groups(Some([ConsentState::Unknown].to_vec()))
        .await
        .unwrap();
    assert_eq!(bo_sync_summary.num_synced, 2);

    // Verify Bob now has all messages
    let bo_messages1 = bo_group1.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(bo_messages1.len(), 3);

    let bo_messages2 = bo_group2.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(bo_messages2.len(), 3);
}

#[xmtp_common::test]
async fn test_sync_100_allowed_groups_performance() {
    tester!(alix);
    tester!(bo, passkey);

    let group_count = 100;
    let mut groups = Vec::with_capacity(group_count);

    for _ in 0..group_count {
        let group = alix.create_group(None, None).unwrap();
        group
            .add_members_by_inbox_id(&[bo.inbox_id()])
            .await
            .unwrap();
        groups.push(group);
    }

    xmtp_common::time::sleep(Duration::from_millis(100)).await;

    let start = xmtp_common::time::Instant::now();
    let _synced_count = bo.sync_all_welcomes_and_groups(None).await.unwrap();
    let elapsed = start.elapsed();

    let test_group = groups.first().unwrap();
    let bo_group = bo.group(&test_group.group_id).unwrap();
    assert_eq!(
        bo_group
            .find_messages(&MsgQueryArgs::default())
            .unwrap()
            .len(),
        1,
        "Expected 1 welcome message synced"
    );

    println!(
        "Synced {} groups in {:?} (avg per group: {:?})",
        group_count,
        elapsed,
        elapsed / group_count as u32
    );

    let start = xmtp_common::time::Instant::now();
    bo.sync_all_welcomes_and_groups(None).await.unwrap();
    let elapsed = start.elapsed();

    println!(
        "Synced {} groups in {:?} (avg per group: {:?})",
        group_count,
        elapsed,
        elapsed / group_count as u32
    );
}

#[rstest::rstest]
#[xmtp_common::test]
async fn test_add_remove_then_add_again() {
    let amal = Tester::new().await;
    let bola = Tester::new().await;

    // Create a group and invite bola
    let amal_group = amal.create_group(None, None).unwrap();
    amal_group
        .add_members_by_inbox_id(&[bola.inbox_id()])
        .await
        .unwrap();
    assert_eq!(amal_group.members().await.unwrap().len(), 2);

    // Now remove bola
    amal_group
        .remove_members_by_inbox_id(&[bola.inbox_id()])
        .await
        .unwrap();
    assert_eq!(amal_group.members().await.unwrap().len(), 1);

    // See if Bola can see that they were added to the group
    bola.sync_welcomes().await.unwrap();
    let bola_groups = bola.find_groups(Default::default()).unwrap();
    assert_eq!(bola_groups.len(), 1);
    let bola_group = bola_groups.first().unwrap();
    bola_group.sync().await.unwrap();

    assert!(!bola_group.is_active().unwrap());

    // Bola should have one readable message (them being added to the group)
    let mut bola_messages = bola_group.find_messages(&MsgQueryArgs::default()).unwrap();

    assert_eq!(bola_messages.len(), 2);

    // Add Bola back to the group
    amal_group
        .add_members_by_inbox_id(&[bola.inbox_id()])
        .await
        .unwrap();
    bola.sync_welcomes().await.unwrap();

    // Send a message from Amal, now that Bola is back in the group
    amal_group
        .send_message(vec![1, 2, 3].as_slice(), SendMessageOpts::default())
        .await
        .unwrap();

    // Sync Bola's state to get the latest
    if let Err(err) = bola_group.sync().await {
        panic!("Error syncing group: {:?}", err);
    }
    // Find Bola's updated list of messages
    bola_messages = bola_group.find_messages(&MsgQueryArgs::default()).unwrap();
    // Bola should have been able to decrypt the last message
    assert_eq!(bola_messages.len(), 4);
    assert_eq!(
        bola_messages.get(3).unwrap().decrypted_message_bytes,
        vec![1, 2, 3]
    )
}

async fn get_key_package_init_key<Context: XmtpSharedContext, Id: AsRef<[u8]>>(
    client: &Client<Context>,
    installation_id: Id,
) -> Result<Vec<u8>, IdentityError> {
    let mut kps_map = client
        .get_key_packages_for_installation_ids(vec![installation_id.as_ref().to_vec()])
        .await
        .map_err(|_| IdentityError::NewIdentity("Failed to fetch key packages".to_string()))?;

    let kp_result = kps_map.remove(installation_id.as_ref()).ok_or_else(|| {
        IdentityError::NewIdentity(format!(
            "Missing key package for {}",
            hex::encode(installation_id.as_ref())
        ))
    })??;

    serialize_key_package_hash_ref(&kp_result.inner, &client.context.mls_provider())
}

#[xmtp_common::test]
async fn test_key_package_rotation() {
    let alix_wallet = generate_local_wallet();
    let bo_wallet = generate_local_wallet();
    let alix = ClientBuilder::new_test_client(&alix_wallet).await;
    let bo = ClientBuilder::new_test_client(&bo_wallet).await;

    let alix_original_init_key = get_key_package_init_key(&alix, alix.installation_public_key())
        .await
        .unwrap();
    let bo_original_init_key = get_key_package_init_key(&bo, bo.installation_public_key())
        .await
        .unwrap();

    let alix_fetched_identity: StoredIdentity = alix.context.db().fetch(&()).unwrap().unwrap();
    assert!(alix_fetched_identity.next_key_package_rotation_ns.is_some());
    let bo_fetched_identity: StoredIdentity = bo.context.db().fetch(&()).unwrap().unwrap();
    assert!(bo_fetched_identity.next_key_package_rotation_ns.is_some());
    // Bo's original key should be deleted
    let bo_original_from_db = bo
        .db()
        .find_key_package_history_entry_by_hash_ref(bo_original_init_key.clone());
    assert!(bo_original_from_db.is_ok());

    alix.create_group_with_members(&[bo_wallet.identifier()], None, None)
        .await
        .unwrap();
    let bo_keys_queued_for_rotation = bo.context.db().is_identity_needs_rotation().unwrap();
    assert!(!bo_keys_queued_for_rotation);

    bo.sync_welcomes().await.unwrap();

    //check the rotation value has been set and less than Queue rotation interval
    let bo_fetched_identity: StoredIdentity = bo.context.db().fetch(&()).unwrap().unwrap();
    assert!(bo_fetched_identity.next_key_package_rotation_ns.is_some());
    let updated_at = bo
        .context
        .db()
        .key_package_rotation_history()
        .into_iter()
        .map(|(_, updated_at)| updated_at)
        .next_back()
        .unwrap();
    assert!(bo_fetched_identity.next_key_package_rotation_ns.unwrap() - updated_at < 5 * NS_IN_SEC);

    //check original keys must not be marked to be deleted
    let bo_keys = bo
        .context
        .db()
        .find_key_package_history_entry_by_hash_ref(bo_original_init_key.clone());
    assert!(bo_keys.unwrap().delete_at_ns.is_none());
    //wait for worker to rotate the keypackage
    xmtp_common::time::sleep(std::time::Duration::from_secs(11)).await;
    //check the rotation queue must be cleared
    let bo_keys_queued_for_rotation = bo.context.db().is_identity_needs_rotation().unwrap();
    assert!(!bo_keys_queued_for_rotation);

    let bo_fetched_identity: StoredIdentity = bo.context.db().fetch(&()).unwrap().unwrap();
    assert!(bo_fetched_identity.next_key_package_rotation_ns.unwrap() > 0);

    let bo_new_key = get_key_package_init_key(&bo, bo.installation_public_key())
        .await
        .unwrap();
    // Bo's key should have changed
    assert_ne!(bo_original_init_key, bo_new_key);

    // Depending on timing, old key should already be deleted, or marked to be deleted
    let bo_keys = bo
        .context
        .db()
        .find_key_package_history_entry_by_hash_ref(bo_original_init_key.clone())
        .ok();
    if let Some(key) = bo_keys {
        assert!(key.delete_at_ns.is_some());
    }

    xmtp_common::time::sleep(std::time::Duration::from_secs(10)).await;
    let bo_keys = bo
        .context
        .db()
        .find_key_package_history_entry_by_hash_ref(bo_original_init_key.clone());
    assert!(bo_keys.is_err());

    bo.sync_welcomes().await.unwrap();
    let bo_new_key_2 = get_key_package_init_key(&bo, bo.installation_public_key())
        .await
        .unwrap();
    // Bo's key should not have changed syncing the second time.
    assert_eq!(bo_new_key, bo_new_key_2);

    let alix_keys_queued_for_rotation = alix.context.db().is_identity_needs_rotation().unwrap();
    assert!(!alix_keys_queued_for_rotation);

    alix.sync_welcomes().await.unwrap();
    let alix_key_2 = get_key_package_init_key(&alix, alix.installation_public_key())
        .await
        .unwrap();

    // Alix's key should not have changed at all
    assert_eq!(alix_original_init_key, alix_key_2);

    alix.create_group_with_members(&[bo_wallet.identifier()], None, None)
        .await
        .unwrap();
    bo.sync_welcomes().await.unwrap();

    // Bo should have two groups now
    let bo_groups = bo.find_groups(GroupQueryArgs::default()).unwrap();
    assert_eq!(bo_groups.len(), 2);

    // Bo's original key should be deleted
    let bo_original_after_delete = bo
        .db()
        .find_key_package_history_entry_by_hash_ref(bo_original_init_key);
    assert!(bo_original_after_delete.is_err());
}

#[xmtp_common::test]
async fn test_find_or_create_dm_by_inbox_id() {
    let user1 = generate_local_wallet();
    let user2 = generate_local_wallet();
    let client1 = ClientBuilder::new_test_client(&user1).await;
    let client2 = ClientBuilder::new_test_client(&user2).await;

    // First call should create a new DM
    let dm1 = client1
        .find_or_create_dm_by_inbox_id(client2.inbox_id().to_string(), None)
        .await
        .unwrap();

    // Verify DM was created with correct properties
    let metadata = dm1.metadata().await.unwrap();
    assert_eq!(
        metadata.dm_members.clone().unwrap().member_one_inbox_id,
        client1.inbox_id()
    );
    assert_eq!(
        metadata.dm_members.unwrap().member_two_inbox_id,
        client2.inbox_id()
    );

    // Second call should find the existing DM
    let dm2 = client1
        .find_or_create_dm_by_inbox_id(client2.inbox_id().to_string(), None)
        .await
        .unwrap();

    // Verify we got back the same DM
    assert_eq!(dm1.group_id, dm2.group_id);
    assert_eq!(dm1.created_at_ns, dm2.created_at_ns);

    // Verify the DM appears in conversations list
    let conversations = client1.find_groups(GroupQueryArgs::default()).unwrap();
    assert_eq!(conversations.len(), 1);
    assert_eq!(conversations[0].group_id, dm1.group_id);
}

#[xmtp_common::test(unwrap_try = true)]
async fn should_stream_consent() {
    let alix = Tester::builder().sync_worker().build().await;
    let bo = Tester::new().await;

    let receiver = alix.local_events.subscribe();
    let stream = receiver.stream_consent_updates();
    futures::pin_mut!(stream);

    let group = alix
        .create_group_with_inbox_ids(&[bo.inbox_id().to_string()], None, None)
        .await
        .unwrap();
    xmtp_common::time::sleep(std::time::Duration::from_millis(500)).await;

    // first record is denied consent to the group.
    group.update_consent_state(ConsentState::Denied).unwrap();

    xmtp_common::time::sleep(std::time::Duration::from_millis(500)).await;

    // second is allowing consent for the group
    alix.set_consent_states(&[StoredConsentRecord {
        entity: hex::encode(&group.group_id),
        state: ConsentState::Allowed,
        entity_type: ConsentType::ConversationId,
        consented_at_ns: now_ns(),
    }])
    .await
    .unwrap();

    xmtp_common::time::sleep(std::time::Duration::from_millis(500)).await;

    // third allowing consent for bo inbox id
    alix.set_consent_states(&[StoredConsentRecord {
        entity: bo.inbox_id().to_string(),
        entity_type: ConsentType::InboxId,
        state: ConsentState::Allowed,
        consented_at_ns: now_ns(),
    }])
    .await
    .unwrap();

    // First consent update from creating the group
    let item = stream.next().await??;
    assert_eq!(item.len(), 1);
    assert_eq!(item[0].entity_type, ConsentType::ConversationId);
    assert_eq!(item[0].entity, hex::encode(&group.group_id));
    assert_eq!(item[0].state, ConsentState::Allowed);

    let item = stream.next().await??;
    assert_eq!(item.len(), 1);
    assert_eq!(item[0].entity_type, ConsentType::ConversationId);
    assert_eq!(item[0].entity, hex::encode(&group.group_id));
    assert_eq!(item[0].state, ConsentState::Denied);

    let item = stream.next().await??;
    assert_eq!(item.len(), 1);
    assert_eq!(item[0].entity_type, ConsentType::ConversationId);
    assert_eq!(item[0].entity, hex::encode(group.group_id));
    assert_eq!(item[0].state, ConsentState::Allowed);

    let item = stream.next().await??;
    assert_eq!(item.len(), 1);
    assert_eq!(item[0].entity_type, ConsentType::InboxId);
    assert_eq!(item[0].entity, bo.inbox_id());
    assert_eq!(item[0].state, ConsentState::Allowed);
}

#[rstest::rstest]
#[xmtp_common::test(unwrap_try = true)]
// Set to 50 seconds to safely account for the 16 second keepalive interval and 10 second timeout
#[timeout(Duration::from_secs(50))]
#[cfg_attr(any(target_arch = "wasm32"), ignore)]
async fn should_reconnect() {
    toxiproxy_test(async || {
        let alix = Tester::builder().proxy().build().await;
        let bo = Tester::builder().build().await;

        let start_new_convo = || async {
            bo.create_group_with_inbox_ids(&[alix.inbox_id().to_string()], None, None)
                .await
                .unwrap()
        };

        let stream = alix.client.stream_conversations(None, false).await.unwrap();
        futures::pin_mut!(stream);

        start_new_convo().await;

        let success_res = stream.try_next().await;
        assert!(success_res.is_ok());

        // Black hole the connection for a minute, then reconnect. The test will timeout without the keepalives.
        alix.for_each_proxy(async |p| {
            p.with_timeout("downstream".into(), 60_000, 1.0).await;
        })
        .await;

        start_new_convo().await;

        let should_fail = stream.try_next().await;
        assert!(should_fail.is_err());

        start_new_convo().await;

        alix.for_each_proxy(async |p| {
            p.delete_all_toxics().await.unwrap();
        })
        .await;
        xmtp_common::time::sleep(std::time::Duration::from_millis(500)).await;

        // stream closes after it gets the broken pipe b/c of blackhole & HTTP/2 KeepAlive
        futures_test::assert_stream_done!(stream);
        xmtp_common::time::sleep(std::time::Duration::from_millis(100)).await;
        let mut new_stream = alix.client.stream_conversations(None, false).await.unwrap();
        let new_res = new_stream.try_next().await;
        assert!(new_res.is_ok());
        assert!(new_res.unwrap().is_some());
    })
    .await
}

#[rstest::rstest]
#[xmtp_common::test(unwrap_try = true)]
async fn test_list_conversations_pagination() {
    use prost::Message;
    use xmtp_mls_common::group::GroupMetadataOptions;

    let alix = Tester::builder().build().await;
    let bo = Tester::builder().build().await;

    // Create 15 groups with small delays to ensure different created_at_ns values
    let mut all_group_ids = Vec::new();
    for i in 0..15 {
        let group = alix
            .create_group_with_inbox_ids(
                &[bo.inbox_id().to_string()],
                None,
                Some(GroupMetadataOptions {
                    name: Some(format!("Group {}", i + 1)),
                    ..Default::default()
                }),
            )
            .await
            .unwrap();
        all_group_ids.push(group.group_id.clone());
        group
            .send_message(
                TextCodec::encode("hello".to_string())
                    .unwrap()
                    .encode_to_vec()
                    .as_slice(),
                SendMessageOpts::default(),
            )
            .await
            .unwrap();
        // Small delay to ensure different timestamps
        xmtp_common::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    let mut before_ns = None;
    let mut all_conversation_ids = Vec::new();
    loop {
        let results = alix
            .list_conversations(GroupQueryArgs {
                limit: Some(5),
                last_activity_before_ns: before_ns,
                ..Default::default()
            })
            .unwrap();

        if results.is_empty() {
            break;
        }
        assert_eq!(results.len(), 5);

        all_conversation_ids.extend(results.iter().map(|item| item.group.group_id.clone()));

        before_ns = Some(
            results
                .last()
                .unwrap()
                .last_message
                .as_ref()
                .unwrap()
                .sent_at_ns,
        );
    }

    assert_eq!(
        all_conversation_ids.len(),
        15,
        "Should have 15 total conversations"
    );
    all_conversation_ids.dedup();

    // Check that we got all 15 unique groups
    assert_eq!(
        all_conversation_ids.len(),
        15,
        "Should have 15 total conversations after deduping"
    );
}

#[xmtp_common::test]
async fn test_delete_message() {
    let alix = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let bo = ClientBuilder::new_test_client(&generate_local_wallet()).await;

    // Create a group with both users
    let group = alix
        .create_group_with_inbox_ids(&[bo.inbox_id().to_string()], None, None)
        .await
        .unwrap();

    // Send a message
    let message_id = group
        .send_message(
            TextCodec::encode("test message".to_string())
                .unwrap()
                .encode_to_vec()
                .as_slice(),
            SendMessageOpts::default(),
        )
        .await
        .unwrap();

    // Verify the message exists
    let message = alix.message(message_id.clone()).unwrap();
    assert_eq!(message.id, message_id);

    // Delete the message
    let deleted_count = alix.delete_message(message_id.clone()).unwrap();
    assert_eq!(deleted_count, 1, "Should delete exactly 1 message");

    // Verify the message no longer exists
    let result = alix.message(message_id.clone());
    assert!(result.is_err(), "Message should not exist after deletion");

    // Test idempotency - deleting again should not error and return 0
    let deleted_count = alix.delete_message(message_id).unwrap();
    assert_eq!(
        deleted_count, 0,
        "Deleting non-existent message should return 0"
    );
}
