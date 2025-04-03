use super::*;
use crate::Client;
use crate::XmtpApi;
use xmtp_db::group::GroupQueryArgs;
use xmtp_db::group::StoredGroup;
use xmtp_db::group_message::MsgQueryArgs;
use xmtp_db::DbConnection;
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
        let groups = conn.find_groups(GroupQueryArgs::default())?;

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

#[cfg(test)]
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);
    use wasm_bindgen_test::wasm_bindgen_test;

    use super::*;

    use crate::{
        builder::ClientBuilder,
        groups::GroupMetadataOptions,
        utils::{test::wait_for_min_intents, Tester},
    };
    use xmtp_common::assert_ok;
    use xmtp_cryptography::utils::generate_local_wallet;

    #[xmtp_common::test]
    #[cfg_attr(target_family = "wasm", ignore)]
    async fn test_message_history_sync() {
        let amal_a = Tester::new().await;

        // Create an alix client.
        let alix = Tester::new().await;

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
        let syncable_groups = amal_a.syncable_groups(amal_a.provider.conn_ref()).unwrap();
        assert_eq!(syncable_groups.len(), 1);
        let syncable_messages = amal_a
            .syncable_messages(amal_a.provider.conn_ref())
            .unwrap();
        assert_eq!(syncable_messages.len(), 2); // welcome message, and message that was just sent

        // Create a second installation for amal.
        let amal_b = amal_a.clone().await;

        let groups_b = amal_b.syncable_groups(amal_b.provider.conn_ref()).unwrap();
        assert_eq!(groups_b.len(), 0);

        // make sure amal's worker has time to sync
        // 3 Intents:
        //  1.) UpdateGroupMembership Intent for new sync group
        //  2.) Device Sync Request
        //  3.) MessageHistory Sync Request
        wait_for_min_intents(amal_b.provider.conn_ref(), 3).await;
        tracing::info!("Waiting for intents published");

        let old_group_id = amal_a.get_sync_group(&amal_a.provider).unwrap().group_id;
        // Check for new welcomes to new groups in the first installation (should be welcomed to a new sync group from amal_b).
        amal_a
            .sync_welcomes(&amal_a.provider)
            .await
            .expect("sync_welcomes");
        let new_group_id = amal_a.get_sync_group(&amal_a.provider).unwrap().group_id;
        // group id should have changed to the new sync group created by the second installation
        assert_ne!(old_group_id, new_group_id);

        // Have the second installation request for a consent sync.
        amal_b
            .v1_send_sync_request(&amal_b.provider, DeviceSyncKind::MessageHistory)
            .await
            .unwrap();

        // Have amal_a receive the message (and auto-process)
        let amal_a_sync_group = amal_a.get_sync_group(&amal_a.provider).unwrap();
        assert_ok!(amal_a_sync_group.sync_with_conn(&amal_a.provider).await);

        xmtp_common::wait_for_some(|| async {
            amal_b
                .v1_get_latest_sync_reply(&amal_b.provider, DeviceSyncKind::MessageHistory)
                .await
                .unwrap()
        })
        .await
        .unwrap();

        xmtp_common::wait_for_eq(
            || {
                let groups_a = amal_a
                    .syncable_groups(amal_a.provider.conn_ref())
                    .unwrap()
                    .len();
                let groups_b = amal_b
                    .syncable_groups(amal_b.provider.conn_ref())
                    .unwrap()
                    .len();
                let messages_a = amal_a
                    .syncable_messages(amal_a.provider.conn_ref())
                    .unwrap()
                    .len();
                let messages_b = amal_b
                    .syncable_messages(amal_b.provider.conn_ref())
                    .unwrap()
                    .len();
                futures::future::ready(groups_a != groups_b || messages_a != messages_b)
            },
            true,
        )
        .await
        .unwrap();
    }

    #[xmtp_common::test]
    async fn test_sync_continues_during_db_disconnect() {
        let amal_a = Tester::new().await;

        // make sure amal's worker has time to sync
        // 3 Intents:
        //  1.) UpdateGroupMembership Intent for new sync group
        //  2.) Device Sync Request
        //  3.) MessageHistory Sync Request
        wait_for_min_intents(amal_a.provider.conn_ref(), 3).await;
        tracing::info!("Waiting for intents published");
        let old_group_id = amal_a.get_sync_group(&amal_a.provider).unwrap().group_id;

        // let old_group_id = amal_a.get_sync_group(amal_a_conn).unwrap().group_id;
        tracing::info!("Disconnecting");
        amal_a.release_db_connection().unwrap();

        // Create a second installation for amal.
        let amal_b = amal_a.clone().await;

        let groups_b = amal_b.syncable_groups(amal_b.provider.conn_ref()).unwrap();
        assert_eq!(groups_b.len(), 0);

        // make sure amal's worker has time to sync
        // 3 Intents:
        //  1.) UpdateGroupMembership Intent for new sync group
        //  2.) Device Sync Request
        //  3.) MessageHistory Sync Request
        wait_for_min_intents(amal_b.provider.conn_ref(), 3).await;
        tracing::info!("Waiting for intents published");

        // Have the second installation request for a consent sync.
        amal_b
            .v1_send_sync_request(&amal_b.provider, DeviceSyncKind::MessageHistory)
            .await
            .unwrap();

        amal_a.reconnect_db().unwrap();

        // make sure amal's worker has time to sync
        // 2 Intents:
        //  1.) Device Sync Request
        //  2.) MessageHistory Sync Request
        wait_for_min_intents(amal_a.provider.conn_ref(), 2).await;
        tracing::info!("Waiting for intents published");

        // Check for new welcomes to new groups in the first installation (should be welcomed to a new sync group from amal_b).
        amal_a
            .sync_welcomes(&amal_a.provider)
            .await
            .expect("sync_welcomes");
        let new_group_id = amal_a.get_sync_group(&amal_a.provider).unwrap().group_id;
        // group id should have changed to the new sync group created by the second installation
        assert_ne!(old_group_id, new_group_id);
    }

    #[xmtp_common::test]
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

    #[wasm_bindgen_test(unsupported = test)]
    fn test_new_pin() {
        let pin = new_pin();
        assert!(pin.chars().all(|c| c.is_numeric()));
        assert_eq!(pin.len(), 4);
    }

    #[wasm_bindgen_test(unsupported = test)]
    fn test_new_request_id() {
        let request_id = new_request_id();
        assert_eq!(request_id.len(), ENC_KEY_SIZE);
    }

    #[wasm_bindgen_test(unsupported = test)]
    fn test_new_key() {
        let sig_key = DeviceSyncKeyType::new_aes_256_gcm_key();
        let enc_key = DeviceSyncKeyType::new_aes_256_gcm_key();
        assert_eq!(sig_key.len(), ENC_KEY_SIZE);
        assert_eq!(enc_key.len(), ENC_KEY_SIZE);
        // ensure keys are different (seed isn't reused)
        assert_ne!(sig_key, enc_key);
    }

    #[wasm_bindgen_test(unsupported = test)]
    fn test_generate_nonce() {
        let nonce_1 = generate_nonce();
        let nonce_2 = generate_nonce();
        assert_eq!(nonce_1.len(), NONCE_SIZE);
        // ensure nonces are different (seed isn't reused)
        assert_ne!(nonce_1, nonce_2);
    }
}
