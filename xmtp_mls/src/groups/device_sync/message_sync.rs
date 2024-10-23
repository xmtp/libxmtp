use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};

use aes_gcm::aead::generic_array::GenericArray;
use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm,
};
use serde::Deserialize;
use tracing::warn;
use xmtp_id::scw_verifier::SmartContractSignatureVerifier;
use xmtp_proto::{
    xmtp::mls::message_contents::plaintext_envelope::v2::MessageType::{Reply, Request},
    xmtp::mls::message_contents::plaintext_envelope::{Content, V2},
    xmtp::mls::message_contents::{DeviceSyncRequest as DeviceSyncRequestProto, PlaintextEnvelope},
};

use super::*;

use crate::storage::key_value_store::{KVStore, Key};
use crate::storage::DbConnection;
use crate::XmtpApi;
use crate::{
    groups::{GroupMessageKind, StoredGroupMessage},
    storage::group::StoredGroup,
    Client, Store,
};

impl<ApiClient, V> Client<ApiClient, V>
where
    ApiClient: XmtpApi + Clone,
    V: SmartContractSignatureVerifier + Clone,
{
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
        sync_group.sync().await?;

        Ok(())
    }

    // returns (request_id, pin_code)
    pub async fn send_history_request(&self) -> Result<(String, String), DeviceSyncError> {
        let request = DeviceSyncRequest::new(DeviceSyncKind::MessageHistory);
        self.send_sync_request(request).await
    }

    pub async fn reply_to_history_request(&self) -> Result<DeviceSyncReplyProto, DeviceSyncError> {
        let Some((_msg, request)) = self
            .pending_sync_request(DeviceSyncKind::MessageHistory)
            .await?
        else {
            return Err(DeviceSyncError::NoPendingRequest);
        };

        let groups = self.syncable_groups()?;
        let messages = self.syncable_messages()?;

        let reply = self
            .send_syncables(&request.request_id, &[groups, messages])
            .await?;

        Ok(reply)
    }

    pub async fn process_message_history_reply(
        &self,
        conn: &DbConnection,
    ) -> Result<(), DeviceSyncError> {
        // load the request_id
        let request_id: Option<String> = KVStore::get(conn, &Key::MessageHistorySyncRequestId)
            .map_err(DeviceSyncError::Storage)?;
        let Some(request_id) = request_id else {
            return Err(DeviceSyncError::NoReplyToProcess);
        };

        // process the reply
        self.process_sync_reply(&request_id).await
    }

    pub(crate) fn verify_pin(
        &self,
        request_id: &str,
        pin_code: &str,
    ) -> Result<(), DeviceSyncError> {
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
                serde_json::from_slice::<DeviceSyncContent>(&msg.decrypted_message_bytes);

            match message_history_content {
                Ok(DeviceSyncContent::Request(request)) => {
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
            return Err(DeviceSyncError::PinNotFound);
        }

        Ok(())
    }

    fn syncable_groups(&self) -> Result<Vec<Syncable>, DeviceSyncError> {
        let conn = self.store().conn()?;
        let groups = conn
            .find_groups(None, None, None, None, Some(ConversationType::Group))?
            .into_iter()
            .map(Syncable::Group)
            .collect();
        Ok(groups)
    }

    fn syncable_messages(&self) -> Result<Vec<Syncable>, DeviceSyncError> {
        let conn = self.store().conn()?;
        let groups = conn.find_groups(None, None, None, None, Some(ConversationType::Group))?;

        let mut all_messages = vec![];
        for StoredGroup { id, .. } in groups.into_iter() {
            let messages = conn.get_group_messages(id, None, None, None, None, None)?;
            for msg in messages {
                all_messages.push(Syncable::GroupMessage(msg));
            }
        }

        Ok(all_messages)
    }
}

