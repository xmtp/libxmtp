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
use super::{GroupError, MlsGroup};

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
    pub async fn send_consent_sync_request(&self) -> Result<(String, String), MessageHistoryError> {
        // find the sync group
        let conn = self.store().conn()?;
        let sync_group = self.get_sync_group()?;

        // sync the group
        sync_group.sync().await?;
    }
}
