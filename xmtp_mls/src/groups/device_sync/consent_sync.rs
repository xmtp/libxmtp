use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};

use aes_gcm::aead::generic_array::GenericArray;
use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm,
};
use serde::Deserialize;
use xmtp_id::scw_verifier::SmartContractSignatureVerifier;
use xmtp_proto::{
    xmtp::mls::message_contents::plaintext_envelope::v2::MessageType::{Reply, Request},
    xmtp::mls::message_contents::plaintext_envelope::{Content, V2},
    xmtp::mls::message_contents::PlaintextEnvelope,
};

use super::*;

use crate::storage::key_value_store::{KeyValueStore, StoreKey};
use crate::XmtpApi;
use crate::{groups::GroupMessageKind, Client};

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
            self.send_history_reply(reply.clone()).await?;
            return Ok(reply);
        }

        Err(DeviceSyncError::NoPendingRequest)
    }

    pub async fn send_consent_sync_reply(
        &self,
        contents: DeviceSyncReplyProto,
    ) -> Result<(), DeviceSyncError> {
        // find the sync group
        let conn = self.store().conn()?;
        let sync_group = self.get_sync_group()?;

        sync_group.sync().await?;

        Ok(())
    }
}