#[cfg(all(not(target_arch = "wasm32"), test))]
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

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
        let encryption_key = DeviceSyncKeyType::new_chacha20_poly1305_key();
        let reply = DeviceSyncReply::new(&request_id, url, encryption_key);
        let result = client.send_sync_reply(reply.into()).await;

        // the reply should fail because there's no pending request to reply to
        assert!(result.is_err());

        let (request_id, _) = client
            .send_history_request()
            .await
            .expect("history request");

        let request_id2 = new_request_id();
        let url = "https://test.com/abc-123";
        let encryption_key = DeviceSyncKeyType::new_chacha20_poly1305_key();
        let reply = DeviceSyncReply::new(&request_id2, url, encryption_key);
        let result = client.send_sync_reply(reply.into()).await;

        // the reply should fail because there's a mismatched request ID
        assert!(result.is_err());

        let url = "https://test.com/abc-123";
        let encryption_key = DeviceSyncKeyType::new_chacha20_poly1305_key();
        let reply = DeviceSyncReply::new(&request_id, url, encryption_key);
        let result = client.send_sync_reply(reply.into()).await;

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
        amal_a_sync_group.sync().await.expect("sync");

        // find the sync group (it should be the same as amal_a's sync group)
        let amal_b_sync_groups = amal_b.store().conn().unwrap().find_sync_groups().unwrap();
        assert_eq!(amal_b_sync_groups.len(), 1);
        // get the first sync group
        let amal_b_sync_group = amal_b.group(amal_b_sync_groups[0].id.clone()).unwrap();
        amal_b_sync_group.sync().await.expect("sync");

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
        amal_a_sync_group.sync().await.expect("sync");
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

        let groups = amal_a.syncable_groups().unwrap();

        let input_file = NamedTempFile::new().unwrap();
        let input_path = input_file.path();
        write_to_file(input_path, groups).unwrap();

        let output_file = NamedTempFile::new().unwrap();
        let output_path = output_file.path();
        let encryption_key = DeviceSyncKeyType::new_chacha20_poly1305_key();
        encrypt_bytes(input_path, output_path, encryption_key.as_bytes()).unwrap();

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
        amal_a_sync_group.sync().await.expect("sync");

        // amal_a builds and sends a message history reply back
        let history_reply =
            DeviceSyncReply::new(&new_request_id(), &history_sync_url, encryption_key);
        amal_a
            .send_sync_reply(history_reply.into())
            .await
            .expect("send reply");

        amal_a_sync_group.sync().await.expect("sync");
        // amal_b should have received the reply
        let amal_b_sync_groups = amal_b.store().conn().unwrap().find_sync_groups().unwrap();
        assert_eq!(amal_b_sync_groups.len(), 1);

        let amal_b_sync_group = amal_b.group(amal_b_sync_groups[0].id.clone()).unwrap();
        amal_b_sync_group.sync().await.expect("sync");

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

        let result = amal_a.syncable_groups().unwrap();
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

        group_a.send_message(b"hi").await.expect("send");
        group_a.send_message(b"hi x2").await.expect("send");
        group_b.send_message(b"hi").await.expect("send");
        group_b.send_message(b"hi x2").await.expect("send");

        let messages_result = amal_a.syncable_messages().unwrap();
        assert_eq!(messages_result.len(), 4);
    }

    #[test]
    fn test_encrypt_decrypt_file() {
        let key = DeviceSyncKeyType::new_chacha20_poly1305_key();
        let converted_key: DeviceSyncKeyTypeProto = key.into();
        let key_bytes = key.as_bytes();
        let input_content = b"'{\"test\": \"data\"}\n{\"test\": \"data2\"}\n'";
        let input_file = NamedTempFile::new().expect("Unable to create temp file");
        let encrypted_file = NamedTempFile::new().expect("Unable to create temp file");
        let decrypted_file = NamedTempFile::new().expect("Unable to create temp file");

        // Write test input file
        std::fs::write(input_file.path(), input_content).expect("Unable to write test input file");

        // Encrypt the file
        encrypt_bytes(input_file.path(), encrypted_file.path(), key_bytes)
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
            .send_sync_reply(DeviceSyncReplyProto {
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

        group_a.send_message(b"hi").await.expect("send message");

        let (bundle_path, enc_key) = amal_a
            .write_history_bundle()
            .await
            .expect("Unable to write history bundle");

        let output_file = NamedTempFile::new().expect("Unable to create temp file");
        let converted_key: DeviceSyncKeyTypeProto = enc_key.into();
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
        let created_at_ns = amal_sync_groups[0].created_at_ns;

        let external_client_group = MlsGroup::new(
            external_client.clone(),
            sync_group_id.clone(),
            created_at_ns,
        );
        let result = external_client_group
            .add_members(vec![external_wallet.get_address()])
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
        let sig_key = DeviceSyncKeyType::new_chacha20_poly1305_key();
        let enc_key = DeviceSyncKeyType::new_chacha20_poly1305_key();
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
