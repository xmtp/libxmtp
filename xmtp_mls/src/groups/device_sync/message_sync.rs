use super::*;
use crate::storage::group::GroupQueryArgs;
use crate::storage::group_message::MsgQueryArgs;
use crate::XmtpApi;
use crate::{storage::group::StoredGroup, Client};
use xmtp_id::scw_verifier::SmartContractSignatureVerifier;

impl<ApiClient, V> Client<ApiClient, V>
where
    ApiClient: XmtpApi,
    V: SmartContractSignatureVerifier,
{
    pub(super) fn syncable_groups(
        &self,
        conn: &DbConnection,
    ) -> Result<Vec<Syncable>, DeviceSyncError> {
        let groups = conn
            .find_groups(GroupQueryArgs::default())?
            .into_iter()
            .map(Syncable::Group)
            .collect();

        Ok(groups)
    }

    pub(super) fn syncable_messages(
        &self,
        conn: &DbConnection,
    ) -> Result<Vec<Syncable>, DeviceSyncError> {
        let groups =
            conn.find_groups(GroupQueryArgs::default().conversation_type(ConversationType::Group))?;

        let mut all_messages = vec![];
        for StoredGroup { id, .. } in groups.into_iter() {
            let messages = conn.get_group_messages(&id, &MsgQueryArgs::default())?;
            for msg in messages {
                all_messages.push(Syncable::GroupMessage(msg));
            }
        }

        Ok(all_messages)
    }
}

