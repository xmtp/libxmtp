use super::*;
use crate::{Client, XmtpApi};
use xmtp_id::scw_verifier::SmartContractSignatureVerifier;

impl<ApiClient, V> Client<ApiClient, V>
where
    ApiClient: XmtpApi + Clone,
    V: SmartContractSignatureVerifier + Clone,
{
    pub async fn send_consent_sync_request(&self) -> Result<(String, String), DeviceSyncError> {
        let request = DeviceSyncRequest::new(DeviceSyncKind::Consent);
        self.send_sync_request(&self.mls_provider()?, request).await
    }

    pub async fn reply_to_consent_sync_request(
        &self,
        provider: &XmtpOpenMlsProvider,
    ) -> Result<DeviceSyncReplyProto, DeviceSyncError> {
        let conn = provider.conn_ref();
        let (_msg, request) = self
            .pending_sync_request(provider, DeviceSyncKind::Consent)
            .await?;

        let consent_records = self.syncable_consent_records(conn)?;

        let reply = self
            .create_sync_reply(
                &request.request_id,
                &[consent_records],
                DeviceSyncKind::Consent,
            )
            .await?;
        self.send_sync_reply(provider, reply.clone()).await?;

        Ok(reply)
    }

    pub async fn process_consent_sync_reply(
        &self,
        provider: &XmtpOpenMlsProvider,
    ) -> Result<(), DeviceSyncError> {
        self.process_sync_reply(provider, DeviceSyncKind::Consent)
            .await
    }

    fn syncable_consent_records(
        &self,
        conn: &DbConnection,
    ) -> Result<Vec<Syncable>, DeviceSyncError> {
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
        storage::consent_record::{ConsentState, ConsentType},
    };
    use mockito;
    use xmtp_cryptography::utils::generate_local_wallet;
    use xmtp_id::InboxOwner;

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
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
            .with_body("12345")
            .create();

        let history_sync_url =
            format!("http://{}:{}", HISTORY_SERVER_HOST, HISTORY_SERVER_PORT + 1);

        let wallet = generate_local_wallet();
        let mut amal_a = ClientBuilder::new_test_client(&wallet).await;
        amal_a.history_sync_url = Some(history_sync_url.clone());
        let amal_a_provider = amal_a.mls_provider().unwrap();
        let amal_a_conn = amal_a_provider.conn_ref();

        // create an alix installation and consent with alix
        let alix_wallet = generate_local_wallet();
        let consent_record = StoredConsentRecord::new(
            ConsentType::Address,
            ConsentState::Allowed,
            alix_wallet.get_address(),
        );
        amal_a.set_consent_states(&[consent_record]).await.unwrap();

        // Ensure that consent record now exists.
        let syncable_consent_records = amal_a.syncable_consent_records(&amal_a_conn).unwrap();
        assert_eq!(syncable_consent_records.len(), 1);

        // The first installation should have zero sync groups.
        let amal_a_sync_groups = amal_a.store().conn().unwrap().latest_sync_group().unwrap();
        assert!(amal_a_sync_groups.is_none());

        // Create a second installation for amal.
        let amal_b = ClientBuilder::new_test_client(&wallet).await;
        let amal_b_provider = amal_b.mls_provider().unwrap();
        let amal_b_conn = amal_b_provider.conn_ref();
        // Turn on history sync for the second installation.
        assert_ok!(amal_b.enable_history_sync(&amal_b_provider).await);
        // Check for new welcomes to new groups in the first installation (should be welcomed to a new sync group from amal_b).
        amal_a.sync_welcomes().await.expect("sync_welcomes");
        // Have the second installation request for a consent sync.
        let (_group_id, _pin_code) = amal_b
            .send_consent_sync_request()
            .await
            .expect("history request");

        // The first installation should now be a part of the sync group created by the second installation.
        let amal_a_sync_groups = amal_a.store().conn().unwrap().latest_sync_group().unwrap();
        assert!(amal_a_sync_groups.is_some());

        // Have first installation reply.
        // This is to make sure it finds the request in its sync group history,
        // verifies the pin code,
        // has no problem packaging the consent records,
        // and sends a reply message to the first installation.
        let reply = amal_a
            .reply_to_consent_sync_request(&amal_a_provider)
            .await
            .unwrap();

        // recreate the encrypted payload that was uploaded to our mock server using the same encryption key...
        let (enc_payload, _key) = encrypt_syncables_with_key(
            &[amal_a.syncable_consent_records(amal_a_conn).unwrap()],
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

        // The second installation has consented to nobody
        let consent_records = amal_b.store().conn().unwrap().consent_records().unwrap();
        assert_eq!(consent_records.len(), 0);

        // Have the second installation process the reply.
        amal_b
            .process_consent_sync_reply(&amal_b_provider)
            .await
            .unwrap();

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
}
