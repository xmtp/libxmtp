#![allow(unused, dead_code)]
// TODO: Delete this on the next hammer version.
use super::device_sync::handle::{SyncMetric, WorkerHandle};
use super::device_sync::DeviceSyncError;
use crate::groups::device_sync::preference_sync::UserPreferenceUpdate;
use crate::subscriptions::SyncEvent;
use crate::{configuration::NS_IN_HOUR, subscriptions::LocalEvents, Client};
use aes_gcm::aead::generic_array::GenericArray;
use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm,
};
use rand::{Rng, RngCore};
use serde::{Deserialize, Serialize};
use xmtp_common::time::now_ns;
use xmtp_cryptography::utils as crypto_utils;
use xmtp_db::consent_record::StoredConsentRecord;
use xmtp_db::group::{ConversationType, GroupQueryArgs, StoredGroup};
use xmtp_db::group_message::{GroupMessageKind, MsgQueryArgs, StoredGroupMessage};
use xmtp_db::{DbConnection, StorageError, Store, XmtpOpenMlsProvider};
use xmtp_id::scw_verifier::SmartContractSignatureVerifier;
use xmtp_proto::api_client::trait_impls::XmtpApi;
use xmtp_proto::xmtp::mls::message_contents::device_sync_key_type::Key as EncKeyProto;
use xmtp_proto::xmtp::mls::message_contents::plaintext_envelope::Content;
use xmtp_proto::xmtp::mls::message_contents::{
    plaintext_envelope::v2::MessageType, plaintext_envelope::V2,
    DeviceSyncKeyType as DeviceSyncKeyTypeProto, DeviceSyncKind, PlaintextEnvelope,
};
use xmtp_proto::xmtp::mls::message_contents::{
    DeviceSyncReply as DeviceSyncReplyProto, DeviceSyncRequest as DeviceSyncRequestProto,
};

pub const ENC_KEY_SIZE: usize = 32; // 256-bit key
pub const NONCE_SIZE: usize = 12; // 96-bit nonce

#[derive(Debug, Deserialize, Serialize, PartialEq)]
#[serde(untagged)]
pub(super) enum Syncable {
    Group(StoredGroup),
    GroupMessage(StoredGroupMessage),
    ConsentRecord(StoredConsentRecord),
}

