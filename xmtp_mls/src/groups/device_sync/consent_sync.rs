use super::*;
use crate::{Client, XmtpApi};
use xmtp_id::scw_verifier::SmartContractSignatureVerifier;

impl<ApiClient, V> Client<ApiClient, V>
where
    ApiClient: XmtpApi,
    V: SmartContractSignatureVerifier,
{
    pub(super) fn syncable_consent_records(
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

    use std::{
        thread,
        time::{Duration, Instant},
    };

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
        assert_ok!(amal_a.enable_sync(&amal_a_provider).await);
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
        let syncable_consent_records = amal_a.syncable_consent_records(amal_a_conn).unwrap();
        assert_eq!(syncable_consent_records.len(), 1);

        // Create a second installation for amal with sync.
        let amal_b = ClientBuilder::new_test_client(&wallet).await;
        let amal_b_provider = amal_b.mls_provider().unwrap();
        let amal_b_conn = amal_b_provider.conn_ref();
        assert_ok!(amal_b.enable_sync(&amal_b_provider).await);

        let old_group_id = amal_a.get_sync_group().unwrap().group_id;

        // Check for new welcomes to new groups in the first installation (should be welcomed to a new sync group from amal_b).
        amal_a
            .sync_welcomes(amal_a_conn)
            .await
            .expect("sync_welcomes");

        let new_group_id = amal_a.get_sync_group().unwrap().group_id;
        // group id should have changed to the new sync group created by the second installation
        assert_ne!(old_group_id, new_group_id);

        // recreate the encrypted payload that was uploaded to our mock server using the same encryption key...
        let amal_a_syncables = amal_a.syncable_consent_records(amal_a_conn).unwrap();
        let amal_a_syncables_len = amal_a_syncables.len();
        tracing::info!("amal a syncables: {}", amal_a_syncables.len());
        let (enc_payload, _key) = encrypt_syncables_with_key(
            &[amal_a_syncables],
            // tests always give the same enc key
            DeviceSyncKeyType::new_aes_256_gcm_key(),
        )
        .unwrap();
        // have the mock server reply with the payload

        let _m = server
            .mock("GET", &*format!("/files/12345"))
            .with_status(200)
            .with_body(&enc_payload)
            .create();

        // Have the second installation request for a consent sync.
        amal_b
            .send_sync_request(&amal_b_provider, DeviceSyncKind::Consent)
            .await
            .unwrap();

        // Have amal_a receive the message (and auto-process)
        let amal_a_sync_group = amal_a.get_sync_group().unwrap();
        assert_ok!(amal_a_sync_group.sync_with_conn(&amal_a_provider).await);

        // Wait for up to 3 seconds for the reply on amal_b (usually is almost instant)
        let start = Instant::now();
        let mut reply = None;
        while reply.is_none() {
            reply = amal_b
                .sync_reply(&amal_b_provider, DeviceSyncKind::Consent)
                .await
                .unwrap();
            if start.elapsed() > Duration::from_secs(3) {
                panic!("Did not receive consent reply.");
            }
        }

        // Load consents of both installations
        amal_b.sync_welcomes(&amal_b_conn).await.unwrap();
        let consent_records_a = amal_a.syncable_consent_records(&amal_a_conn).unwrap();
        let consent_records_b = amal_b.syncable_consent_records(&amal_b_conn).unwrap();

        // Ensure the consent is synced.

        assert_eq!(consent_records_a.len(), 2); // 2 consents - alix, and the group sync
        assert_eq!(consent_records_b.len(), amal_a_syncables_len);
        for record in &consent_records_b {
            assert!(consent_records_a.contains(record));
        }
    }
}
