use std::fs::File;
use std::io::{Read, Write};
use std::path::PathBuf;

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
    client::ClientError,
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
    #[error("Pin not found")]
    PinNotFound,
    #[error("Pin does not match the expected value")]
    PinMismatch,
    #[error("IO error: {0}")]
    IO(#[from] std::io::Error),
    #[error("JSON Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("AES-GCM Encryption error")]
    AesGcm(#[from] aes_gcm::Error),
}

impl<ApiClient> Client<ApiClient>
where
    ApiClient: XmtpApi,
{
    pub async fn allow_history_sync(&self) -> Result<(), ClientError> {
        let history_sync_group = self.create_sync_group()?;
        history_sync_group
            .sync(self)
            .await
            .map_err(|e| ClientError::Generic(e.to_string()))?;
        Ok(())
    }

    #[allow(dead_code)]
    pub(crate) async fn send_message_history_request(&self) -> Result<String, GroupError> {
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

    #[allow(dead_code)]
    pub(crate) async fn send_message_history_reply(
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
        let sync_group = self.group(sync_group_id.clone())?;

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
        let intent = NewGroupIntent::new(IntentKind::SendMessage, sync_group_id, intent_data);
        intent.store(&conn)?;

        // publish the intent
        if let Err(err) = sync_group.publish_intents(conn, self).await {
            log::error!("error publishing sync group intents: {:?}", err);
        }
        Ok(())
    }

    #[allow(dead_code)]
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

    #[allow(dead_code)]
    pub(crate) async fn prepare_messages_to_sync(
        &self,
    ) -> Result<Vec<StoredGroupMessage>, StorageError> {
        let conn = self.store().conn()?;
        let groups = conn.find_groups(None, None, None, None)?;
        let mut all_messages: Vec<StoredGroupMessage> = vec![];

        for StoredGroup { id, .. } in groups.clone() {
            let messages = conn.get_group_messages(id, None, None, None, None, None)?;
            all_messages.extend(messages);
        }

        Ok(all_messages)
    }
}

#[allow(dead_code)]
fn write_messages_file(messages: Vec<StoredGroupMessage>) -> Result<PathBuf, MessageHistoryError> {
    let tmp_dir = std::env::temp_dir();
    let file_path = tmp_dir.join("messages.jsonl");
    let mut file = std::fs::File::create(&file_path)?;
    for message in messages {
        let message_str = serde_json::to_string(&message)?;
        file.write_all(message_str.as_bytes())?;
    }
    Ok(file_path)
}

#[allow(dead_code)]
fn encrypt_messages_file(
    input_path: &str,
    output_path: &str,
    key: &[u8; ENC_KEY_SIZE],
) -> Result<(), MessageHistoryError> {
    // Read in the messages file content
    let mut input_file = File::open(input_path)?;
    let mut buffer = Vec::new();
    input_file.read_to_end(&mut buffer)?;

    let nonce = generate_nonce();

    // Create a cipher instance
    let cipher = Aes256Gcm::new(GenericArray::from_slice(key));
    let nonce_array = GenericArray::from_slice(&nonce);

    // Encrypt the file content
    let ciphertext = cipher.encrypt(nonce_array, buffer.as_ref())?;

    // Write the nonce and ciphertext to the output file
    let mut output_file = File::create(output_path)?;
    output_file.write_all(&nonce)?;
    output_file.write_all(&ciphertext)?;

    Ok(())
}

#[allow(dead_code)]
fn decrypt_messages_file(
    input_path: &str,
    output_path: &str,
    key: &[u8; ENC_KEY_SIZE],
) -> Result<(), MessageHistoryError> {
    // Read the file content
    let mut input_file = File::open(input_path)?;
    let mut buffer = Vec::new();
    input_file.read_to_end(&mut buffer)?;

    // Split the nonce and ciphertext
    let (nonce, ciphertext) = buffer.split_at(NONCE_SIZE);

    // Create a cipher instance
    let cipher = Aes256Gcm::new(GenericArray::from_slice(key));
    let nonce_array = GenericArray::from_slice(nonce);

    // Decrypt the ciphertext
    let plaintext = cipher.decrypt(nonce_array, ciphertext)?;

    // Write the plaintext to the output file
    let mut output_file = File::create(output_path)?;
    output_file.write_all(&plaintext)?;

    Ok(())
}

#[derive(Clone)]
struct HistoryRequest {
    pin_code: String,
    request_id: String,
}

impl HistoryRequest {
    #[allow(dead_code)]
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

struct HistoryReply {
    request_id: String,
    url: String,
    bundle_hash: Vec<u8>,
    signing_key: HistoryKeyType,
    encryption_key: HistoryKeyType,
}

impl HistoryReply {
    #[allow(dead_code)]
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
enum HistoryKeyType {
    Chacha20Poly1305([u8; ENC_KEY_SIZE]),
}

impl HistoryKeyType {
    #[allow(dead_code)]
    fn new_chacha20_poly1305_key() -> Self {
        let mut key = [0u8; 32]; // 256-bit key
        crypto_utils::rng().fill_bytes(&mut key[..]);
        HistoryKeyType::Chacha20Poly1305(key)
    }

    #[allow(dead_code)]
    fn len(&self) -> usize {
        match self {
            HistoryKeyType::Chacha20Poly1305(key) => key.len(),
        }
    }

    #[allow(dead_code)]
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

fn new_request_id() -> String {
    Alphanumeric.sample_string(&mut rand::thread_rng(), 24)
}

fn generate_nonce() -> [u8; NONCE_SIZE] {
    let mut nonce = [0u8; NONCE_SIZE];
    rand::thread_rng().fill(&mut nonce);
    nonce
}

fn new_pin() -> String {
    let mut rng = rand::thread_rng();
    let pin: u32 = rng.gen_range(0..10000);
    format!("{:04}", pin)
}

// Yes, this is a just a simple string comparison.
// If we need to add more complex logic, we can do so here.
// For example if we want to add a time limit or enforce a certain number of attempts.
fn verify_pin(expected: &str, actual: &str) -> bool {
    expected == actual
}

#[cfg(test)]
mod tests {

    use super::*;
    use xmtp_cryptography::utils::generate_local_wallet;

    use crate::assert_ok;
    use crate::builder::ClientBuilder;

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
    async fn test_send_message_history_request() {
        let wallet = generate_local_wallet();
        let client = ClientBuilder::new_test_client(&wallet).await;
        assert_ok!(client.allow_history_sync().await);

        // test that the request is sent, and that the pin code is returned
        let pin_code = client
            .send_message_history_request()
            .await
            .expect("history request");
        assert_eq!(pin_code.len(), 4);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_send_message_history_reply() {
        let wallet = generate_local_wallet();
        let client = ClientBuilder::new_test_client(&wallet).await;
        assert_ok!(client.allow_history_sync().await);

        let request_id = new_request_id();
        let url = "https://test.com/abc-123";
        let backup_hash = b"ABC123".into();
        let signing_key = HistoryKeyType::new_chacha20_poly1305_key();
        let encryption_key = HistoryKeyType::new_chacha20_poly1305_key();
        let reply = HistoryReply::new(&request_id, url, backup_hash, signing_key, encryption_key);
        let result = client.send_message_history_reply(reply.into()).await;
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
            .send_message_history_request()
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

        let amal_a_conn = amal_a.store().conn().unwrap();
        let amal_a_messages = amal_a_conn
            .get_group_messages(amal_a_sync_group.group_id, None, None, None, None, None)
            .unwrap();
        assert_eq!(amal_a_messages.len(), 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_provide_pin_challenge() {
        let wallet = generate_local_wallet();
        let amal_a = ClientBuilder::new_test_client(&wallet).await;
        let amal_b = ClientBuilder::new_test_client(&wallet).await;
        assert_ok!(amal_b.allow_history_sync().await);

        amal_a.sync_welcomes().await.expect("sync_welcomes");

        let pin_code = amal_b
            .send_message_history_request()
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
    async fn test_request_reply_roundtrip() {
        let wallet = generate_local_wallet();
        let amal_a = ClientBuilder::new_test_client(&wallet).await;
        let amal_b = ClientBuilder::new_test_client(&wallet).await;
        assert_ok!(amal_b.allow_history_sync().await);

        amal_a.sync_welcomes().await.expect("sync_welcomes");

        // amal_b sends a message history request to sync group messages
        let pin_code = amal_b
            .send_message_history_request()
            .await
            .expect("history request");

        let amal_a_sync_groups = amal_a.store().conn().unwrap().find_sync_groups().unwrap();
        assert_eq!(amal_a_sync_groups.len(), 1);
        // get the first sync group
        let amal_a_sync_group = amal_a.group(amal_a_sync_groups[0].id.clone()).unwrap();
        amal_a_sync_group.sync(&amal_a).await.expect("sync");
        let pin_challenge_result = amal_a.provide_pin(&pin_code);
        assert_ok!(pin_challenge_result);

        // amal_a builds and sends a message history reply back
        let history_reply = HistoryReply::new(
            "test",
            "https://test.com/abc-123",
            b"ABC123".into(),
            HistoryKeyType::new_chacha20_poly1305_key(),
            HistoryKeyType::new_chacha20_poly1305_key(),
        );
        amal_a
            .send_message_history_reply(history_reply.into())
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

    #[test]
    fn test_encrypt_decrypt_file() {
        let key = HistoryKeyType::new_chacha20_poly1305_key();
        let key_bytes = key.as_bytes();
        let input_content = b"Hello, this is a test message!";
        let input_path = "test_input.txt";
        let encrypted_path = "test_encrypted.dat";
        let decrypted_path = "test_decrypted.txt";

        // Write test input file
        std::fs::write(input_path, input_content).expect("Unable to write test input file");

        // Encrypt the file
        encrypt_messages_file(input_path, encrypted_path, &key_bytes).expect("Encryption failed");

        // Decrypt the file
        decrypt_messages_file(encrypted_path, decrypted_path, &key_bytes)
            .expect("Decryption failed");

        // Read the decrypted file content
        let decrypted_content =
            std::fs::read(decrypted_path).expect("Unable to read decrypted file");

        // Assert the decrypted content is the same as the original input content
        assert_eq!(decrypted_content, input_content);

        // Clean up test files
        std::fs::remove_file(input_path).expect("Unable to remove test input file");
        std::fs::remove_file(encrypted_path).expect("Unable to remove test encrypted file");
        std::fs::remove_file(decrypted_path).expect("Unable to remove test decrypted file");
    }

    #[test]
    fn test_new_pin() {
        let pin = new_pin();
        assert_eq!(pin.len(), 4);
    }

    #[test]
    fn test_new_key() {
        let sig_key = HistoryKeyType::new_chacha20_poly1305_key();
        let enc_key = HistoryKeyType::new_chacha20_poly1305_key();
        assert_eq!(sig_key.len(), 32);
        // ensure keys are different (seed isn't reused)
        assert_ne!(sig_key, enc_key);
    }

    #[test]
    fn test_generate_nonce() {
        let nonce = generate_nonce();
        assert_eq!(nonce.len(), NONCE_SIZE);
    }
}
