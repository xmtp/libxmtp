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

#[cfg(test)]
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    use crate::{
        groups::{device_sync::handle::SyncMetric, scoped_client::ScopedGroupClient},
        utils::tester::{Tester, XmtpClientWalletTester},
    };
    use xmtp_db::consent_record::{ConsentState, ConsentType, StoredConsentRecord};

    use xmtp_cryptography::utils::generate_local_wallet;
    use xmtp_id::associations::test_utils::WalletTestExt;

    #[xmtp_common::test]
    #[cfg_attr(target_family = "wasm", ignore)]
    async fn test_consent_sync() {
        let amal_a = Tester::new().await;

        // create an alix installation and consent with alix
        let alix_wallet = generate_local_wallet();
        let consent_record = StoredConsentRecord::new(
            ConsentType::InboxId,
            ConsentState::Allowed,
            alix_wallet.get_inbox_id(0),
        );

        amal_a.set_consent_states(&[consent_record]).await.unwrap();

        // Ensure that consent record now exists.
        let syncable_consent_records = amal_a
            .syncable_consent_records(amal_a.provider.conn_ref())
            .unwrap();
        assert_eq!(syncable_consent_records.len(), 1);

        // Create a second installation for amal with sync.
        let amal_b = amal_a.clone().await;

        amal_b
            .worker()
            .wait(SyncMetric::V1RequestSent, 1)
            .await
            .unwrap();

        let consent_records_b = amal_b
            .syncable_consent_records(amal_b.provider.conn_ref())
            .unwrap();
        assert_eq!(consent_records_b.len(), 0);

        amal_a
            .get_sync_group(&amal_a.provider)
            .unwrap()
            .sync()
            .await
            .unwrap();
        amal_a
            .worker()
            .wait(SyncMetric::V1PayloadSent, 1)
            .await
            .unwrap();

        // Have amal_a receive the message (and auto-process)
        amal_b
            .get_sync_group(&amal_b.provider)
            .unwrap()
            .sync()
            .await
            .unwrap();
        amal_b
            .worker()
            .wait(SyncMetric::V1PayloadProcessed, 1)
            .await
            .unwrap();

        // Test consent streaming
        let amal_b_sync_group = amal_b.get_sync_group(&amal_b.provider).unwrap();
        let bo_wallet = generate_local_wallet();

        // Ensure bo is not consented with amal_b
        let bo_consent_with_amal_b = amal_b
            .provider
            .conn_ref()
            .get_consent_record(bo_wallet.get_inbox_id(0), ConsentType::InboxId)
            .unwrap();
        assert!(bo_consent_with_amal_b.is_none());

        // Consent with bo on the amal_a installation
        amal_a
            .set_consent_states(&[StoredConsentRecord::new(
                ConsentType::InboxId,
                ConsentState::Allowed,
                bo_wallet.get_inbox_id(0),
            )])
            .await
            .unwrap();
        amal_a
            .worker()
            .wait(SyncMetric::V1ConsentSent, 2)
            .await
            .unwrap();
        let amal_a_subscription = amal_a.local_events().subscribe();

        // Wait for the consent to get streamed to the amal_b
        amal_b_sync_group
            .sync_with_conn(&amal_b.provider)
            .await
            .unwrap();
        amal_b
            .worker()
            .wait(SyncMetric::V1ConsentReceived, 1)
            .await
            .unwrap();

        // No new messages were generated for the amal_a installation during this time.
        assert!(amal_a_subscription.is_empty());
    }
}
