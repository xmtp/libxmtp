use super::*;
use crate::storage::group_message::MsgQueryArgs;
use crate::storage::key_value_store::{KVStore, Key};
use crate::XmtpApi;
use crate::{storage::group::StoredGroup, Client};
use xmtp_id::scw_verifier::SmartContractSignatureVerifier;

impl<ApiClient, V> Client<ApiClient, V>
where
    ApiClient: XmtpApi + Clone,
    V: SmartContractSignatureVerifier + Clone,
{
    // returns (request_id, pin_code)
    pub async fn send_history_request(&self) -> Result<(String, String), DeviceSyncError> {
        let request = DeviceSyncRequest::new(DeviceSyncKind::MessageHistory);
        self.send_sync_request(request).await
    }

    pub async fn reply_to_history_request(
        &self,
        pin_code: &str,
    ) -> Result<DeviceSyncReplyProto, DeviceSyncError> {
        let Some((_msg, request)) = self
            .pending_sync_request(DeviceSyncKind::MessageHistory)
            .await?
        else {
            return Err(DeviceSyncError::NoPendingRequest);
        };

        self.verify_pin(&request.request_id, pin_code)?;

        let groups = self.syncable_groups()?;
        let messages = self.syncable_messages()?;

        let reply = self
            .send_syncables(&request.request_id, &[groups, messages])
            .await?;

        Ok(reply)
    }

    pub async fn process_message_history_reply(&self) -> Result<(), DeviceSyncError> {
        let conn = self.store().conn()?;
        // load the request_id
        let request_id: Option<String> = KVStore::get(&conn, &Key::MessageHistorySyncRequestId)
            .map_err(DeviceSyncError::Storage)?;
        let Some(request_id) = request_id else {
            return Err(DeviceSyncError::NoReplyToProcess);
        };

        // process the reply
        self.process_sync_reply(&request_id).await
    }

    fn syncable_groups(&self) -> Result<Vec<Syncable>, DeviceSyncError> {
        let conn = self.store().conn()?;
        let groups = conn
            .find_groups(None, None, None, None, Some(ConversationType::Group))?
            .into_iter()
            .map(Syncable::Group)
            .collect();
        Ok(groups)
    }

    fn syncable_messages(&self) -> Result<Vec<Syncable>, DeviceSyncError> {
        let conn = self.store().conn()?;
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
    async fn test_enable_history_sync() {
        let wallet = generate_local_wallet();
        let client = ClientBuilder::new_test_client(&wallet).await;
        assert_ok!(client.enable_history_sync().await);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    #[ignore]
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

        // Create an alix client.
        let alix_wallet = generate_local_wallet();
        let alix = ClientBuilder::new_test_client(&alix_wallet).await;

        // Have amal_a create a group and add alix to that group, then send a message.
        let group = amal_a
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        group.add_members_by_inbox_id(&[alix.inbox_id()]);

        // Ensure that consent record now exists.
        let syncable_consent_records = amal_a.syncable_consent_records().unwrap();
        assert_eq!(syncable_consent_records.len(), 1);

        // The first installation should have zero sync groups.
        let amal_a_sync_groups = amal_a.store().conn().unwrap().find_sync_groups().unwrap();
        assert_eq!(amal_a_sync_groups.len(), 0);

        // Create a second installation for amal.
        let amal_b = ClientBuilder::new_test_client(&wallet).await;
        // Turn on history sync for the second installation.
        assert_ok!(amal_b.enable_history_sync().await);
        // Check for new welcomes to new groups in the first installation (should be welcomed to a new sync group from amal_b).
        amal_a.sync_welcomes().await.expect("sync_welcomes");
        // Have the second installation request for a consent sync.
        let (_group_id, pin_code) = amal_b
            .send_consent_sync_request()
            .await
            .expect("history request");

        // The first installation should now be a part of the sync group created by the second installation.
        let amal_a_sync_groups = amal_a.store().conn().unwrap().find_sync_groups().unwrap();
        assert_eq!(amal_a_sync_groups.len(), 1);

        // Have first installation reply.
        // This is to make sure it finds the request in its sync group history,
        // verifies the pin code,
        // has no problem packaging the consent records,
        // and sends a reply message to the first installation.
        let reply = amal_a
            .reply_to_consent_sync_request(&pin_code)
            .await
            .unwrap();

        // recreate the encrypted payload that was uploaded to our mock server using the same encryption key...
        let (enc_payload, _key) = encrypt_syncables_with_key(
            &[amal_a.syncable_consent_records().unwrap()],
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

        // The second installatino has consented to nobody
        let consent_records = amal_b.store().conn().unwrap().consent_records().unwrap();
        assert_eq!(consent_records.len(), 0);

        // Have the second installation process the reply.
        amal_b.process_consent_sync_reply().await.unwrap();

        // Load consents of both installations
        let consent_records_a = amal_a.store().conn().unwrap().consent_records().unwrap();
        let consent_records_b = amal_b.store().conn().unwrap().consent_records().unwrap();

        // Ensure the consent is synced.
        assert_eq!(consent_records_a.len(), 2); // 2 consents - alix, and the group sync
        assert_eq!(consent_records_b.len(), 2);
        for record in &consent_records_a {
            assert!(consent_records_b.contains(record));
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

        let result = amal_a.syncable_groups().unwrap();
        assert_eq!(result.len(), 2);
    }

    #[tokio::test]
    async fn test_externals_cant_join_sync_group() {
        let wallet = generate_local_wallet();
        let amal = ClientBuilder::new_test_client(&wallet).await;
        assert_ok!(amal.enable_history_sync().await);
        amal.sync_welcomes().await.expect("sync welcomes");

        let external_wallet = generate_local_wallet();
        let external_client = ClientBuilder::new_test_client(&external_wallet).await;
        assert_ok!(external_client.enable_history_sync().await);
        external_client
            .sync_welcomes()
            .await
            .expect("sync welcomes");

        let amal_sync_groups = amal
            .store()
            .conn()
            .unwrap()
            .find_sync_groups()
            .expect("find sync groups");
        assert_eq!(amal_sync_groups.len(), 1);

        // try to join amal's sync group
        let sync_group_id = amal_sync_groups[0].id.clone();
        let created_at_ns = amal_sync_groups[0].created_at_ns;

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
        let sig_key = DeviceSyncKeyType::new_chacha20_poly1305_key();
        let enc_key = DeviceSyncKeyType::new_chacha20_poly1305_key();
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
