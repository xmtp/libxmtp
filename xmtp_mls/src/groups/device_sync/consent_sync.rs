use super::*;
use crate::{
    storage::key_value_store::{KVStore, Key},
    Client, XmtpApi,
};
use xmtp_id::scw_verifier::SmartContractSignatureVerifier;

impl<ApiClient, V> Client<ApiClient, V>
where
    ApiClient: XmtpApi + Clone,
    V: SmartContractSignatureVerifier + Clone,
{
    pub async fn send_consent_sync_request(&self) -> Result<(String, String), DeviceSyncError> {
        let request = DeviceSyncRequest::new(DeviceSyncKind::Consent);
        self.send_sync_request(request).await
    }

    pub async fn reply_to_consent_sync_request(
        &self,
        pin_code: &str,
    ) -> Result<DeviceSyncReplyProto, DeviceSyncError> {
        let Some((_msg, request)) = self.pending_sync_request(DeviceSyncKind::Consent).await?
        else {
            return Err(DeviceSyncError::NoPendingRequest);
        };

        self.verify_pin(&request.request_id, pin_code)?;

        let consent_records = self.syncable_consent_records()?;

        let reply = self
            .send_syncables(&request.request_id, &[consent_records])
            .await?;

        Ok(reply)
    }

    async fn _process_consent_sync_reply(&self) -> Result<(), DeviceSyncError> {
        let conn = self.store().conn()?;

        // load the request_id
        let request_id: Option<String> =
            KVStore::get(&conn, &Key::ConsentSyncRequestId).map_err(DeviceSyncError::Storage)?;
        let Some(request_id) = request_id else {
            return Err(DeviceSyncError::NoReplyToProcess);
        };

        // process the reply
        self.process_sync_reply(&request_id).await
    }

    fn syncable_consent_records(&self) -> Result<Vec<Syncable>, DeviceSyncError> {
        let conn = self.store().conn()?;
        let consent_records = conn
            .consent_records()?
            .into_iter()
            .map(Syncable::ConsentRecord)
            .collect();
        Ok(consent_records)
    }
}

#[cfg(all(not(target_arch = "wasm32"), test))]
pub(crate) mod tests {
    const HISTORY_SERVER_HOST: &str = "0.0.0.0";
    const HISTORY_SERVER_PORT: u16 = 5558;

    use super::*;
    use crate::{
        assert_ok,
        builder::ClientBuilder,
        groups::GroupMetadataOptions,
        storage::consent_record::{ConsentState, ConsentType},
    };
    use mockito;
    use xmtp_cryptography::utils::generate_local_wallet;
    use xmtp_id::InboxOwner;

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    #[ignore]
    async fn test_consent_sync() {
        let options = mockito::ServerOpts {
            host: HISTORY_SERVER_HOST,
            port: HISTORY_SERVER_PORT + 1,
            ..Default::default()
        };
        let mut server = mockito::Server::new_with_opts_async(options).await;

        let _m = server
            .mock("POST", "/upload")
            .with_status(201)
            .with_body("File uploaded")
            .create();

        let history_sync_url = format!(
            "http://{}:{}/upload",
            HISTORY_SERVER_HOST,
            HISTORY_SERVER_PORT + 1
        );

        let wallet = generate_local_wallet();
        let amal_a = ClientBuilder::new_test_client(&wallet).await;
        let _group_a = amal_a
            .create_group(None, GroupMetadataOptions::default())
            .expect("create group");

        let alix_wallet = generate_local_wallet();
        // let alix = ClientBuilder::new_test_client(&alix_wallet).await;
        let consent_record = StoredConsentRecord::new(
            ConsentType::Address,
            ConsentState::Allowed,
            alix_wallet.get_address(),
        );

        let syncable_consent_records = amal_a.syncable_consent_records().unwrap();

        let (enc_content, enc_key) = encrypt_syncables(&[syncable_consent_records]).unwrap();

        let _m = server
            .mock("GET", "/upload")
            .with_status(201)
            .with_body(&enc_content)
            .create();

        let wallet = generate_local_wallet();
        let mut amal_a = ClientBuilder::new_test_client(&wallet).await;
        amal_a.history_sync_url = Some(history_sync_url.clone());
        let amal_b = ClientBuilder::new_test_client(&wallet).await;
        assert_ok!(amal_b.enable_history_sync().await);

        amal_a.sync_welcomes().await.expect("sync_welcomes");

        // amal_b sends a message history request to sync group messages
        let (_group_id, _pin_code) = amal_b
            .send_history_request()
            .await
            .expect("history request");

        let amal_a_sync_groups = amal_a.store().conn().unwrap().find_sync_groups().unwrap();
        assert_eq!(amal_a_sync_groups.len(), 1);
        // get the first sync group
        let amal_a_sync_group = amal_a.group(amal_a_sync_groups[0].id.clone()).unwrap();
        amal_a_sync_group.sync().await.expect("sync");

        // amal_a builds and sends a message history reply back
        let history_reply = DeviceSyncReply::new(&new_request_id(), &history_sync_url, enc_key);
        amal_a
            .send_sync_reply(history_reply.into())
            .await
            .expect("send reply");

        amal_a_sync_group.sync().await.expect("sync");
        // amal_b should have received the reply
        let amal_b_sync_groups = amal_b.store().conn().unwrap().find_sync_groups().unwrap();
        assert_eq!(amal_b_sync_groups.len(), 1);

        let amal_b_sync_group = amal_b.group(amal_b_sync_groups[0].id.clone()).unwrap();
        amal_b_sync_group.sync().await.expect("sync");

        let amal_b_conn = amal_b.store().conn().unwrap();
        let amal_b_messages = amal_b_conn
            .get_group_messages(&amal_b_sync_group.group_id, &MsgQueryArgs::default())
            .unwrap();

        // there should be two messages in the sync group
        assert_eq!(amal_b_messages.len(), 2);

        // first a request
        let request_msg = &amal_b_messages[0];
        let content: DeviceSyncContent =
            serde_json::from_slice(&request_msg.decrypted_message_bytes).unwrap();

        let DeviceSyncContent::Request(_request) = content else {
            panic!("should be a request");
        };

        // then a reply
        let reply_msg = &amal_b_messages[1];
        let content: DeviceSyncContent =
            serde_json::from_slice(&reply_msg.decrypted_message_bytes).unwrap();
        let DeviceSyncContent::Reply(_reply) = content else {
            panic!("should be a reply");
        };
    }
}
