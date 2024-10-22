use std::io::Cursor;

use super::*;
use crate::{
    storage::{
        key_value_store::{KVStore, Key},
        DbConnection,
    },
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
    ) -> Result<DeviceSyncReplyProto, DeviceSyncError> {
        let pending_request = self.get_pending_history_request().await?;
        if let Some((request_id, _)) = pending_request {
            let reply: DeviceSyncReplyProto = self.prepare_history_reply(&request_id).await?.into();
            self.send_sync_reply(reply.clone()).await?;
            return Ok(reply);
        }

        Err(DeviceSyncError::NoPendingRequest)
    }

    pub async fn process_consent_sync_reply(
        &self,
        conn: &DbConnection,
    ) -> Result<(), DeviceSyncError> {
        // load the request_id
        let request_id: Option<String> =
            KVStore::get(conn, &Key::ConsentSyncRequestId).map_err(DeviceSyncError::Storage)?;
        let Some(request_id) = request_id else {
            return Err(DeviceSyncError::NoReplyToProcess);
        };

        // process the reply
        self.process_sync_reply(&request_id).await
    }

    pub async fn prepare_consent_sync_reply(
        &self,
        request_id: &str,
    ) -> Result<DeviceSyncReply, DeviceSyncError> {
        let conn = self.store().conn()?;
        let consent_records = conn.load_consent_records()?;

        // build the payload
        let mut payload = Vec::new();
        for record in consent_records {
            payload.extend_from_slice(serde_json::to_string(&record)?.as_bytes());
            payload.push(b'\n');
        }

        // encrypt the payload
        let enc_key = DeviceSyncKeyType::new_chacha20_poly1305_key();
        let payload = encrypt_bytes(&payload, enc_key.as_bytes())?;

        // upload the payload
        let Some(url) = &self.history_sync_url else {
            return Err(DeviceSyncError::MissingHistorySyncUrl);
        };
        tracing::info!("Using upload url {url}upload");

        let response = reqwest::Client::new()
            .post(format!("{url}upload"))
            .body(payload)
            .send()
            .await?;

        if !response.status().is_success() {
            tracing::error!(
                "Failed to upload file. Status code: {} Response: {response:?}",
                response.status()
            );
            response.error_for_status()?;
            // checked for error, the above line bubbled up
            unreachable!();
        }

        let upload_url = format!("{url}files/{}", response.text().await?);

        Ok(DeviceSyncReply::new(request_id, &upload_url, enc_key))
    }
}
