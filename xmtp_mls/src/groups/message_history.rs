use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};

use aes_gcm::aead::generic_array::GenericArray;
use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm,
};
use prost::Message;
use rand::{
    distributions::{Alphanumeric, DistString},
    Rng, RngCore,
};
use ring::hmac;
use serde::Deserialize;
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

use super::GroupError;

use crate::XmtpApi;
use crate::{
    configuration::DELIMITER,
    groups::{intents::SendMessageIntentData, GroupMessageKind, StoredGroupMessage},
    storage::{
        group::StoredGroup,
        group_intent::{IntentKind, NewGroupIntent},
        StorageError,
    },
    Client, Store,
};

const ENC_KEY_SIZE: usize = 32; // 256-bit key
const NONCE_SIZE: usize = 12; // 96-bit nonce

#[derive(Debug, Error)]
pub enum MessageHistoryError {
    #[error("pin not found")]
    PinNotFound,
    #[error("pin does not match the expected value")]
    PinMismatch,
    #[error("IO error: {0}")]
    IO(#[from] std::io::Error),
    #[error("JSON serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
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
    pub async fn allow_history_sync(&self) -> Result<(), GroupError> {
        let history_sync_group = self.create_sync_group()?;
        history_sync_group.sync(self).await?;
        Ok(())
    }

    pub async fn ensure_member_of_all_groups(&self, inbox_id: String) -> Result<(), GroupError> {
        let conn = self.store().conn()?;
        let groups = conn.find_groups(None, None, None, None)?;
        for group in groups {
            let group = self.group(group.id)?;
            Box::pin(group.add_members_by_inbox_id(self, vec![inbox_id.clone()])).await?;
        }

        Ok(())
    }

    pub(crate) async fn send_history_request(&self) -> Result<String, GroupError> {
        // find the sync group
        let conn = self.store().conn()?;
        let sync_group_id = conn
            .find_sync_groups()?
            .pop()
            .ok_or(GroupError::GroupNotFound)?
            .id;
        let sync_group = self.group(sync_group_id.clone())?;

        // build the request
        let history_request = HistoryRequest::new();
        let pin_code = history_request.pin_code.clone();
        let idempotency_key = new_request_id();
        let envelope = PlaintextEnvelope {
            content: Some(Content::V2(V2 {
                message_type: Some(Request(history_request.into())),
                idempotency_key,
            })),
        };

        // build the intent
        let mut encoded_envelope = vec![];
        envelope
            .encode(&mut encoded_envelope)
            .map_err(GroupError::EncodeError)?;
        let intent_data: Vec<u8> = SendMessageIntentData::new(encoded_envelope).into();
        let intent = NewGroupIntent::new(IntentKind::SendMessage, sync_group_id, intent_data);
        intent.store(&conn)?;

        // publish the intent
        if let Err(err) = sync_group.publish_intents(conn, self).await {
            log::error!("error publishing sync group intents: {:?}", err);
        }

        Ok(pin_code)
    }

    pub(crate) async fn send_history_reply(
        &self,
        contents: MessageHistoryReply,
    ) -> Result<(), GroupError> {
        // find the sync group
        let conn = self.store().conn()?;
        let sync_group_id = conn
            .find_sync_groups()?
            .pop()
            .ok_or(GroupError::GroupNotFound)?
            .id;

        // build the reply
        let envelope = PlaintextEnvelope {
            content: Some(Content::V2(V2 {
                idempotency_key: new_request_id(),
                message_type: Some(Reply(contents)),
            })),
        };

        // build the intent
        let mut encoded_envelope = vec![];
        envelope
            .encode(&mut encoded_envelope)
            .map_err(GroupError::EncodeError)?;
        let intent_data: Vec<u8> = SendMessageIntentData::new(encoded_envelope).into();
        let intent =
            NewGroupIntent::new(IntentKind::SendMessage, sync_group_id.clone(), intent_data);
        intent.store(&conn)?;

        // publish the intent
        let sync_group = self.group(sync_group_id)?;
        if let Err(err) = sync_group.publish_intents(conn, self).await {
            log::error!("error publishing sync group intents: {:?}", err);
        }
        Ok(())
    }

    pub(crate) fn provide_pin(&self, pin_challenge: &str) -> Result<(), GroupError> {
        let conn = self.store().conn()?;
        let sync_group_id = conn
            .find_sync_groups()?
            .pop()
            .ok_or(GroupError::GroupNotFound)?
            .id;

        let requests = conn.get_group_messages(
            sync_group_id,
            None,
            None,
            Some(GroupMessageKind::Application),
            None,
            None,
        )?;
        let request = requests.into_iter().find(|msg| {
            let msg_bytes = &msg.decrypted_message_bytes;
            match msg_bytes.iter().position(|&idx| idx == DELIMITER as u8) {
                Some(index) => {
                    let (_id_part, pin_part) = msg_bytes.split_at(index);
                    let pin = String::from_utf8_lossy(&pin_part[1..]);
                    verify_pin(&pin, pin_challenge)
                }
                None => false,
            }
        });
        if request.is_none() {
            return Err(GroupError::MessageHistory(MessageHistoryError::PinNotFound));
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
                    conn.insert_or_ignore_group(group)?;
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
        url: &str,
    ) -> Result<HistoryReply, MessageHistoryError> {
        let (history_file, enc_key) = self.write_history_bundle().await?;

        let signing_key = HistoryKeyType::new_chacha20_poly1305_key();
        upload_history_bundle(url, history_file.clone(), signing_key.as_bytes()).await?;

        let history_reply = HistoryReply::new(
            request_id,
            url,
            signing_key.as_bytes().to_vec(),
            signing_key,
            enc_key,
        );

        Ok(history_reply)
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
        let groups = conn.find_groups(None, None, None, None)?;
        let mut all_groups: Vec<StoredGroup> = vec![];

        for group in groups.into_iter() {
            all_groups.push(group);
        }

        Ok(all_groups)
    }

    async fn prepare_messages_to_sync(
        &self,
    ) -> Result<Vec<StoredGroupMessage>, MessageHistoryError> {
        let conn = self.store().conn()?;
        let groups = conn.find_groups(None, None, None, None)?;
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
    encryption_key: &[u8; 32],
) -> Result<(), MessageHistoryError> {
    // let enc_key: HistoryKeyType = encryption_key.try_into()?;
    // let enc_key_bytes = enc_key.as_bytes();

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
    signing_key: &[u8],
) -> Result<(), MessageHistoryError> {
    let mut file = File::open(file_path)?;
    let mut content = Vec::new();
    file.read_to_end(&mut content)?;

    let key = hmac::Key::new(hmac::HMAC_SHA256, signing_key);
    let tag = hmac::sign(&key, &content);
    let hmac_hex = hex::encode(tag.as_ref());

    let client = reqwest::Client::new();
    let _response = client
        .post(url)
        .header("X-HMAC", hmac_hex)
        .body(content)
        .send()
        .await?;

    Ok(())
}

pub(crate) async fn download_history_bundle(
    url: &str,
    hmac_value: Vec<u8>,
    signing_key: MessageHistoryKeyType,
) -> Result<PathBuf, MessageHistoryError> {
    let sign_key: HistoryKeyType = signing_key.try_into()?;
    let sign_key_bytes = sign_key.as_bytes().to_vec();
    let client = reqwest::Client::new();
    let response = client
        .get(url)
        .header("X-HMAC", hmac_value)
        .header("X-SIGNING-KEY", hex::encode(sign_key_bytes))
        .send()
        .await?;

    // let output_path = std::env::temp_dir().join("messages.jsonl");
    if response.status().is_success() {
        let file_path = std::env::temp_dir().join("downloaded_bundle.jsonl.enc");
        let mut file = File::create(&file_path)?;
        let bytes = response.bytes().await?;
        file.write_all(&bytes)?;
        Ok(file_path)

        // decrypt_history_file(&input_path, &output_path, enc_key.as_bytes())?;
    } else {
        eprintln!(
            "Failed to download file. Status code: {} Response: {:?}",
            response.status(),
            response
        );
        Err(MessageHistoryError::Reqwest(
            response.error_for_status().unwrap_err(),
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

#[derive(Debug)]
pub(crate) struct HistoryReply {
    /// Unique ID for each client Message History Request
    request_id: String,
    /// URL to download the backup bundle
    url: String,
    /// HMAC value of the backup bundle
    bundle_hash: Vec<u8>,
    /// HMAC Signing key for the backup bundle
    signing_key: HistoryKeyType,
    /// Encryption key for the backup bundle
    encryption_key: HistoryKeyType,
}

impl HistoryReply {
    pub(crate) fn new(
        id: &str,
        url: &str,
        bundle_hash: Vec<u8>,
        signing_key: HistoryKeyType,
        encryption_key: HistoryKeyType,
    ) -> Self {
        Self {
            request_id: id.into(),
            url: url.into(),
            bundle_hash,
            signing_key,
            encryption_key,
        }
    }
}

impl From<HistoryReply> for MessageHistoryReply {
    fn from(reply: HistoryReply) -> Self {
        MessageHistoryReply {
            request_id: reply.request_id,
            url: reply.url,
            bundle_hash: reply.bundle_hash,
            signing_key: Some(reply.signing_key.into()),
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

// Yes, this is a just a simple string comparison.
// If we need to add more complex logic, we can do so here.
// For example if we want to add a time limit or enforce a certain number of attempts.
fn verify_pin(expected: &str, actual: &str) -> bool {
    expected.eq(actual)
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

    use crate::{assert_ok, builder::ClientBuilder};

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_allow_history_sync() {
        let wallet = generate_local_wallet();
        let client = ClientBuilder::new_test_client(&wallet).await;
        assert_ok!(client.allow_history_sync().await);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_installations_are_added_to_sync_group() {
        let wallet = generate_local_wallet();
        let amal_a = ClientBuilder::new_test_client(&wallet).await;
        let amal_b = ClientBuilder::new_test_client(&wallet).await;
        let amal_c = ClientBuilder::new_test_client(&wallet).await;
        assert_ok!(amal_c.allow_history_sync().await);

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
        assert_ok!(client.allow_history_sync().await);

        // test that the request is sent, and that the pin code is returned
        let pin_code = client
            .send_history_request()
            .await
            .expect("history request");
        assert_eq!(pin_code.len(), 4);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_send_history_reply() {
        let wallet = generate_local_wallet();
        let client = ClientBuilder::new_test_client(&wallet).await;
        assert_ok!(client.allow_history_sync().await);

        let request_id = new_request_id();
        let url = "https://test.com/abc-123";
        let backup_hash = b"ABC123".into();
        let signing_key = HistoryKeyType::new_chacha20_poly1305_key();
        let encryption_key = HistoryKeyType::new_chacha20_poly1305_key();
        let reply = HistoryReply::new(&request_id, url, backup_hash, signing_key, encryption_key);
        let result = client.send_history_reply(reply.into()).await;
        assert_ok!(result);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_history_messages_stored_correctly() {
        let wallet = generate_local_wallet();
        let amal_a = ClientBuilder::new_test_client(&wallet).await;
        let amal_b = ClientBuilder::new_test_client(&wallet).await;
        assert_ok!(amal_b.allow_history_sync().await);

        amal_a.sync_welcomes().await.expect("sync_welcomes");

        let _sent = amal_b
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
    async fn test_provide_pin_challenge() {
        let wallet = generate_local_wallet();
        let amal_a = ClientBuilder::new_test_client(&wallet).await;
        let amal_b = ClientBuilder::new_test_client(&wallet).await;
        assert_ok!(amal_b.allow_history_sync().await);

        amal_a.sync_welcomes().await.expect("sync_welcomes");

        let pin_code = amal_b
            .send_history_request()
            .await
            .expect("history request");

        let amal_a_sync_groups = amal_a.store().conn().unwrap().find_sync_groups().unwrap();
        assert_eq!(amal_a_sync_groups.len(), 1);
        // get the first sync group
        let amal_a_sync_group = amal_a.group(amal_a_sync_groups[0].id.clone()).unwrap();
        amal_a_sync_group.sync(&amal_a).await.expect("sync");
        let pin_challenge_result = amal_a.provide_pin(&pin_code);
        assert_ok!(pin_challenge_result);

        let pin_challenge_result_2 = amal_a.provide_pin("000");
        assert!(pin_challenge_result_2.is_err());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    #[ignore]
    async fn test_request_reply_roundtrip() {
        let wallet = generate_local_wallet();
        let amal_a = ClientBuilder::new_test_client(&wallet).await;
        let amal_b = ClientBuilder::new_test_client(&wallet).await;
        assert_ok!(amal_b.allow_history_sync().await);

        amal_a.sync_welcomes().await.expect("sync_welcomes");

        // amal_b sends a message history request to sync group messages
        let _pin_code = amal_b
            .send_history_request()
            .await
            .expect("history request");

        let amal_a_sync_groups = amal_a.store().conn().unwrap().find_sync_groups().unwrap();
        assert_eq!(amal_a_sync_groups.len(), 1);
        // get the first sync group
        let amal_a_sync_group = amal_a.group(amal_a_sync_groups[0].id.clone()).unwrap();
        amal_a_sync_group.sync(&amal_a).await.expect("sync");

        // amal_a builds and sends a message history reply back
        let history_reply = HistoryReply::new(
            "test",
            "https://test.com/abc-123",
            b"ABC123".into(),
            HistoryKeyType::new_chacha20_poly1305_key(),
            HistoryKeyType::new_chacha20_poly1305_key(),
        );
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
        let _group_a = amal_a.create_group(None).expect("create group");
        let _group_b = amal_a.create_group(None).expect("create group");

        let result = amal_a.prepare_groups_to_sync().await.unwrap();
        assert_eq!(result.len(), 2);
    }

    #[tokio::test]
    async fn test_prepare_group_messages_to_sync() {
        let wallet = generate_local_wallet();
        let amal_a = ClientBuilder::new_test_client(&wallet).await;
        let group_a = amal_a.create_group(None).expect("create group");
        let group_b = amal_a.create_group(None).expect("create group");

        group_a
            .send_message(b"hi", &amal_a)
            .await
            .expect("send message");
        group_a
            .send_message(b"hi x2", &amal_a)
            .await
            .expect("send message");
        group_b
            .send_message(b"hi", &amal_a)
            .await
            .expect("send message");
        group_b
            .send_message(b"hi x2", &amal_a)
            .await
            .expect("send message");

        let messages_result = amal_a.prepare_messages_to_sync().await.unwrap();
        assert_eq!(messages_result.len(), 4);
    }

    #[tokio::test]
    async fn test_write_to_file() {
        let wallet = generate_local_wallet();
        let amal_a = ClientBuilder::new_test_client(&wallet).await;
        let group_a = amal_a.create_group(None).expect("create group");
        let group_b = amal_a.create_group(None).expect("create group");

        group_a
            .send_message(b"hi", &amal_a)
            .await
            .expect("send message");
        group_a
            .send_message(b"hi", &amal_a)
            .await
            .expect("send message");
        group_b
            .send_message(b"hi", &amal_a)
            .await
            .expect("send message");
        group_b
            .send_message(b"hi", &amal_a)
            .await
            .expect("send message");

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
        let converted_key: MessageHistoryKeyType = key.try_into().expect("Unable to convert key");
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
            .with_status(200)
            .with_body("File uploaded")
            .create();

        let signing_key = b"test_signing_key";
        let file_content = b"test file content";

        let mut file = NamedTempFile::new().unwrap();
        file.write_all(file_content).unwrap();
        let file_path = file.path().to_str().unwrap().to_string();

        let url = format!(
            "http://{}:{}/upload",
            HISTORY_SERVER_HOST,
            HISTORY_SERVER_PORT + 1
        );
        let result = upload_history_bundle(&url, file_path.into(), signing_key).await;

        assert!(result.is_ok());
        _m.assert_async().await;
        server.reset();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_download_history_bundle() {
        let bundle_id = "test_bundle_id";
        let hmac_value = "test_hmac_value";
        let signing_key = HistoryKeyType::new_chacha20_poly1305_key();
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
        let output_path = download_history_bundle(&url, hmac_value.into(), signing_key.into())
            .await
            .expect("could not download history bundle");

        _m.assert_async().await;
        std::fs::remove_file(output_path.as_path()).expect("Unable to remove test output file");
        server.reset();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_prepare_history_reply() {
        let wallet = generate_local_wallet();
        let amal_a = ClientBuilder::new_test_client(&wallet).await;
        let amal_b = ClientBuilder::new_test_client(&wallet).await;
        assert_ok!(amal_b.allow_history_sync().await);

        amal_a.sync_welcomes().await.expect("sync_welcomes");

        let request_id = new_request_id();

        let port = HISTORY_SERVER_PORT + 2;
        let options = mockito::ServerOpts {
            host: HISTORY_SERVER_HOST,
            port,
            ..Default::default()
        };
        let mut server = mockito::Server::new_with_opts_async(options).await;

        let url = format!("http://{HISTORY_SERVER_HOST}:{port}/upload");
        let _m = server
            .mock("POST", "/upload")
            .with_status(201)
            .with_body("encrypted_content")
            .create();

        let reply = amal_a.prepare_history_reply(&request_id, &url).await;
        assert!(reply.is_ok());
        _m.assert_async().await;
        server.reset();
    }

    #[tokio::test]
    async fn test_insert_history_bundle() {
        let wallet = generate_local_wallet();
        let amal_a = ClientBuilder::new_test_client(&wallet).await;
        let amal_b = ClientBuilder::new_test_client(&wallet).await;
        let group_a = amal_a.create_group(None).expect("create group");

        group_a
            .send_message(b"hi", &amal_a)
            .await
            .expect("send message");

        let (bundle_path, enc_key) = amal_a
            .write_history_bundle()
            .await
            .expect("Unable to write history bundle");

        let output_file = NamedTempFile::new().expect("Unable to create temp file");
        let converted_key: MessageHistoryKeyType =
            enc_key.try_into().expect("Unable to convert key");
        decrypt_history_file(&bundle_path, output_file.path(), converted_key)
            .expect("Unable to decrypt history file");

        let inserted = amal_b.insert_history_bundle(output_file.path());
        assert!(inserted.is_ok());
    }

    #[test]
    fn test_new_pin() {
        let pin = new_pin();
        assert!(pin.chars().all(|c| c.is_numeric()));
        assert_eq!(pin.len(), 4);
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