impl<ApiClient, V> Client<ApiClient, V>
where
    ApiClient: XmtpApi,
    V: SmartContractSignatureVerifier,
{
    pub(super) async fn v1_send_sync_request(
        &self,
        provider: &XmtpOpenMlsProvider,
        kind: DeviceSyncKind,
    ) -> Result<DeviceSyncRequestProto, DeviceSyncError> {
        tracing::info!(
            inbox_id = self.inbox_id(),
            installation_id = hex::encode(self.installation_public_key()),
            "Sending a sync request for {kind:?}"
        );
        let request = DeviceSyncRequest::new(kind);

        // find the sync group
        let sync_group = self.get_sync_group(provider).await?;

        // sync the group
        sync_group.sync_with_conn(provider).await?;

        // lookup if a request has already been made
        if let Ok((_msg, request)) = self
            .v1_get_pending_sync_request(provider, request.kind)
            .await
        {
            return Ok(request);
        }

        // build the request
        let request: DeviceSyncRequestProto = request.into();

        let content = DeviceSyncContent::Request(request.clone());
        let content_bytes = serde_json::to_vec(&content)?;

        let _message_id = sync_group.prepare_message(&content_bytes, provider, {
            let request = request.clone();
            move |now| PlaintextEnvelope {
                content: Some(Content::V2(V2 {
                    message_type: Some(MessageType::DeviceSyncRequest(request)),
                    idempotency_key: now.to_string(),
                })),
            }
        })?;

        // publish the intent
        sync_group.publish_intents(provider).await?;

        Ok(request)
    }

    pub(super) async fn v1_reply_to_sync_request(
        &self,
        provider: &XmtpOpenMlsProvider,
        request: DeviceSyncRequestProto,
        handle: &WorkerHandle<SyncMetric>,
    ) -> Result<DeviceSyncReplyProto, DeviceSyncError> {
        let conn = provider.conn_ref();

        let records = match request.kind() {
            DeviceSyncKind::Consent => vec![self.v1_syncable_consent_records(conn)?],
            DeviceSyncKind::MessageHistory => {
                vec![
                    self.v1_syncable_groups(conn)?,
                    self.v1_syncable_messages(conn)?,
                ]
            }
            DeviceSyncKind::Unspecified => return Err(DeviceSyncError::UnspecifiedDeviceSyncKind),
        };

        let reply = self
            .v1_create_sync_reply(&request.request_id, &records, request.kind())
            .await?;
        self.v1_send_sync_reply(provider, reply.clone()).await?;

        handle.increment_metric(SyncMetric::V1PayloadSent);

        Ok(reply)
    }

    async fn v1_send_sync_reply(
        &self,
        provider: &XmtpOpenMlsProvider,
        contents: DeviceSyncReplyProto,
    ) -> Result<(), DeviceSyncError> {
        // find the sync group
        let sync_group = self.get_sync_group(provider).await?;

        // sync the group
        sync_group.sync_with_conn(provider).await?;

        let (_msg, _request) = self
            .v1_get_pending_sync_request(provider, contents.kind())
            .await?;

        // add original sender to all groups on this device on the node
        self.add_new_installation_to_groups().await?;

        // the reply message
        let (content_bytes, contents) = {
            let content = DeviceSyncContent::Reply(contents);
            let content_bytes = serde_json::to_vec(&content)?;
            let DeviceSyncContent::Reply(contents) = content else {
                unreachable!("This is a reply.");
            };

            (content_bytes, contents)
        };

        sync_group.prepare_message(&content_bytes, provider, |now| PlaintextEnvelope {
            content: Some(Content::V2(V2 {
                message_type: Some(MessageType::DeviceSyncReply(contents)),
                idempotency_key: now.to_string(),
            })),
        })?;

        sync_group.publish_intents(provider).await?;

        Ok(())
    }

    async fn v1_get_pending_sync_request(
        &self,
        provider: &XmtpOpenMlsProvider,
        kind: DeviceSyncKind,
    ) -> Result<(StoredGroupMessage, DeviceSyncRequestProto), DeviceSyncError> {
        let sync_group = self.get_sync_group(provider).await?;
        sync_group.sync_with_conn(provider).await?;

        let messages = provider.conn_ref().get_group_messages(
            &sync_group.group_id,
            &MsgQueryArgs {
                kind: Some(GroupMessageKind::Application),
                ..Default::default()
            },
        )?;

        for msg in messages.into_iter().rev() {
            let Ok(msg_content) =
                serde_json::from_slice::<DeviceSyncContent>(&msg.decrypted_message_bytes)
            else {
                continue;
            };

            match msg_content {
                DeviceSyncContent::Reply(reply) if reply.kind() == kind => {
                    return Err(DeviceSyncError::NoPendingRequest);
                }
                DeviceSyncContent::Request(request) if request.kind() == kind => {
                    return Ok((msg, request));
                }
                _ => {}
            }
        }

        Err(DeviceSyncError::NoPendingRequest)
    }

    #[cfg(test)]
    async fn v1_get_latest_sync_reply(
        &self,
        provider: &XmtpOpenMlsProvider,
        kind: DeviceSyncKind,
    ) -> Result<Option<(StoredGroupMessage, DeviceSyncReplyProto)>, DeviceSyncError> {
        let sync_group = self.get_sync_group(provider).await?;
        sync_group.sync_with_conn(provider).await?;

        let messages = sync_group.find_messages(&MsgQueryArgs {
            kind: Some(GroupMessageKind::Application),
            ..Default::default()
        })?;

        for msg in messages.into_iter().rev() {
            let Ok(msg_content) =
                serde_json::from_slice::<DeviceSyncContent>(&msg.decrypted_message_bytes)
            else {
                continue;
            };
            match msg_content {
                DeviceSyncContent::Reply(reply) if reply.kind() == kind => {
                    return Ok(Some((msg, reply)));
                }
                _ => {}
            }
        }

        Ok(None)
    }

    pub(super) async fn v1_process_sync_reply(
        &self,
        provider: &XmtpOpenMlsProvider,
        reply: DeviceSyncReplyProto,
        handle: &WorkerHandle<SyncMetric>,
    ) -> Result<(), DeviceSyncError> {
        let conn = provider.conn_ref();

        #[allow(deprecated)]
        let time_diff = reply.timestamp_ns.abs_diff(now_ns() as u64);
        if time_diff > NS_IN_HOUR as u64 {
            // time discrepancy is too much
            return Err(DeviceSyncError::SyncPayloadTooOld);
        }

        #[allow(deprecated)]
        let Some(enc_key) = reply.encryption_key.clone() else {
            return Err(DeviceSyncError::InvalidPayload);
        };

        let enc_payload = download_history_payload(&reply.url).await?;
        self.v1_insert_encrypted_syncables(provider, enc_payload, &enc_key.try_into()?)
            .await?;

        self.sync_welcomes(provider).await?;

        let groups = conn.find_groups(GroupQueryArgs {
            conversation_type: Some(ConversationType::Group),
            ..Default::default()
        })?;
        for StoredGroup { id, .. } in groups.into_iter() {
            let group = self.group_with_conn(provider.conn_ref(), &id)?;
            group.maybe_update_installations(provider, None).await?;
            Box::pin(group.sync_with_conn(provider)).await?;
        }

        handle.increment_metric(SyncMetric::V1PayloadProcessed);

        Ok(())
    }

    async fn v1_create_sync_reply(
        &self,
        request_id: &str,
        syncables: &[Vec<Syncable>],
        kind: DeviceSyncKind,
    ) -> Result<DeviceSyncReplyProto, DeviceSyncError> {
        let (payload, enc_key) = encrypt_syncables(syncables)?;

        // upload the payload
        let Some(url) = &self.device_sync.server_url else {
            return Err(DeviceSyncError::MissingSyncServerUrl);
        };
        let upload_url = format!("{url}/upload");
        tracing::info!(
            inbox_id = self.inbox_id(),
            installation_id = hex::encode(self.installation_public_key()),
            "Using upload url {upload_url}",
        );

        let response = reqwest::Client::new()
            .post(upload_url)
            .body(payload)
            .send()
            .await?;

        if !response.status().is_success() {
            tracing::error!(
                inbox_id = self.inbox_id(),
                installation_id = hex::encode(self.installation_public_key()),
                "Failed to upload file. Status code: {} Response: {response:?}",
                response.status()
            );
            response.error_for_status()?;
            // checked for error, the above line bubbled up
            unreachable!();
        }

        let url = format!("{url}/files/{}", response.text().await?);

        #[allow(deprecated)]
        let sync_reply = DeviceSyncReplyProto {
            encryption_key: Some(enc_key.into()),
            request_id: request_id.to_string(),
            url,
            timestamp_ns: now_ns() as u64,
            kind: kind as i32,
            ..Default::default()
        };

        Ok(sync_reply)
    }

    async fn v1_insert_encrypted_syncables(
        &self,
        provider: &XmtpOpenMlsProvider,
        payload: Vec<u8>,
        enc_key: &DeviceSyncKeyType,
    ) -> Result<(), DeviceSyncError> {
        let conn = provider.conn_ref();
        let enc_key = enc_key.as_bytes();

        // Split the nonce and ciphertext
        let (nonce, ciphertext) = payload.split_at(NONCE_SIZE);

        // Create a cipher instance
        let cipher = Aes256Gcm::new(GenericArray::from_slice(enc_key));
        let nonce_array = GenericArray::from_slice(nonce);

        // Decrypt the ciphertext
        let payload = cipher.decrypt(nonce_array, ciphertext)?;
        let payload: Vec<Syncable> = serde_json::from_slice(&payload)?;

        for syncable in payload {
            match syncable {
                Syncable::Group(group) => {
                    conn.insert_or_replace_group(group)?;
                }
                Syncable::GroupMessage(group_message) => {
                    if let Err(err) = group_message.store(conn) {
                        match err {
                            // this is fine because we are inserting messages that already exist
                            StorageError::DieselResult(
                                xmtp_db::diesel::result::Error::DatabaseError(
                                    xmtp_db::diesel::result::DatabaseErrorKind::ForeignKeyViolation,
                                    _,
                                ),
                            ) => {}
                            // otherwise propagate the error
                            _ => Err(err)?,
                        }
                    }
                }
                Syncable::ConsentRecord(consent_record) => {
                    if let Some(existing_consent_record) =
                        conn.maybe_insert_consent_record_return_existing(&consent_record)?
                    {
                        if existing_consent_record.state != consent_record.state {
                            tracing::warn!("Existing consent record exists and does not match payload state. Streaming consent_record update to sync group.");
                            self.local_events
                                .send(LocalEvents::SyncEvent(SyncEvent::PreferencesOutgoing(
                                    vec![UserPreferenceUpdate::ConsentUpdate(
                                        existing_consent_record,
                                    )],
                                )))
                                .map_err(|e| DeviceSyncError::Generic(e.to_string()))?;
                        }
                    }
                }
            };
        }

        Ok(())
    }

    fn v1_syncable_consent_records(
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

    pub(super) fn v1_syncable_groups(
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

    pub(super) fn v1_syncable_messages(
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

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub enum DeviceSyncContent {
    Request(DeviceSyncRequestProto),
    Reply(DeviceSyncReplyProto),
}

pub(crate) async fn download_history_payload(url: &str) -> Result<Vec<u8>, DeviceSyncError> {
    tracing::info!("downloading history bundle from {:?}", url);
    let response = reqwest::Client::new().get(url).send().await?;

    if !response.status().is_success() {
        tracing::error!(
            "Failed to download file. Status code: {} Response: {:?}",
            response.status(),
            response
        );
        response.error_for_status()?;
        unreachable!("Checked for error");
    }

    Ok(response.bytes().await?.to_vec())
}

#[derive(Clone, Debug)]
pub(super) struct DeviceSyncRequest {
    pub pin_code: String,
    pub request_id: String,
    pub kind: DeviceSyncKind,
}

impl DeviceSyncRequest {
    pub(crate) fn new(kind: DeviceSyncKind) -> Self {
        Self {
            pin_code: new_pin(),
            request_id: new_request_id(),
            kind,
        }
    }
}

impl From<DeviceSyncRequest> for DeviceSyncRequestProto {
    fn from(req: DeviceSyncRequest) -> Self {
        #[allow(deprecated)]
        DeviceSyncRequestProto {
            pin_code: req.pin_code,
            request_id: req.request_id,
            kind: req.kind as i32,
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct DeviceSyncReply {
    /// Unique ID for each client Message History Request
    request_id: String,
    /// URL to download the backup bundle
    url: String,
    /// Encryption key for the backup bundle
    encryption_key: DeviceSyncKeyType,
    /// UNIX timestamp of when the reply was sent in ns
    timestamp_ns: u64,
    // sync kind
    kind: DeviceSyncKind,
}

impl From<DeviceSyncReply> for DeviceSyncReplyProto {
    fn from(reply: DeviceSyncReply) -> Self {
        #[allow(deprecated)]
        DeviceSyncReplyProto {
            request_id: reply.request_id,
            url: reply.url,
            encryption_key: Some(reply.encryption_key.into()),
            timestamp_ns: reply.timestamp_ns,
            kind: reply.kind as i32,
            ..Default::default()
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub(crate) enum DeviceSyncKeyType {
    Aes256Gcm([u8; ENC_KEY_SIZE]),
}

impl DeviceSyncKeyType {
    fn new_aes_256_gcm_key() -> Self {
        let mut rng = crypto_utils::rng();
        let mut key = [0u8; ENC_KEY_SIZE];
        rng.fill_bytes(&mut key);
        DeviceSyncKeyType::Aes256Gcm(key)
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        match self {
            DeviceSyncKeyType::Aes256Gcm(key) => key.len(),
        }
    }

    fn as_bytes(&self) -> &[u8; ENC_KEY_SIZE] {
        match self {
            DeviceSyncKeyType::Aes256Gcm(key) => key,
        }
    }
}

impl From<DeviceSyncKeyType> for DeviceSyncKeyTypeProto {
    fn from(key: DeviceSyncKeyType) -> Self {
        match key {
            DeviceSyncKeyType::Aes256Gcm(key) => DeviceSyncKeyTypeProto {
                key: Some(EncKeyProto::Aes256Gcm(key.to_vec())),
            },
        }
    }
}

impl TryFrom<DeviceSyncKeyTypeProto> for DeviceSyncKeyType {
    type Error = DeviceSyncError;
    fn try_from(key: DeviceSyncKeyTypeProto) -> Result<Self, Self::Error> {
        let DeviceSyncKeyTypeProto { key } = key;
        match key {
            Some(k) => {
                let EncKeyProto::Aes256Gcm(key) = k;
                match key.try_into() {
                    Ok(array) => Ok(DeviceSyncKeyType::Aes256Gcm(array)),
                    Err(_) => Err(DeviceSyncError::Conversion),
                }
            }
            None => Err(DeviceSyncError::Conversion),
        }
    }
}

pub(super) fn new_request_id() -> String {
    xmtp_common::rand_string::<ENC_KEY_SIZE>()
}

pub(super) fn generate_nonce() -> [u8; NONCE_SIZE] {
    xmtp_common::rand_array::<NONCE_SIZE>()
}

pub(super) fn new_pin() -> String {
    let mut rng = crypto_utils::rng();
    let pin: u32 = rng.gen_range(0..10000);
    format!("{:04}", pin)
}

fn encrypt_syncables(
    syncables: &[Vec<Syncable>],
) -> Result<(Vec<u8>, DeviceSyncKeyType), DeviceSyncError> {
    let enc_key = DeviceSyncKeyType::new_aes_256_gcm_key();
    encrypt_syncables_with_key(syncables, enc_key)
}

fn encrypt_syncables_with_key(
    syncables: &[Vec<Syncable>],
    enc_key: DeviceSyncKeyType,
) -> Result<(Vec<u8>, DeviceSyncKeyType), DeviceSyncError> {
    let syncables: Vec<&Syncable> = syncables.iter().flat_map(|s| s.iter()).collect();
    let payload = serde_json::to_vec(&syncables)?;

    let enc_key_bytes = enc_key.as_bytes();
    let mut result = generate_nonce().to_vec();

    // create a cipher instance
    let cipher = Aes256Gcm::new(GenericArray::from_slice(enc_key_bytes));
    let nonce_array = GenericArray::from_slice(&result);

    // encrypt the payload and append to the result
    result.append(&mut cipher.encrypt(nonce_array, &*payload)?);

    Ok((result, enc_key))
}

#[cfg(test)]
mod tests {
    use xmtp_proto::xmtp::mls::message_contents::DeviceSyncKind;

    use crate::{
        groups::device_sync::handle::SyncMetric,
        utils::{Tester, XmtpClientTesterBuilder},
    };

    #[xmtp_common::test(unwrap_try = "true")]
    async fn v1_sync_still_works() {
        let alix1 = Tester::new().await;
        let alix2 = alix1.builder.build().await;

        alix1.sync_welcomes(&alix1.provider).await?;
        alix1.worker().wait(SyncMetric::PayloadSent, 1).await?;

        alix2.get_sync_group(&alix2.provider).await?.sync().await?;
        alix2.worker().wait(SyncMetric::PayloadProcessed, 1).await?;

        assert_eq!(alix1.worker().get(SyncMetric::V1PayloadSent), 0);
        assert_eq!(alix2.worker().get(SyncMetric::V1PayloadProcessed), 0);

        alix2
            .v1_send_sync_request(&alix2.provider, DeviceSyncKind::MessageHistory)
            .await?;
        alix1.sync_device_sync(&alix1.provider).await?;
        alix1.worker().wait(SyncMetric::V1PayloadSent, 1).await?;

        alix2.sync_device_sync(&alix2.provider).await?;
        alix2
            .worker()
            .wait(SyncMetric::V1PayloadProcessed, 1)
            .await?;

        alix2
            .v1_send_sync_request(&alix2.provider, DeviceSyncKind::Consent)
            .await?;
        alix1.sync_device_sync(&alix1.provider).await?;
        alix1.worker().wait(SyncMetric::V1PayloadSent, 2).await?;

        alix2.sync_device_sync(&alix2.provider).await?;
        alix2
            .worker()
            .wait(SyncMetric::V1PayloadProcessed, 2)
            .await?;
    }
}
