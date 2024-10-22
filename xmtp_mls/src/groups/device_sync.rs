use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};

use aes_gcm::aead::generic_array::GenericArray;
use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm,
};
use rand::{
    distributions::{Alphanumeric, DistString},
    Rng, RngCore,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use xmtp_cryptography::utils as crypto_utils;
use xmtp_id::scw_verifier::SmartContractSignatureVerifier;
use xmtp_proto::api_client::trait_impls::XmtpApi;
use xmtp_proto::xmtp::mls::message_contents::device_sync_key_type::Key as EncKeyProto;
use xmtp_proto::xmtp::mls::message_contents::plaintext_envelope::Content;
use xmtp_proto::xmtp::mls::message_contents::{
    DeviceSyncReply as DeviceSyncReplyProto, DeviceSyncRequest as DeviceSyncRequestProto,
};

use xmtp_proto::xmtp::mls::message_contents::{
    plaintext_envelope::v2::MessageType::{Reply, Request},
    plaintext_envelope::V2,
    DeviceSyncKeyType as DeviceSyncKeyTypeProto, DeviceSyncKind, PlaintextEnvelope,
};

use super::group_metadata::ConversationType;
use super::{GroupError, MlsGroup};

use crate::storage::key_value_store::Key;
use crate::Store;
use crate::{
    client::ClientError,
    storage::{
        consent_record::StoredConsentRecord,
        group::StoredGroup,
        group_message::{GroupMessageKind, StoredGroupMessage},
        key_value_store::KVStore,
        StorageError,
    },
    Client,
};

#[cfg(feature = "consent-sync")]
pub mod consent_sync;
#[cfg(feature = "message-history")]
pub mod message_sync;

pub const ENC_KEY_SIZE: usize = 32; // 256-bit key
pub const NONCE_SIZE: usize = 12; // 96-bit nonce

#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
enum Syncable {
    Group(StoredGroup),
    GroupMessage(StoredGroupMessage),
    ConsentRecord(StoredConsentRecord),
}

#[derive(Debug, Error)]
pub enum DeviceSyncError {
    #[error("pin not found")]
    PinNotFound,
    #[error("pin does not match the expected value")]
    PinMismatch,
    #[error("IO error: {0}")]
    IO(#[from] std::io::Error),
    #[error("Serialization/Deserialization Error {0}")]
    Serde(#[from] serde_json::Error),
    #[error("AES-GCM encryption error")]
    AesGcm(#[from] aes_gcm::Error),
    #[error("reqwest error: {0}")]
    Reqwest(#[from] reqwest::Error),
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("type conversion error")]
    Conversion,
    #[error("utf-8 error: {0}")]
    UTF8(#[from] std::str::Utf8Error),
    #[error("client error: {0}")]
    Client(#[from] ClientError),
    #[error("group error: {0}")]
    Group(#[from] GroupError),
    #[error("request ID of reply does not match request")]
    ReplyRequestIdMismatch,
    #[error("reply already processed")]
    ReplyAlreadyProcessed,
    #[error("no pending request to reply to")]
    NoPendingRequest,
    #[error("no reply to process")]
    NoReplyToProcess,
    #[error("generic: {0}")]
    Generic(String),
    #[error("missing history sync url")]
    MissingHistorySyncUrl,
    #[error("invalid history message payload")]
    InvalidPayload,
    #[error("invalid history bundle url")]
    InvalidBundleUrl,
}

impl<ApiClient, V> Client<ApiClient, V>
where
    ApiClient: XmtpApi + Clone,
    V: SmartContractSignatureVerifier + Clone,
{
    async fn send_sync_request(
        &self,
        request: DeviceSyncRequest,
    ) -> Result<(String, String), DeviceSyncError> {
        // find the sync group
        let conn = self.store().conn()?;
        let sync_group = self.get_sync_group()?;

        // sync the group
        sync_group.sync().await?;

        let messages = sync_group.find_messages(
            Some(GroupMessageKind::Application),
            None,
            None,
            None,
            None,
        )?;

        let store_key = match request.kind {
            DeviceSyncKind::Consent => Key::ConsentSyncRequestId,
            DeviceSyncKind::MessageHistory => Key::MessageHistorySyncRequestId,
        };

        if let Some(request_id) =
            KVStore::get::<String>(&conn, &store_key).map_err(DeviceSyncError::Storage)?
        {
            for message in messages.iter().rev() {
                let ctx: DeviceSyncContent =
                    serde_json::from_slice(&message.decrypted_message_bytes)?;
                if let DeviceSyncContent::Request(request) = ctx {
                    if request.request_id == request_id {
                        return Ok((request.request_id, request.pin_code));
                    }
                }
            }

            // Request id not found.
            tracing::warn!("Unable to find sync message with request_id: {request_id}");
            KVStore::delete(&conn, &store_key).map_err(DeviceSyncError::Storage)?;
        }

        // build the request
        let request: DeviceSyncRequestProto = request.into();
        let pin_code = request.pin_code.clone();
        let request_id = request.request_id.clone();
        KVStore::set(&conn, &store_key, request_id.clone()).map_err(DeviceSyncError::Storage)?;

        let content = DeviceSyncContent::Request(request.clone());
        let content_bytes = serde_json::to_vec(&content)?;

        let _message_id = sync_group.prepare_message(&content_bytes, &conn, move |_time_ns| {
            PlaintextEnvelope {
                content: Some(Content::V2(V2 {
                    message_type: Some(Request(request)),
                    idempotency_key: new_request_id(),
                })),
            }
        })?;

        // publish the intent
        if let Err(err) = sync_group.publish_intents(&conn.into()).await {
            tracing::error!("error publishing sync group intents: {:?}", err);
        }

        Ok((request_id, pin_code))
    }

    async fn pending_sync_request(
        &self,
        kind: DeviceSyncKind,
    ) -> Result<Option<(StoredGroupMessage, DeviceSyncRequestProto)>, DeviceSyncError> {
        let sync_group = self.get_sync_group()?;

        sync_group.sync().await?;

        let messages = sync_group.find_messages(
            Some(GroupMessageKind::Application),
            None,
            None,
            None,
            None,
        )?;

        let mut replied_request_ids = vec![];
        for msg in messages.into_iter().rev() {
            let msg_content: DeviceSyncContent =
                serde_json::from_slice(&msg.decrypted_message_bytes)?;
            match msg_content {
                DeviceSyncContent::Request(request) if request.kind == kind as i32 => {
                    if replied_request_ids.contains(&request.request_id) {
                        // request was already replied to, no longer considered pending.
                        return Ok(None);
                    } else {
                        return Ok(Some((msg, request)));
                    }
                }
                DeviceSyncContent::Reply(reply) => {
                    // track this request_id as being replied to
                    replied_request_ids.push(reply.request_id.clone());
                }
                _ => {}
            }
        }

        Ok(None)
    }

    async fn pending_sync_request_id(
        &self,
        request_id: &str,
    ) -> Result<Option<(StoredGroupMessage, DeviceSyncRequestProto)>, DeviceSyncError> {
        let sync_group = self.get_sync_group()?;

        sync_group.sync().await?;

        let messages = sync_group.find_messages(
            Some(GroupMessageKind::Application),
            None,
            None,
            None,
            None,
        )?;

        for msg in messages.into_iter().rev() {
            let msg_content: DeviceSyncContent =
                serde_json::from_slice(&msg.decrypted_message_bytes)?;
            match msg_content {
                DeviceSyncContent::Request(request) if request.request_id == request_id => {
                    return Ok(Some((msg, request)));
                }
                DeviceSyncContent::Reply(reply) if reply.request_id == request_id => {
                    // already replied, request is not considered pending anymore
                    return Ok(None);
                }
                _ => {}
            }
        }

        Ok(None)
    }

    pub async fn get_sync_reply(
        &self,
        request_id: &str,
    ) -> Result<Option<DeviceSyncReplyProto>, DeviceSyncError> {
        let sync_group = self.get_sync_group()?;

        sync_group.sync().await?;
        let messages = sync_group.find_messages(
            Some(GroupMessageKind::Application),
            None,
            None,
            None,
            None,
        )?;

        for msg in messages.iter().rev() {
            // ignore this installation's messages
            if msg.sender_installation_id == self.installation_public_key() {
                continue;
            }

            let content: DeviceSyncContent = serde_json::from_slice(&msg.decrypted_message_bytes)?;
            if let DeviceSyncContent::Reply(reply) = content {
                if reply.request_id == request_id {
                    return Ok(Some(reply));
                }
            }
        }

        Ok(None)
    }

    pub async fn process_sync_reply(&self, request_id: &str) -> Result<(), DeviceSyncError> {
        let Some(reply) = self.get_sync_reply(&request_id).await? else {
            return Err(DeviceSyncError::NoReplyToProcess);
        };
        let Some(encryption_key) = reply.encryption_key.clone() else {
            return Err(DeviceSyncError::InvalidPayload);
        };

        let history_bundle = download_history_bundle(&reply.url).await?;
        let sync_path = std::env::temp_dir().join("sync.jsonl");

        decrypt_history_file(&history_bundle, &sync_path, encryption_key)?;

        self.insert_sync_bundle(&sync_path)?;

        // clean up temporary files associated with the bundle
        std::fs::remove_file(history_bundle.as_path())?;
        std::fs::remove_file(sync_path.as_path())?;

        self.sync_welcomes().await?;

        let conn = self.store().conn()?;
        let groups = conn.find_groups(None, None, None, None, Some(ConversationType::Group))?;
        for crate::storage::group::StoredGroup { id, .. } in groups.into_iter() {
            let group = self.group(id)?;
            Box::pin(group.sync()).await?;
        }

        Ok(())
    }

    pub async fn ensure_member_of_all_groups(&self, inbox_id: &str) -> Result<(), GroupError> {
        let conn = self.store().conn()?;
        let groups = conn.find_groups(None, None, None, None, Some(ConversationType::Group))?;
        for group in groups {
            let group = self.group(group.id)?;
            Box::pin(group.add_members_by_inbox_id(vec![inbox_id.to_string()])).await?;
        }

        Ok(())
    }

    pub(crate) fn insert_sync_bundle(&self, history_file: &Path) -> Result<(), DeviceSyncError> {
        let conn = self.store().conn()?;

        let file = File::open(history_file)?;
        let reader = BufReader::new(file);

        for line in reader.lines() {
            let db_entry: Syncable = serde_json::from_str(&line?)?;
            match db_entry {
                Syncable::Group(group) => group.store(&conn),
                Syncable::GroupMessage(group_message) => group_message.store(&conn),
                Syncable::ConsentRecord(consent_record) => consent_record.store(&conn),
            }?;
        }

        Ok(())
    }

    async fn send_syncables(
        &self,
        request_id: &str,
        syncables: &[Vec<Syncable>],
    ) -> Result<DeviceSyncReplyProto, DeviceSyncError> {
        let mut payload = vec![];
        for collection in syncables {
            for syncable in collection {
                payload.extend_from_slice(serde_json::to_string(&syncable)?.as_bytes());
                payload.push(b'\n');
            }
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

        let url = format!("{url}files/{}", response.text().await?);

        let sync_reply = DeviceSyncReplyProto {
            encryption_key: Some(enc_key.into()),
            request_id: request_id.to_string(),
            url,
        };

        self.send_sync_reply(sync_reply.clone()).await?;

        Ok(sync_reply)
    }

    async fn send_sync_reply(&self, contents: DeviceSyncReplyProto) -> Result<(), DeviceSyncError> {
        // find the sync group
        let conn = self.store().conn()?;
        let sync_group = self.get_sync_group()?;

        // sync the group
        sync_group.sync().await?;

        // try to add original sender to all groups on this device on the node
        if let Some((msg, _request)) = self.pending_sync_request_id(&contents.request_id).await? {
            self.ensure_member_of_all_groups(&msg.sender_inbox_id)
                .await?;
        }

        // the reply message
        let (content_bytes, contents) = {
            let content = DeviceSyncContent::Reply(contents);
            let content_bytes = serde_json::to_vec(&content)?;
            let DeviceSyncContent::Reply(contents) = content else {
                // we know it's a reply, we just want to take the contents back, as we'll need them
                unreachable!();
            };

            (content_bytes, contents)
        };

        let _message_id = sync_group.prepare_message(&content_bytes, &conn, move |_time_ns| {
            PlaintextEnvelope {
                content: Some(Content::V2(V2 {
                    idempotency_key: new_request_id(),
                    message_type: Some(Reply(contents)),
                })),
            }
        })?;

        // publish the intent
        if let Err(err) = sync_group.publish_messages().await {
            tracing::error!("error publishing sync group intents: {:?}", err);
        }
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub enum DeviceSyncContent {
    Request(DeviceSyncRequestProto),
    Reply(DeviceSyncReplyProto),
}

pub struct MessageHistoryUrls;

impl MessageHistoryUrls {
    pub const LOCAL_ADDRESS: &'static str = "http://0.0.0.0:5558";
    pub const DEV_ADDRESS: &'static str = "https://message-history.dev.ephemera.network/";
    pub const PRODUCTION_ADDRESS: &'static str = "https://message-history.ephemera.network/";
}

impl<ApiClient, V> Client<ApiClient, V>
where
    ApiClient: XmtpApi + Clone,
    V: SmartContractSignatureVerifier + Clone,
{
    pub fn get_sync_group(&self) -> Result<MlsGroup<Self>, GroupError> {
        let conn = self.store().conn()?;
        let sync_group_id = conn
            .find_sync_groups()?
            .pop()
            .ok_or(GroupError::GroupNotFound)?
            .id;
        let sync_group = self.group(sync_group_id.clone())?;

        Ok(sync_group)
    }
}

pub(crate) fn decrypt_history_file(
    input_path: &Path,
    output_path: &Path,
    encryption_key: DeviceSyncKeyTypeProto,
) -> Result<(), DeviceSyncError> {
    let enc_key: DeviceSyncKeyType = encryption_key.try_into()?;
    let enc_key_bytes = enc_key.as_bytes();
    // Read the messages file content
    let mut input_file = File::open(input_path)?;
    let mut buffer = Vec::new();
    input_file.read_to_end(&mut buffer)?;

    // Split the nonce and ciphertext
    let (nonce, ciphertext) = buffer.split_at(NONCE_SIZE);

    // Create a cipher instance
    let cipher = Aes256Gcm::new(GenericArray::from_slice(enc_key_bytes));
    let nonce_array = GenericArray::from_slice(nonce);

    // Decrypt the ciphertext
    let plaintext = cipher.decrypt(nonce_array, ciphertext)?;

    // Write the plaintext to the output file
    let mut output_file = File::create(output_path)?;
    output_file.write_all(&plaintext)?;

    Ok(())
}

pub(super) async fn upload_history_bundle(
    url: &str,
    file_path: PathBuf,
) -> Result<String, DeviceSyncError> {
    let mut file = File::open(file_path)?;
    let mut content = Vec::new();
    file.read_to_end(&mut content)?;

    let client = reqwest::Client::new();
    let response = client.post(url).body(content).send().await?;

    if response.status().is_success() {
        Ok(response.text().await?)
    } else {
        tracing::error!(
            "Failed to upload file. Status code: {} Response: {:?}",
            response.status(),
            response
        );
        Err(DeviceSyncError::Reqwest(
            response
                .error_for_status()
                .expect_err("Checked for success"),
        ))
    }
}

pub(crate) async fn download_history_bundle(url: &str) -> Result<PathBuf, DeviceSyncError> {
    let client = reqwest::Client::new();

    tracing::info!("downloading history bundle from {:?}", url);

    let bundle_name = url
        .split('/')
        .last()
        .ok_or(DeviceSyncError::InvalidBundleUrl)?;

    let response = client.get(url).send().await?;

    if response.status().is_success() {
        let file_name = format!("{}.jsonl.enc", bundle_name);
        let file_path = std::env::temp_dir().join(file_name);
        let mut file = File::create(&file_path)?;
        let bytes = response.bytes().await?;
        file.write_all(&bytes)?;
        tracing::info!("downloaded history bundle to {:?}", file_path);
        Ok(file_path)
    } else {
        tracing::error!(
            "Failed to download file. Status code: {} Response: {:?}",
            response.status(),
            response
        );
        Err(DeviceSyncError::Reqwest(
            response
                .error_for_status()
                .expect_err("Checked for success"),
        ))
    }
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
        DeviceSyncRequestProto {
            pin_code: req.pin_code,
            request_id: req.request_id,
            kind: req.kind as i32,
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
}

impl DeviceSyncReply {
    pub(crate) fn new(id: &str, url: &str, encryption_key: DeviceSyncKeyType) -> Self {
        Self {
            request_id: id.into(),
            url: url.into(),
            encryption_key,
        }
    }
}

impl From<DeviceSyncReply> for DeviceSyncReplyProto {
    fn from(reply: DeviceSyncReply) -> Self {
        DeviceSyncReplyProto {
            request_id: reply.request_id,
            url: reply.url,
            encryption_key: Some(reply.encryption_key.into()),
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub(crate) enum DeviceSyncKeyType {
    Chacha20Poly1305([u8; ENC_KEY_SIZE]),
}

impl DeviceSyncKeyType {
    fn new_chacha20_poly1305_key() -> Self {
        let mut rng = crypto_utils::rng();
        let mut key = [0u8; ENC_KEY_SIZE];
        rng.fill_bytes(&mut key);
        DeviceSyncKeyType::Chacha20Poly1305(key)
    }

    fn len(&self) -> usize {
        match self {
            DeviceSyncKeyType::Chacha20Poly1305(key) => key.len(),
        }
    }

    fn as_bytes(&self) -> &[u8; ENC_KEY_SIZE] {
        match self {
            DeviceSyncKeyType::Chacha20Poly1305(key) => key,
        }
    }
}

impl From<DeviceSyncKeyType> for DeviceSyncKeyTypeProto {
    fn from(key: DeviceSyncKeyType) -> Self {
        match key {
            DeviceSyncKeyType::Chacha20Poly1305(key) => DeviceSyncKeyTypeProto {
                key: Some(EncKeyProto::Chacha20Poly1305(key.to_vec())),
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
                let EncKeyProto::Chacha20Poly1305(hist_key) = k;
                match hist_key.try_into() {
                    Ok(array) => Ok(DeviceSyncKeyType::Chacha20Poly1305(array)),
                    Err(_) => Err(DeviceSyncError::Conversion),
                }
            }
            None => Err(DeviceSyncError::Conversion),
        }
    }
}

pub(super) fn new_request_id() -> String {
    Alphanumeric.sample_string(&mut rand::thread_rng(), ENC_KEY_SIZE)
}

pub(super) fn generate_nonce() -> [u8; NONCE_SIZE] {
    let mut nonce = [0u8; NONCE_SIZE];
    let mut rng = crypto_utils::rng();
    rng.fill_bytes(&mut nonce);
    nonce
}

pub(super) fn new_pin() -> String {
    let mut rng = crypto_utils::rng();
    let pin: u32 = rng.gen_range(0..10000);
    format!("{:04}", pin)
}

fn write_to_file<T: serde::Serialize>(
    file_path: &Path,
    content: Vec<T>,
) -> Result<(), DeviceSyncError> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(file_path)?;
    for entry in content {
        let entry_str = serde_json::to_string(&entry)?;
        file.write_all(entry_str.as_bytes())?;
        file.write_all(b"\n")?;
    }

    Ok(())
}

fn encrypt_bytes(
    payload: &[u8],
    encryption_key: &[u8; ENC_KEY_SIZE],
) -> Result<Vec<u8>, DeviceSyncError> {
    let mut result = generate_nonce().to_vec();

    // create a cipher instance
    let cipher = Aes256Gcm::new(GenericArray::from_slice(encryption_key));
    let nonce_array = GenericArray::from_slice(&result);

    // encrypt the payload and append to the result
    result.append(&mut cipher.encrypt(nonce_array, payload)?);

    Ok(result)
}
