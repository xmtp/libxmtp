use std::fs::File;
use std::io::{Read, Write};
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
use xmtp_proto::xmtp::mls::message_contents::DeviceSyncKeyType as DeviceSyncKeyTypeProto;
use xmtp_proto::xmtp::mls::message_contents::{
    device_sync_key_type::Key, DeviceSyncReply as DeviceSyncReplyProto,
    DeviceSyncRequest as DeviceSyncRequestProto,
};

use super::group_metadata::ConversationType;
use super::{GroupError, MlsGroup};

use crate::Client;
use crate::{client::ClientError, storage::StorageError};

#[cfg(feature = "consent-sync")]
pub mod consent;
#[cfg(feature = "message-history")]
pub mod messages;

pub const ENC_KEY_SIZE: usize = 32; // 256-bit key
pub const NONCE_SIZE: usize = 12; // 96-bit nonce

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
) -> Result<(), MessageHistoryError> {
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

#[derive(Clone, Debug)]
pub(super) struct DeviceSyncRequest {
    pub pin_code: String,
    pub request_id: String,
}

impl DeviceSyncRequest {
    pub(crate) fn new() -> Self {
        Self {
            pin_code: new_pin(),
            request_id: new_request_id(),
        }
    }
}

impl From<DeviceSyncRequest> for DeviceSyncRequestProto {
    fn from(req: DeviceSyncRequest) -> Self {
        DeviceSyncRequestProto {
            pin_code: req.pin_code,
            request_id: req.request_id,
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
                key: Some(Key::Chacha20Poly1305(key.to_vec())),
            },
        }
    }
}

impl TryFrom<DeviceSyncKeyTypeProto> for DeviceSyncKeyType {
    type Error = MessageHistoryError;
    fn try_from(key: DeviceSyncKeyTypeProto) -> Result<Self, Self::Error> {
        let DeviceSyncKeyTypeProto { key } = key;
        match key {
            Some(k) => {
                let Key::Chacha20Poly1305(hist_key) = k;
                match hist_key.try_into() {
                    Ok(array) => Ok(DeviceSyncKeyType::Chacha20Poly1305(array)),
                    Err(_) => Err(MessageHistoryError::Conversion),
                }
            }
            None => Err(MessageHistoryError::Conversion),
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
