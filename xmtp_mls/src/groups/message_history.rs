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
use xmtp_proto::{
    xmtp::mls::message_contents::plaintext_envelope::v2::MessageType::{Reply, Request},
    xmtp::mls::message_contents::plaintext_envelope::{Content, V2},
    xmtp::mls::message_contents::PlaintextEnvelope,
    xmtp::mls::message_contents::{
        message_history_key_type::Key, MessageHistoryKeyType, MessageHistoryReply,
        MessageHistoryRequest,
    },
};

use super::{GroupError, MlsGroup};

use crate::XmtpApi;
use crate::{
    client::ClientError,
    groups::{GroupMessageKind, StoredGroupMessage},
    storage::{group::StoredGroup, StorageError},
    Client, Store,
};

const ENC_KEY_SIZE: usize = 32; // 256-bit key
const NONCE_SIZE: usize = 12; // 96-bit nonce

pub struct MessageHistoryUrls;

impl MessageHistoryUrls {
    pub const LOCAL_ADDRESS: &'static str = "http://0.0.0.0:5558";
    pub const DEV_ADDRESS: &'static str = "https://message-history.dev.ephemera.network/";
    pub const PRODUCTION_ADDRESS: &'static str = "https://message-history.ephemera.network/";
}

#[derive(Debug, Serialize, Deserialize)]
pub enum MessageHistoryContent {
    Request(MessageHistoryRequest),
    Reply(MessageHistoryReply),
}

