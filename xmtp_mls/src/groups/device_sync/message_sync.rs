use super::*;
use crate::storage::group_message::MsgQueryArgs;
use crate::XmtpApi;
use crate::{storage::group::StoredGroup, Client};
use xmtp_id::scw_verifier::SmartContractSignatureVerifier;

impl<ApiClient, V> Client<ApiClient, V>
where
    ApiClient: XmtpApi + Clone,
    V: SmartContractSignatureVerifier + Clone,
{
    // returns (request_id, pin_code)
    pub async fn send_history_sync_request(
        &self,
        provider: &XmtpOpenMlsProvider,
    ) -> Result<(String, String), DeviceSyncError> {
        let request = DeviceSyncRequest::new(DeviceSyncKind::MessageHistory);

        self.send_sync_request(provider, request).await
    }

    pub async fn reply_to_history_sync_request(
        &self,
        provider: &XmtpOpenMlsProvider,
    ) -> Result<DeviceSyncReplyProto, DeviceSyncError> {
        let conn = provider.conn_ref();
        let (_msg, request) = self
            .pending_sync_request(provider, DeviceSyncKind::MessageHistory)
            .await?;

        let groups = self.syncable_groups(conn)?;
        let messages = self.syncable_messages(conn)?;

        let reply = self
            .create_sync_reply(
                &request.request_id,
                &[groups, messages],
                DeviceSyncKind::MessageHistory,
            )
            .await?;
        self.send_sync_reply(provider, reply.clone()).await?;

        Ok(reply)
    }

    pub async fn process_history_sync_reply(
        &self,
        provider: &XmtpOpenMlsProvider,
    ) -> Result<(), DeviceSyncError> {
        self.process_sync_reply(provider, DeviceSyncKind::MessageHistory)
            .await
    }

    fn syncable_groups(&self, conn: &DbConnection) -> Result<Vec<Syncable>, DeviceSyncError> {
        let groups = conn
            .find_groups(None, None, None, None, Some(ConversationType::Group))?
            .into_iter()
            .map(Syncable::Group)
            .collect();
        Ok(groups)
    }

    fn syncable_messages(&self, conn: &DbConnection) -> Result<Vec<Syncable>, DeviceSyncError> {
        let groups = conn.find_groups(None, None, None, None, Some(ConversationType::Group))?;

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

    const HISTORY_SERVER_HOST: &str = "0.0.0.0";
    const HISTORY_SERVER_PORT: u16 = 5558;

    use super::*;
    use crate::{assert_ok, builder::ClientBuilder, groups::GroupMetadataOptions};
    use mockito;
    use xmtp_cryptography::utils::generate_local_wallet;
    use xmtp_id::InboxOwner;

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_message_history_sync() {
        let options = mockito::ServerOpts {
            host: HISTORY_SERVER_HOST,
            port: HISTORY_SERVER_PORT + 1,
            ..Default::default()
        };
        let mut server = mockito::Server::new_with_opts_async(options).await;

        let _m = server
            .mock("POST", "/upload")
            .with_status(201)
            .with_body("12345")
            .create();

        let history_sync_url =
            format!("http://{}:{}", HISTORY_SERVER_HOST, HISTORY_SERVER_PORT + 1);

        let wallet = generate_local_wallet();
        let mut amal_a = ClientBuilder::new_test_client(&wallet).await;
        amal_a.history_sync_url = Some(history_sync_url.clone());
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
        let syncable_groups = amal_a.syncable_groups(&amal_a_conn).unwrap();
        assert_eq!(syncable_groups.len(), 1);
        let syncable_messages = amal_a.syncable_messages(&amal_a_conn).unwrap();
        assert_eq!(syncable_messages.len(), 2); // welcome message, and message that was just sent

        // The first installation should have zero sync groups.
        let amal_a_sync_group = amal_a_conn.latest_sync_group().unwrap();
        assert!(amal_a_sync_group.is_none());

        // Create a second installation for amal.
        let amal_b = ClientBuilder::new_test_client(&wallet).await;
        let amal_b_provider = amal_b.mls_provider().unwrap();
        let amal_b_conn = amal_b_provider.conn_ref();
        // Turn on history sync for the second installation.
        assert_ok!(amal_b.enable_sync(&amal_b_provider).await);
        // Check for new welcomes to new groups in the first installation (should be welcomed to a new sync group from amal_b).
        amal_a
            .sync_welcomes(amal_a_conn)
            .await
            .expect("sync_welcomes");
        // Have the second installation request for a consent sync.
        let (_group_id, _pin_code) = amal_b
            .send_history_sync_request(&amal_b_provider)
            .await
            .expect("history request");

        // The first installation should now be a part of the sync group created by the second installation.
        let amal_a_sync_group = amal_a_conn.latest_sync_group().unwrap();
        assert!(amal_a_sync_group.is_some());

        // Have first installation reply.
        // This is to make sure it finds the request in its sync group history,
        // verifies the pin code,
        // has no problem packaging the consent records,
        // and sends a reply message to the first installation.
        let reply = amal_a
            .reply_to_history_sync_request(&amal_a_provider)
            .await
            .unwrap();

        // recreate the encrypted payload that was uploaded to our mock server using the same encryption key...
        let (enc_payload, _key) = encrypt_syncables_with_key(
            &[
                amal_a.syncable_groups(&amal_a_conn).unwrap(),
                amal_a.syncable_messages(&amal_a_conn).unwrap(),
            ],
            reply.encryption_key.unwrap().try_into().unwrap(),
        )
        .unwrap();

        // have the mock server reply with the payload
        let file_path = reply.url.replace(&history_sync_url, "");
        let _m = server
            .mock("GET", &*file_path)
            .with_status(200)
            .with_body(&enc_payload)
            .create();

        // The second installation has no groups
        assert_eq!(amal_b.syncable_groups(&amal_b_conn).unwrap().len(), 0);
        assert_eq!(amal_b.syncable_messages(&amal_b_conn).unwrap().len(), 0);

        // Have the second installation process the reply.
        amal_b
            .process_history_sync_reply(&amal_b_provider)
            .await
            .unwrap();

        // Load consents of both installations
        let groups_a = amal_a.syncable_groups(&amal_a_conn).unwrap();
        let groups_b = amal_b.syncable_groups(&amal_b_conn).unwrap();
        let messages_a = amal_a.syncable_messages(&amal_a_conn).unwrap();
        let messages_b = amal_b.syncable_messages(&amal_b_conn).unwrap();

        // Ensure the groups and messages are synced.
        assert_eq!(groups_a.len(), 1);
        assert_eq!(groups_b.len(), 1);
        for record in &groups_a {
            assert!(groups_b.contains(record));
        }
        assert_eq!(messages_a.len(), 2);
        assert_eq!(messages_b.len(), 2);
        for record in &messages_a {
            assert!(messages_b.contains(record));
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
        let amal = ClientBuilder::new_test_client(&wallet).await;
        assert_ok!(amal.enable_sync(&amal.mls_provider().unwrap()).await);
        amal.sync_welcomes(&amal.store().conn().unwrap())
            .await
            .expect("sync welcomes");

        let external_wallet = generate_local_wallet();
        let external_client = ClientBuilder::new_test_client(&external_wallet).await;
        assert_ok!(
            external_client
                .enable_sync(&external_client.mls_provider().unwrap())
                .await
        );
        external_client
            .sync_welcomes(&external_client.store().conn().unwrap())
            .await
            .expect("sync welcomes");

        let amal_sync_group = amal
            .store()
            .conn()
            .unwrap()
            .latest_sync_group()
            .expect("find sync group");
        assert!(amal_sync_group.is_some());
        let amal_sync_group = amal_sync_group.unwrap();

        // try to join amal's sync group
        let sync_group_id = amal_sync_group.id.clone();
        let created_at_ns = amal_sync_group.created_at_ns;

        let external_client_group = MlsGroup::new(
            external_client.clone(),
            sync_group_id.clone(),
            created_at_ns,
        );
        let result = external_client_group
            .add_members(&[external_wallet.get_address()])
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
