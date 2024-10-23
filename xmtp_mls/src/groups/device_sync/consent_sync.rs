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

    async fn process_consent_sync_reply(&self) -> Result<(), DeviceSyncError> {
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