#[derive(Debug, Error)]
pub enum MessageHistoryError {
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

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum SyncableTables {
    StoredGroup(StoredGroup),
    StoredGroupMessage(StoredGroupMessage),
}

impl<ApiClient> Client<ApiClient>
where
    ApiClient: XmtpApi,
{
    pub fn get_sync_group(&self) -> Result<MlsGroup, GroupError> {
        let conn = self.store().conn()?;
        let sync_group_id = conn
            .find_sync_groups()?
            .pop()
            .ok_or(GroupError::GroupNotFound)?
            .id;
        let sync_group = self.group(sync_group_id.clone())?;

        Ok(sync_group)
    }

    pub async fn enable_history_sync(&self) -> Result<(), GroupError> {
        // look for the sync group, create if not found
        let sync_group = match self.get_sync_group() {
            Ok(group) => group,
            Err(_) => {
                // create the sync group
                self.create_sync_group()?
            }
        };

        // sync the group
        sync_group.sync(self).await?;

        Ok(())
    }

    pub async fn ensure_member_of_all_groups(&self, inbox_id: String) -> Result<(), GroupError> {
        let conn = self.store().conn()?;
        let groups = conn.find_groups(None, None, None, None, false)?;
        for group in groups {
            let group = self.group(group.id)?;
            Box::pin(group.add_members_by_inbox_id(self, vec![inbox_id.clone()])).await?;
        }

        Ok(())
    }

    // returns (request_id, pin_code)
    pub async fn send_history_request(&self) -> Result<(String, String), MessageHistoryError> {
        // find the sync group
        let conn = self.store().conn()?;
        let sync_group = self.get_sync_group()?;

        // sync the group
        sync_group.sync(self).await?;

        let messages = sync_group.find_messages(
            Some(GroupMessageKind::Application),
            None,
            None,
            None,
            None,
        )?;

        let last_message = messages.last();
        if let Some(msg) = last_message {
            let message_history_content =
                serde_json::from_slice::<MessageHistoryContent>(&msg.decrypted_message_bytes)?;

            if let MessageHistoryContent::Request(request) = message_history_content {
                return Ok((request.request_id, request.pin_code));
            }
        };

        // build the request
        let history_request = HistoryRequest::new();
        let pin_code = history_request.pin_code.clone();
        let request_id = history_request.request_id.clone();

        let content = MessageHistoryContent::Request(MessageHistoryRequest {
            request_id: request_id.clone(),
            pin_code: pin_code.clone(),
        });
        let content_bytes = serde_json::to_vec(&content)?;

        let _message_id =
            sync_group.prepare_message(content_bytes.as_slice(), &conn, move |_time_ns| {
                PlaintextEnvelope {
                    content: Some(Content::V2(V2 {
                        message_type: Some(Request(history_request.into())),
                        idempotency_key: new_request_id(),
                    })),
                }
            })?;

        // publish the intent
        if let Err(err) = sync_group.publish_intents(&conn.into(), self).await {
            tracing::error!("error publishing sync group intents: {:?}", err);
        }

        Ok((request_id, pin_code))
    }

    pub(crate) async fn send_history_reply(
        &self,
        contents: MessageHistoryReply,
    ) -> Result<(), MessageHistoryError> {
        // find the sync group
        let conn = self.store().conn()?;
        let sync_group = self.get_sync_group()?;

        // sync the group
        Box::pin(sync_group.sync(self)).await?;

        let messages = sync_group.find_messages(
            Some(GroupMessageKind::Application),
            None,
            None,
            None,
            None,
        )?;

        let last_message = match messages.last() {
            Some(msg) => {
                let message_history_content =
                    serde_json::from_slice::<MessageHistoryContent>(&msg.decrypted_message_bytes)?;
                match message_history_content {
                    MessageHistoryContent::Request(request) => {
                        // check that the request ID matches
                        if !request.request_id.eq(&contents.request_id) {
                            return Err(MessageHistoryError::ReplyRequestIdMismatch);
                        }
                        Some(msg)
                    }
                    MessageHistoryContent::Reply(_) => {
                        // if last message is a reply, it's already been processed
                        return Err(MessageHistoryError::ReplyAlreadyProcessed);
                    }
                }
            }
            None => {
                return Err(MessageHistoryError::NoPendingRequest);
            }
        };

        tracing::info!("{:?}", last_message);

        if let Some(msg) = last_message {
            // ensure the requester is a member of all the groups
            self.ensure_member_of_all_groups(msg.sender_inbox_id.clone())
                .await?;
        }

        // the reply message
        let content = MessageHistoryContent::Reply(contents.clone());
        let content_bytes = serde_json::to_vec(&content)?;

        let _message_id =
            sync_group.prepare_message(content_bytes.as_slice(), &conn, move |_time_ns| {
                PlaintextEnvelope {
                    content: Some(Content::V2(V2 {
                        idempotency_key: new_request_id(),
                        message_type: Some(Reply(contents)),
                    })),
                }
            })?;

        // publish the intent
        if let Err(err) = sync_group.publish_intents(&conn.into(), self).await {
            tracing::error!("error publishing sync group intents: {:?}", err);
        }
        Ok(())
    }

    pub async fn get_pending_history_request(
        &self,
    ) -> Result<Option<(String, String)>, MessageHistoryError> {
        let sync_group = self.get_sync_group()?;

        // sync the group
        sync_group.sync(self).await?;

        let messages = sync_group.find_messages(
            Some(GroupMessageKind::Application),
            None,
            None,
            None,
            None,
        )?;
        let last_message = messages.last();

        let history_request: Option<(String, String)> = if let Some(msg) = last_message {
            let message_history_content =
                serde_json::from_slice::<MessageHistoryContent>(&msg.decrypted_message_bytes)?;
            match message_history_content {
                // if the last message is a request, return its request ID and pin code
                MessageHistoryContent::Request(request) => {
                    Some((request.request_id, request.pin_code))
                }
                _ => None,
            }
        } else {
            None
        };

        Ok(history_request)
    }

    pub async fn reply_to_history_request(
        &self,
    ) -> Result<MessageHistoryReply, MessageHistoryError> {
        let pending_request = self.get_pending_history_request().await?;

        if let Some((request_id, _)) = pending_request {
            let reply = self.prepare_history_reply(&request_id).await?;
            self.send_history_reply(reply.clone().into()).await?;
            return Ok(reply.into());
        }

        Err(MessageHistoryError::NoPendingRequest)
    }

    pub async fn get_latest_history_reply(
        &self,
    ) -> Result<Option<MessageHistoryReply>, MessageHistoryError> {
        let sync_group = self.get_sync_group()?;

        // sync the group
        sync_group.sync(self).await?;

        let messages = sync_group.find_messages(
            Some(GroupMessageKind::Application),
            None,
            None,
            None,
            None,
        )?;

        let last_message = messages.last();

        let reply: Option<MessageHistoryReply> = match last_message {
            Some(msg) => {
                // if the message was sent by this installation, ignore it
                if msg
                    .sender_installation_id
                    .eq(&self.installation_public_key())
                {
                    None
                } else {
                    let message_history_content = serde_json::from_slice::<MessageHistoryContent>(
                        &msg.decrypted_message_bytes,
                    )?;
                    match message_history_content {
                        // if the last message is a reply, return it
                        MessageHistoryContent::Reply(reply) => Some(reply),
                        _ => None,
                    }
                }
            }
            None => None,
        };

        Ok(reply)
    }

    pub async fn process_history_reply(&self) -> Result<(), MessageHistoryError> {
        let reply = self.get_latest_history_reply().await?;

        if let Some(reply) = reply {
            let Some(encryption_key) = reply.encryption_key.clone() else {
                return Err(MessageHistoryError::InvalidPayload);
            };

            let history_bundle = download_history_bundle(&reply.url).await?;
            let messages_path = std::env::temp_dir().join("messages.jsonl");

            decrypt_history_file(&history_bundle, &messages_path, encryption_key)?;

            self.insert_history_bundle(&messages_path)?;

            // clean up temporary files associated with the bundle
            std::fs::remove_file(history_bundle.as_path())?;
            std::fs::remove_file(messages_path.as_path())?;

            self.sync_welcomes().await?;

            let conn = self.store().conn()?;
            let groups = conn.find_groups(None, None, None, None, false)?;
            for crate::storage::group::StoredGroup { id, .. } in groups.into_iter() {
                let group = self.group(id)?;
                Box::pin(group.sync(self)).await?;
            }

            return Ok(());
        }

        Err(MessageHistoryError::NoReplyToProcess)
    }

    pub(crate) fn verify_pin(
        &self,
        request_id: &str,
        pin_code: &str,
    ) -> Result<(), MessageHistoryError> {
        let sync_group = self.get_sync_group()?;
        let requests = sync_group.find_messages(
            Some(GroupMessageKind::Application),
            None,
            None,
            None,
            None,
        )?;
        let request = requests.into_iter().find(|msg| {
            let message_history_content =
                serde_json::from_slice::<MessageHistoryContent>(&msg.decrypted_message_bytes);

            match message_history_content {
                Ok(MessageHistoryContent::Request(request)) => {
                    request.request_id.eq(request_id) && request.pin_code.eq(pin_code)
                }
                Err(e) => {
                    tracing::debug!("serde_json error: {:?}", e);
                    false
                }
                _ => false,
            }
        });

        if request.is_none() {
            return Err(MessageHistoryError::PinNotFound);
        }

        Ok(())
    }

    pub(crate) fn insert_history_bundle(
        &self,
        history_file: &Path,
    ) -> Result<(), MessageHistoryError> {
        let file = File::open(history_file)?;
        let reader = BufReader::new(file);
        let lines = reader.lines();

        let conn = self.store().conn()?;

        for line in lines {
            let line = line?;
            let db_entry: SyncableTables = serde_json::from_str(&line)?;
            match db_entry {
                SyncableTables::StoredGroup(group) => {
                    // alternatively consider: group.store(&conn)?
                    conn.insert_or_replace_group(group)?;
                }
                SyncableTables::StoredGroupMessage(group_message) => {
                    group_message.store(&conn)?;
                }
            }
        }

        Ok(())
    }

    pub(crate) async fn prepare_history_reply(
        &self,
        request_id: &str,
    ) -> Result<HistoryReply, MessageHistoryError> {
        let (history_file, enc_key) = self.write_history_bundle().await?;
        let url = match &self.history_sync_url {
            Some(url) => url.as_str(),
            None => return Err(MessageHistoryError::MissingHistorySyncUrl),
        };
        let upload_url = format!("{}{}", url, "upload");
        tracing::info!("using upload url {:?}", upload_url);

        let bundle_file = upload_history_bundle(&upload_url, history_file.clone()).await?;
        let bundle_url = format!("{}files/{}", url, bundle_file);

        tracing::info!("history bundle uploaded to {:?}", bundle_url);

        Ok(HistoryReply::new(request_id, &bundle_url, enc_key))
    }

    async fn write_history_bundle(&self) -> Result<(PathBuf, HistoryKeyType), MessageHistoryError> {
        let groups = self.prepare_groups_to_sync().await?;
        let messages = self.prepare_messages_to_sync().await?;

        let temp_file = std::env::temp_dir().join("history.jsonl.tmp");
        write_to_file(temp_file.as_path(), groups)?;
        write_to_file(temp_file.as_path(), messages)?;

        let history_file = std::env::temp_dir().join("history.jsonl.enc");
        let enc_key = HistoryKeyType::new_chacha20_poly1305_key();
        encrypt_history_file(
            temp_file.as_path(),
            history_file.as_path(),
            enc_key.as_bytes(),
        )?;

        std::fs::remove_file(temp_file.as_path())?;

        Ok((history_file, enc_key))
    }

    async fn prepare_groups_to_sync(&self) -> Result<Vec<StoredGroup>, MessageHistoryError> {
        let conn = self.store().conn()?;
        Ok(conn.find_groups(None, None, None, None, false)?)
    }

    async fn prepare_messages_to_sync(
        &self,
    ) -> Result<Vec<StoredGroupMessage>, MessageHistoryError> {
        let conn = self.store().conn()?;
        let groups = conn.find_groups(None, None, None, None, false)?;
        let mut all_messages: Vec<StoredGroupMessage> = vec![];

        for StoredGroup { id, .. } in groups.into_iter() {
            let messages = conn.get_group_messages(id, None, None, None, None, None)?;
            all_messages.extend(messages);
        }

        Ok(all_messages)
    }
}

fn write_to_file<T: serde::Serialize>(
    file_path: &Path,
    content: Vec<T>,
) -> Result<(), MessageHistoryError> {
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

fn encrypt_history_file(
    input_path: &Path,
    output_path: &Path,
    encryption_key: &[u8; ENC_KEY_SIZE],
) -> Result<(), MessageHistoryError> {
    // Read in the messages file content
    let mut input_file = File::open(input_path)?;
    let mut buffer = Vec::new();
    input_file.read_to_end(&mut buffer)?;

    let nonce = generate_nonce();

    // Create a cipher instance
    let cipher = Aes256Gcm::new(GenericArray::from_slice(encryption_key));
    let nonce_array = GenericArray::from_slice(&nonce);

    // Encrypt the file content
    let ciphertext = cipher.encrypt(nonce_array, buffer.as_ref())?;

    // Write the nonce and ciphertext to the output file
    let mut output_file = File::create(output_path)?;
    output_file.write_all(&nonce)?;
    output_file.write_all(&ciphertext)?;

    Ok(())
}

pub(crate) fn decrypt_history_file(
    input_path: &Path,
    output_path: &Path,
    encryption_key: MessageHistoryKeyType,
) -> Result<(), MessageHistoryError> {
    let enc_key: HistoryKeyType = encryption_key.try_into()?;
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

async fn upload_history_bundle(
    url: &str,
    file_path: PathBuf,
) -> Result<String, MessageHistoryError> {
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
        Err(MessageHistoryError::Reqwest(
            response
                .error_for_status()
                .expect_err("Checked for success"),
        ))
    }
}

pub(crate) async fn download_history_bundle(url: &str) -> Result<PathBuf, MessageHistoryError> {
    let client = reqwest::Client::new();

    tracing::info!("downloading history bundle from {:?}", url);

    let bundle_name = url
        .split('/')
        .last()
        .ok_or(MessageHistoryError::InvalidBundleUrl)?;

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
        Err(MessageHistoryError::Reqwest(
            response
                .error_for_status()
                .expect_err("Checked for success"),
        ))
    }
}

#[derive(Clone)]
struct HistoryRequest {
    pin_code: String,
    request_id: String,
}

impl HistoryRequest {
    pub(crate) fn new() -> Self {
        Self {
            pin_code: new_pin(),
            request_id: new_request_id(),
        }
    }
}

impl From<HistoryRequest> for MessageHistoryRequest {
    fn from(req: HistoryRequest) -> Self {
        MessageHistoryRequest {
            pin_code: req.pin_code,
            request_id: req.request_id,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct HistoryReply {
    /// Unique ID for each client Message History Request
    request_id: String,
    /// URL to download the backup bundle
    url: String,
    /// Encryption key for the backup bundle
    encryption_key: HistoryKeyType,
}

impl HistoryReply {
    pub(crate) fn new(id: &str, url: &str, encryption_key: HistoryKeyType) -> Self {
        Self {
            request_id: id.into(),
            url: url.into(),
            encryption_key,
        }
    }
}

impl From<HistoryReply> for MessageHistoryReply {
    fn from(reply: HistoryReply) -> Self {
        MessageHistoryReply {
            request_id: reply.request_id,
            url: reply.url,
            encryption_key: Some(reply.encryption_key.into()),
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub(crate) enum HistoryKeyType {
    Chacha20Poly1305([u8; ENC_KEY_SIZE]),
}

impl HistoryKeyType {
    fn new_chacha20_poly1305_key() -> Self {
        let mut rng = crypto_utils::rng();
        let mut key = [0u8; ENC_KEY_SIZE];
        rng.fill_bytes(&mut key);
        HistoryKeyType::Chacha20Poly1305(key)
    }

    fn len(&self) -> usize {
        match self {
            HistoryKeyType::Chacha20Poly1305(key) => key.len(),
        }
    }

    fn as_bytes(&self) -> &[u8; ENC_KEY_SIZE] {
        match self {
            HistoryKeyType::Chacha20Poly1305(key) => key,
        }
    }
}

impl From<HistoryKeyType> for MessageHistoryKeyType {
    fn from(key: HistoryKeyType) -> Self {
        match key {
            HistoryKeyType::Chacha20Poly1305(key) => MessageHistoryKeyType {
                key: Some(Key::Chacha20Poly1305(key.to_vec())),
            },
        }
    }
}

impl TryFrom<MessageHistoryKeyType> for HistoryKeyType {
    type Error = MessageHistoryError;
    fn try_from(key: MessageHistoryKeyType) -> Result<Self, Self::Error> {
        let MessageHistoryKeyType { key } = key;
        match key {
            Some(k) => {
                let Key::Chacha20Poly1305(hist_key) = k;
                match hist_key.try_into() {
                    Ok(array) => Ok(HistoryKeyType::Chacha20Poly1305(array)),
                    Err(_) => Err(MessageHistoryError::Conversion),
                }
            }
            None => Err(MessageHistoryError::Conversion),
        }
    }
}

fn new_request_id() -> String {
    Alphanumeric.sample_string(&mut rand::thread_rng(), ENC_KEY_SIZE)
}

fn generate_nonce() -> [u8; NONCE_SIZE] {
    let mut nonce = [0u8; NONCE_SIZE];
    let mut rng = crypto_utils::rng();
    rng.fill_bytes(&mut nonce);
    nonce
}

fn new_pin() -> String {
    let mut rng = crypto_utils::rng();
    let pin: u32 = rng.gen_range(0..10000);
    format!("{:04}", pin)
}

#[cfg(test)]
mod tests {

    const HISTORY_SERVER_HOST: &str = "0.0.0.0";
    const HISTORY_SERVER_PORT: u16 = 5558;

    use super::*;
    use mockito;
    use std::io::{BufRead, BufReader};
    use tempfile::NamedTempFile;
    use xmtp_cryptography::utils::generate_local_wallet;
    use xmtp_id::InboxOwner;

    use crate::{assert_ok, builder::ClientBuilder, groups::GroupMetadataOptions};

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_enable_history_sync() {
        let wallet = generate_local_wallet();
        let client = ClientBuilder::new_test_client(&wallet).await;
        assert_ok!(client.enable_history_sync().await);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_installations_are_added_to_sync_group() {
        let wallet = generate_local_wallet();
        let amal_a = ClientBuilder::new_test_client(&wallet).await;
        let amal_b = ClientBuilder::new_test_client(&wallet).await;
        let amal_c = ClientBuilder::new_test_client(&wallet).await;
        assert_ok!(amal_c.enable_history_sync().await);

        amal_a.sync_welcomes().await.expect("sync_welcomes");
        amal_b.sync_welcomes().await.expect("sync_welcomes");

        let conn_a = amal_a.store().conn().unwrap();
        let amal_a_sync_groups = conn_a.find_sync_groups().unwrap();

        let conn_b = amal_b.store().conn().unwrap();
        let amal_b_sync_groups = conn_b.find_sync_groups().unwrap();

        let conn_c = amal_c.store().conn().unwrap();
        let amal_c_sync_groups = conn_c.find_sync_groups().unwrap();

        assert_eq!(amal_a_sync_groups.len(), 1);
        assert_eq!(amal_b_sync_groups.len(), 1);
        assert_eq!(amal_c_sync_groups.len(), 1);
        // make sure all installations are in the same sync group
        assert_eq!(amal_a_sync_groups[0].id, amal_b_sync_groups[0].id);
        assert_eq!(amal_b_sync_groups[0].id, amal_c_sync_groups[0].id);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_send_history_request() {
        let wallet = generate_local_wallet();
        let client = ClientBuilder::new_test_client(&wallet).await;
        assert_ok!(client.enable_history_sync().await);

        // test that the request is sent, and that the pin code is returned
        let (request_id, pin_code) = client
            .send_history_request()
            .await
            .expect("history request");
        assert_eq!(request_id.len(), 32);
        assert_eq!(pin_code.len(), 4);

        // test that another request will return the same request_id and
        // pin_code because it hasn't been replied to yet
        let (request_id2, pin_code2) = client
            .send_history_request()
            .await
            .expect("history request");
        assert_eq!(request_id, request_id2);
        assert_eq!(pin_code, pin_code2);

        // make sure there's only 1 message in the sync group
        let sync_group = client.get_sync_group().unwrap();
        let messages = sync_group
            .find_messages(Some(GroupMessageKind::Application), None, None, None, None)
            .unwrap();
        assert_eq!(messages.len(), 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_send_history_reply() {
        let wallet = generate_local_wallet();
        let client = ClientBuilder::new_test_client(&wallet).await;
        assert_ok!(client.enable_history_sync().await);

        let request_id = new_request_id();
        let url = "https://test.com/abc-123";
        let encryption_key = HistoryKeyType::new_chacha20_poly1305_key();
        let reply = HistoryReply::new(&request_id, url, encryption_key);
        let result = client.send_history_reply(reply.into()).await;

        // the reply should fail because there's no pending request to reply to
        assert!(result.is_err());

        let (request_id, _) = client
            .send_history_request()
            .await
            .expect("history request");

        let request_id2 = new_request_id();
        let url = "https://test.com/abc-123";
        let encryption_key = HistoryKeyType::new_chacha20_poly1305_key();
        let reply = HistoryReply::new(&request_id2, url, encryption_key);
        let result = client.send_history_reply(reply.into()).await;

        // the reply should fail because there's a mismatched request ID
        assert!(result.is_err());

        let url = "https://test.com/abc-123";
        let encryption_key = HistoryKeyType::new_chacha20_poly1305_key();
        let reply = HistoryReply::new(&request_id, url, encryption_key);
        let result = client.send_history_reply(reply.into()).await;

        // the reply should succeed with a valid request ID
        assert_ok!(result);

        // make sure there's 2 messages in the sync group
        let sync_group = client.get_sync_group().unwrap();
        let messages = sync_group
            .find_messages(Some(GroupMessageKind::Application), None, None, None, None)
            .unwrap();
        assert_eq!(messages.len(), 2);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_history_messages_stored_correctly() {
        let wallet = generate_local_wallet();
        let amal_a = ClientBuilder::new_test_client(&wallet).await;
        let amal_b = ClientBuilder::new_test_client(&wallet).await;
        assert_ok!(amal_b.enable_history_sync().await);

        amal_a.sync_welcomes().await.expect("sync_welcomes");

        let (_group_id, _pin_code) = amal_b
            .send_history_request()
            .await
            .expect("history request");

        // find the sync group
        let amal_a_sync_groups = amal_a.store().conn().unwrap().find_sync_groups().unwrap();
        assert_eq!(amal_a_sync_groups.len(), 1);
        // get the first sync group
        let amal_a_sync_group = amal_a.group(amal_a_sync_groups[0].id.clone()).unwrap();
        amal_a_sync_group.sync(&amal_a).await.expect("sync");

        // find the sync group (it should be the same as amal_a's sync group)
        let amal_b_sync_groups = amal_b.store().conn().unwrap().find_sync_groups().unwrap();
        assert_eq!(amal_b_sync_groups.len(), 1);
        // get the first sync group
        let amal_b_sync_group = amal_b.group(amal_b_sync_groups[0].id.clone()).unwrap();
        amal_b_sync_group.sync(&amal_b).await.expect("sync");

        // make sure they are the same group
        assert_eq!(amal_a_sync_group.group_id, amal_b_sync_group.group_id);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    #[ignore] // this test is only relevant if we are enforcing the PIN challenge
    async fn test_verify_pin() {
        let wallet = generate_local_wallet();
        let amal_a = ClientBuilder::new_test_client(&wallet).await;
        let amal_b = ClientBuilder::new_test_client(&wallet).await;
        assert_ok!(amal_b.enable_history_sync().await);

        amal_a.sync_welcomes().await.expect("sync_welcomes");

        let (request_id, pin_code) = amal_b
            .send_history_request()
            .await
            .expect("history request");

        let amal_a_sync_groups = amal_a.store().conn().unwrap().find_sync_groups().unwrap();
        assert_eq!(amal_a_sync_groups.len(), 1);
        // get the first sync group
        let amal_a_sync_group = amal_a.group(amal_a_sync_groups[0].id.clone()).unwrap();
        amal_a_sync_group.sync(&amal_a).await.expect("sync");
        let pin_challenge_result = amal_a.verify_pin(&request_id, &pin_code);
        assert_ok!(pin_challenge_result);

        let pin_challenge_result_2 = amal_a.verify_pin("000", "000");
        assert!(pin_challenge_result_2.is_err());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    #[ignore]
    async fn test_request_reply_roundtrip() {
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

        let groups = amal_a.prepare_groups_to_sync().await.unwrap();

        let input_file = NamedTempFile::new().unwrap();
        let input_path = input_file.path();
        write_to_file(input_path, groups).unwrap();

        let output_file = NamedTempFile::new().unwrap();
        let output_path = output_file.path();
        let encryption_key = HistoryKeyType::new_chacha20_poly1305_key();
        encrypt_history_file(input_path, output_path, encryption_key.as_bytes()).unwrap();

        let mut file = File::open(output_path).unwrap();
        let mut content = Vec::new();
        file.read_to_end(&mut content).unwrap();

        let _m = server
            .mock("GET", "/upload")
            .with_status(201)
            .with_body(content)
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
        amal_a_sync_group.sync(&amal_a).await.expect("sync");

        // amal_a builds and sends a message history reply back
        let history_reply = HistoryReply::new(&new_request_id(), &history_sync_url, encryption_key);
        amal_a
            .send_history_reply(history_reply.into())
            .await
            .expect("send reply");

        amal_a_sync_group.sync(&amal_a).await.expect("sync");
        // amal_b should have received the reply
        let amal_b_sync_groups = amal_b.store().conn().unwrap().find_sync_groups().unwrap();
        assert_eq!(amal_b_sync_groups.len(), 1);

        let amal_b_sync_group = amal_b.group(amal_b_sync_groups[0].id.clone()).unwrap();
        amal_b_sync_group.sync(&amal_b).await.expect("sync");

        let amal_b_conn = amal_b.store().conn().unwrap();
        let amal_b_messages = amal_b_conn
            .get_group_messages(amal_b_sync_group.group_id, None, None, None, None, None)
            .unwrap();

        assert_eq!(amal_b_messages.len(), 1);
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

        let result = amal_a.prepare_groups_to_sync().await.unwrap();
        assert_eq!(result.len(), 2);
    }

    #[tokio::test]
    async fn test_prepare_group_messages_to_sync() {
        let wallet = generate_local_wallet();
        let amal_a = ClientBuilder::new_test_client(&wallet).await;
        let group_a = amal_a
            .create_group(None, GroupMetadataOptions::default())
            .expect("create group");
        let group_b = amal_a
            .create_group(None, GroupMetadataOptions::default())
            .expect("create group");

        group_a.send_message(b"hi", &amal_a).await.expect("send");
        group_a.send_message(b"hi x2", &amal_a).await.expect("send");
        group_b.send_message(b"hi", &amal_a).await.expect("send");
        group_b.send_message(b"hi x2", &amal_a).await.expect("send");

        let messages_result = amal_a.prepare_messages_to_sync().await.unwrap();
        assert_eq!(messages_result.len(), 4);
    }

    #[tokio::test]
    async fn test_write_to_file() {
        let wallet = generate_local_wallet();
        let amal_a = ClientBuilder::new_test_client(&wallet).await;
        let group_a = amal_a
            .create_group(None, GroupMetadataOptions::default())
            .expect("create group");
        let group_b = amal_a
            .create_group(None, GroupMetadataOptions::default())
            .expect("create group");

        group_a.send_message(b"hi", &amal_a).await.expect("send");
        group_a.send_message(b"hi", &amal_a).await.expect("send");
        group_b.send_message(b"hi", &amal_a).await.expect("send");
        group_b.send_message(b"hi", &amal_a).await.expect("send");

        let groups = amal_a.prepare_groups_to_sync().await.unwrap();
        let messages = amal_a.prepare_messages_to_sync().await.unwrap();

        let temp_file = NamedTempFile::new().expect("Unable to create temp file");
        let wrote_groups = write_to_file(temp_file.path(), groups);
        assert!(wrote_groups.is_ok());
        let wrote_messages = write_to_file(temp_file.path(), messages);
        assert!(wrote_messages.is_ok());

        let file = File::open(temp_file.path()).expect("Unable to open test file");
        let reader = BufReader::new(file);
        let n_lines_written = reader.lines().count();
        assert_eq!(n_lines_written, 6);

        std::fs::remove_file(temp_file).expect("Unable to remove test file");
    }

    #[test]
    fn test_encrypt_decrypt_file() {
        let key = HistoryKeyType::new_chacha20_poly1305_key();
        let converted_key: MessageHistoryKeyType = key.into();
        let key_bytes = key.as_bytes();
        let input_content = b"'{\"test\": \"data\"}\n{\"test\": \"data2\"}\n'";
        let input_file = NamedTempFile::new().expect("Unable to create temp file");
        let encrypted_file = NamedTempFile::new().expect("Unable to create temp file");
        let decrypted_file = NamedTempFile::new().expect("Unable to create temp file");

        // Write test input file
        std::fs::write(input_file.path(), input_content).expect("Unable to write test input file");

        // Encrypt the file
        encrypt_history_file(input_file.path(), encrypted_file.path(), key_bytes)
            .expect("Encryption failed");

        // Decrypt the file
        decrypt_history_file(encrypted_file.path(), decrypted_file.path(), converted_key)
            .expect("Decryption failed");

        // Read the decrypted file content
        let decrypted_content =
            std::fs::read(decrypted_file.path()).expect("Unable to read decrypted file");

        // Assert the decrypted content is the same as the original input content
        assert_eq!(decrypted_content, input_content);

        // Clean up test files
        std::fs::remove_file(input_file).expect("Unable to remove test input file");
        std::fs::remove_file(encrypted_file).expect("Unable to remove test encrypted file");
        std::fs::remove_file(decrypted_file).expect("Unable to remove test decrypted file");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_upload_history_bundle() {
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

        let file_content = b"'{\"test\": \"data\"}\n{\"test\": \"data2\"}\n'";

        let mut file = NamedTempFile::new().unwrap();
        file.write_all(file_content).unwrap();
        let file_path = file.path().to_str().unwrap().to_string();

        let url = format!(
            "http://{}:{}/upload",
            HISTORY_SERVER_HOST,
            HISTORY_SERVER_PORT + 1
        );
        let result = upload_history_bundle(&url, file_path.into()).await;

        assert!(result.is_ok());
        _m.assert_async().await;
        server.reset();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_download_history_bundle() {
        let bundle_id = "test_bundle_id";
        let options = mockito::ServerOpts {
            host: HISTORY_SERVER_HOST,
            port: HISTORY_SERVER_PORT,
            ..Default::default()
        };
        let mut server = mockito::Server::new_with_opts_async(options).await;

        let _m = server
            .mock("GET", format!("/files/{}", bundle_id).as_str())
            .with_status(200)
            .with_body("encrypted_content")
            .create();

        let url = format!(
            "http://{}:{}/files/{bundle_id}",
            HISTORY_SERVER_HOST, HISTORY_SERVER_PORT
        );
        let output_path = download_history_bundle(&url)
            .await
            .expect("could not download history bundle");

        _m.assert_async().await;
        std::fs::remove_file(output_path.as_path()).expect("Unable to remove test output file");
        server.reset();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_prepare_history_reply() {
        let wallet = generate_local_wallet();
        let mut amal_a = ClientBuilder::new_test_client(&wallet).await;
        let amal_b = ClientBuilder::new_test_client(&wallet).await;
        assert_ok!(amal_b.enable_history_sync().await);

        amal_a.sync_welcomes().await.expect("sync_welcomes");

        let request_id = new_request_id();

        let port = HISTORY_SERVER_PORT + 2;
        let options = mockito::ServerOpts {
            host: HISTORY_SERVER_HOST,
            port,
            ..Default::default()
        };
        let mut server = mockito::Server::new_with_opts_async(options).await;

        let url = format!("http://{HISTORY_SERVER_HOST}:{port}/");
        let _m = server
            .mock("POST", "/upload")
            .with_status(201)
            .with_body("encrypted_content")
            .create();

        amal_a.history_sync_url = Some(url);
        let reply = amal_a.prepare_history_reply(&request_id).await;
        assert!(reply.is_ok());
        _m.assert_async().await;
        server.reset();
    }

    #[tokio::test]
    async fn test_get_pending_history_request() {
        let wallet = generate_local_wallet();
        let amal_a = ClientBuilder::new_test_client(&wallet).await;

        // enable history sync for the client
        assert_ok!(amal_a.enable_history_sync().await);

        // ensure there's no pending request initially
        let initial_request = amal_a.get_pending_history_request().await;
        assert!(initial_request.is_ok());
        assert!(initial_request.unwrap().is_none());

        // create a history request
        let request = amal_a
            .send_history_request()
            .await
            .expect("history request");

        // check for the pending request
        let pending_request = amal_a.get_pending_history_request().await;
        assert!(pending_request.is_ok());
        let pending = pending_request.unwrap();
        assert!(pending.is_some());

        let (request_id, pin_code) = pending.unwrap();
        assert_eq!(request_id, request.0);
        assert_eq!(pin_code, request.1);
    }

    #[tokio::test]
    async fn test_get_latest_history_reply() {
        let wallet = generate_local_wallet();
        let amal_a = ClientBuilder::new_test_client(&wallet).await;
        let amal_b = ClientBuilder::new_test_client(&wallet).await;

        // enable history sync for both clients
        assert_ok!(amal_a.enable_history_sync().await);
        assert_ok!(amal_b.enable_history_sync().await);

        // ensure there's no reply initially
        let initial_reply = amal_b.get_latest_history_reply().await;
        assert!(initial_reply.is_ok());
        assert!(initial_reply.unwrap().is_none());

        // amal_b sends a history request
        let (request_id, _pin_code) = amal_b
            .send_history_request()
            .await
            .expect("history request");

        // sync amal_a
        amal_a.sync_welcomes().await.expect("sync_welcomes");

        // amal_a sends a reply
        amal_a
            .send_history_reply(MessageHistoryReply {
                request_id: request_id.clone(),
                url: "http://foo/bar".to_string(),
                encryption_key: None,
            })
            .await
            .expect("send reply");

        // check latest reply for amal_b
        let latest_reply = amal_b.get_latest_history_reply().await;
        assert!(latest_reply.is_ok());
        let received_reply = latest_reply.unwrap();
        assert!(received_reply.is_some());

        let received_reply = received_reply.unwrap();
        assert_eq!(received_reply.request_id, request_id);
    }

    #[tokio::test]
    async fn test_reply_to_history_request() {
        let wallet = generate_local_wallet();
        let mut amal_a = ClientBuilder::new_test_client(&wallet).await;
        let amal_b = ClientBuilder::new_test_client(&wallet).await;

        // enable history sync for both clients
        assert_ok!(amal_a.enable_history_sync().await);
        assert_ok!(amal_b.enable_history_sync().await);

        // amal_b sends a history request
        let (request_id, _pin_code) = amal_b
            .send_history_request()
            .await
            .expect("history request");

        // sync amal_a
        amal_a.sync_welcomes().await.expect("sync_welcomes");

        // start mock server
        let options = mockito::ServerOpts {
            host: HISTORY_SERVER_HOST,
            port: HISTORY_SERVER_PORT + 3,
            ..Default::default()
        };
        let mut server = mockito::Server::new_with_opts_async(options).await;

        let _m = server
            .mock("POST", "/upload")
            .with_status(201)
            .with_body("File uploaded")
            .create();

        let url = format!(
            "http://{}:{}/",
            HISTORY_SERVER_HOST,
            HISTORY_SERVER_PORT + 3
        );
        amal_a.history_sync_url = Some(url);

        // amal_a replies to the history request
        let reply = amal_a.reply_to_history_request().await;
        assert!(reply.is_ok());
        let reply = reply.unwrap();

        // verify the reply
        assert_eq!(reply.request_id, request_id);
        assert!(!reply.url.is_empty());
        assert!(reply.encryption_key.is_some());

        // check if amal_b received the reply
        let received_reply = amal_b.get_latest_history_reply().await;
        assert!(received_reply.is_ok());
        let received_reply = received_reply.unwrap();
        assert!(received_reply.is_some());
        let received_reply = received_reply.unwrap();
        assert_eq!(received_reply.request_id, request_id);
        assert_eq!(received_reply.url, reply.url);
        assert_eq!(received_reply.encryption_key, reply.encryption_key);

        _m.assert_async().await;
        server.reset();
    }

    #[tokio::test]
    async fn test_insert_history_bundle() {
        let wallet = generate_local_wallet();
        let amal_a = ClientBuilder::new_test_client(&wallet).await;
        let amal_b = ClientBuilder::new_test_client(&wallet).await;
        let group_a = amal_a
            .create_group(None, GroupMetadataOptions::default())
            .expect("create group");

        group_a
            .send_message(b"hi", &amal_a)
            .await
            .expect("send message");

        let (bundle_path, enc_key) = amal_a
            .write_history_bundle()
            .await
            .expect("Unable to write history bundle");

        let output_file = NamedTempFile::new().expect("Unable to create temp file");
        let converted_key: MessageHistoryKeyType = enc_key.into();
        decrypt_history_file(&bundle_path, output_file.path(), converted_key)
            .expect("Unable to decrypt history file");

        let inserted = amal_b.insert_history_bundle(output_file.path());
        assert!(inserted.is_ok());
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
        let group = amal.group(sync_group_id).expect("get group");
        let result = group
            .add_members(&external_client, vec![external_wallet.get_address()])
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
        let sig_key = HistoryKeyType::new_chacha20_poly1305_key();
        let enc_key = HistoryKeyType::new_chacha20_poly1305_key();
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