#[cfg(all(not(target_arch = "wasm32"), test))]
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    use super::*;
    use crate::{
        api::test_utils::wait_for_some,
        assert_ok,
        builder::ClientBuilder,
        groups::GroupMetadataOptions,
        utils::test::{wait_for_min_intents, HISTORY_SYNC_URL},
    };
    use std::time::{Duration, Instant};
    use xmtp_cryptography::utils::generate_local_wallet;
    use xmtp_id::InboxOwner;

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_message_history_sync() {
        let wallet = generate_local_wallet();
        let amal_a = ClientBuilder::new_test_client_with_history(&wallet, HISTORY_SYNC_URL).await;

        let amal_a_provider = amal_a.mls_provider().unwrap();
        let amal_a_conn = amal_a_provider.conn_ref();

        // Create an alix client.
        let alix_wallet = generate_local_wallet();
        let alix = ClientBuilder::new_test_client(&alix_wallet).await;

        // Have amal_a create a group and add alix to that group, then send a message.
        let group = amal_a
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        group
            .add_members_by_inbox_id(&[alix.inbox_id()])
            .await
            .unwrap();
        group.send_message(&[1, 2, 3]).await.unwrap();

        // Ensure that groups and messages now exists.
        let syncable_groups = amal_a.syncable_groups(amal_a_conn).unwrap();
        assert_eq!(syncable_groups.len(), 1);
        let syncable_messages = amal_a.syncable_messages(amal_a_conn).unwrap();
        assert_eq!(syncable_messages.len(), 2); // welcome message, and message that was just sent

        // Create a second installation for amal.
        let amal_b = ClientBuilder::new_test_client_with_history(&wallet, HISTORY_SYNC_URL).await;
        let amal_b_provider = amal_b.mls_provider().unwrap();
        let amal_b_conn = amal_b_provider.conn_ref();

        let groups_b = amal_b.syncable_groups(amal_b_conn).unwrap();
        assert_eq!(groups_b.len(), 0);

        // make sure amal's worker has time to sync
        // 3 Intents:
        //  1.) UpdateGroupMembership Intent for new sync group
        //  2.) Device Sync Request
        //  3.) MessageHistory Sync Request
        wait_for_min_intents(amal_b_conn, 3).await;
        tracing::info!("Waiting for intents published");

        let old_group_id = amal_a.get_sync_group(amal_a_conn).unwrap().group_id;
        // Check for new welcomes to new groups in the first installation (should be welcomed to a new sync group from amal_b).
        amal_a
            .sync_welcomes(amal_a_conn)
            .await
            .expect("sync_welcomes");
        let new_group_id = amal_a.get_sync_group(amal_a_conn).unwrap().group_id;
        // group id should have changed to the new sync group created by the second installation
        assert_ne!(old_group_id, new_group_id);

        // Have the second installation request for a consent sync.
        amal_b
            .send_sync_request(&amal_b_provider, DeviceSyncKind::MessageHistory)
            .await
            .unwrap();

        // Have amal_a receive the message (and auto-process)
        let amal_a_sync_group = amal_a.get_sync_group(amal_a_conn).unwrap();
        assert_ok!(amal_a_sync_group.sync_with_conn(&amal_a_provider).await);

        // Wait for up to 3 seconds for the reply on amal_b (usually is almost instant)
        let start = Instant::now();
        let mut reply = None;
        while reply.is_none() {
            reply = amal_b
                .get_latest_sync_reply(&amal_b_provider, DeviceSyncKind::MessageHistory)
                .await
                .unwrap();
            if start.elapsed() > Duration::from_secs(3) {
                panic!("Did not receive sync reply.");
            }
        }

        // Wait up to 3 seconds for sync to process (typically is almost instant)
        let [mut groups_a, mut groups_b, mut messages_a, mut messages_b] = [0; 4];
        let start = Instant::now();
        while groups_a != groups_b || messages_a != messages_b {
            groups_a = amal_a.syncable_groups(amal_a_conn).unwrap().len();
            groups_b = amal_b.syncable_groups(amal_b_conn).unwrap().len();
            messages_a = amal_a.syncable_messages(amal_a_conn).unwrap().len();
            messages_b = amal_b.syncable_messages(amal_b_conn).unwrap().len();

            if start.elapsed() > Duration::from_secs(3) {
                panic!("Message sync did not work. Groups: {groups_a}/{groups_b} | Messages: {messages_a}/{messages_b}");
            }
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_prepare_groups_to_sync() {
        let wallet = generate_local_wallet();
        let amal_a = ClientBuilder::new_test_client(&wallet).await;
        let _group_a = amal_a
            .create_group(None, GroupMetadataOptions::default())
            .expect("create group");
        let _group_b = amal_a
            .create_group(None, GroupMetadataOptions::default())
            .expect("create group");

        let result = amal_a
            .syncable_groups(&amal_a.store().conn().unwrap())
            .unwrap();
        assert_eq!(result.len(), 2);
    }

    #[tokio::test]
    async fn test_externals_cant_join_sync_group() {
        let wallet = generate_local_wallet();
        let amal = ClientBuilder::new_test_client_with_history(&wallet, HISTORY_SYNC_URL).await;
        amal.sync_welcomes(&amal.store().conn().unwrap())
            .await
            .expect("sync welcomes");

        let bo_wallet = generate_local_wallet();
        let bo_client =
            ClientBuilder::new_test_client_with_history(&bo_wallet, HISTORY_SYNC_URL).await;

        bo_client
            .sync_welcomes(&bo_client.store().conn().unwrap())
            .await
            .expect("sync welcomes");

        let amal_sync_group =
            wait_for_some(|| amal.store().conn().unwrap().latest_sync_group().unwrap()).await;

        assert!(amal_sync_group.is_some());

        let amal_sync_group = amal_sync_group.unwrap();

        // try to join amal's sync group
        let sync_group_id = amal_sync_group.id.clone();
        let created_at_ns = amal_sync_group.created_at_ns;

        let external_client_group =
            MlsGroup::new(bo_client.clone(), sync_group_id.clone(), created_at_ns);
        let result = external_client_group
            .add_members(&[bo_wallet.get_address()])
            .await;
        assert!(result.is_err());
    }

    #[test]
    fn test_new_pin() {
        let pin = new_pin();
        assert!(pin.chars().all(|c| c.is_numeric()));
        assert_eq!(pin.len(), 4);
    }

    #[test]
    fn test_new_request_id() {
        let request_id = new_request_id();
        assert_eq!(request_id.len(), ENC_KEY_SIZE);
    }

    #[test]
    fn test_new_key() {
        let sig_key = DeviceSyncKeyType::new_aes_256_gcm_key();
        let enc_key = DeviceSyncKeyType::new_aes_256_gcm_key();
        assert_eq!(sig_key.len(), ENC_KEY_SIZE);
        assert_eq!(enc_key.len(), ENC_KEY_SIZE);
        // ensure keys are different (seed isn't reused)
        assert_ne!(sig_key, enc_key);
    }

    #[test]
    fn test_generate_nonce() {
        let nonce_1 = generate_nonce();
        let nonce_2 = generate_nonce();
        assert_eq!(nonce_1.len(), NONCE_SIZE);
        // ensure nonces are different (seed isn't reused)
        assert_ne!(nonce_1, nonce_2);
    }
}
