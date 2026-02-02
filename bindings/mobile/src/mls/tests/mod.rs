// Shared test utilities and helpers for FFI MLS tests

use super::{
    FfiConsentCallback, FfiConversation, FfiMessage, FfiMessageCallback,
    FfiMessageDeletionCallback, FfiMessageEditCallback, FfiPreferenceCallback, FfiPreferenceUpdate,
    FfiSignatureRequest, FfiXmtpClient, create_client,
};
use crate::{
    FfiAction, FfiActionStyle, FfiActions, FfiAttachment, FfiConsent, FfiConsentEntityType,
    FfiConsentState, FfiContentType, FfiConversationCallback, FfiConversationMessageKind,
    FfiConversationType, FfiCreateDMOptions, FfiCreateGroupOptions, FfiDecodedMessage,
    FfiDecodedMessageBody, FfiDecodedMessageContent, FfiDirection, FfiGroupMembershipState,
    FfiGroupMessageKind, FfiGroupPermissionsOptions, FfiGroupQueryOrderBy, FfiIntent,
    FfiListConversationsOptions, FfiListMessagesOptions, FfiMessageDisappearingSettings,
    FfiMessageWithReactions, FfiMetadataField, FfiMultiRemoteAttachment, FfiPasskeySignature,
    FfiPermissionPolicy, FfiPermissionPolicySet, FfiPermissionUpdateType, FfiReactionAction,
    FfiReactionPayload, FfiReactionSchema, FfiReadReceipt, FfiRemoteAttachment, FfiReply,
    FfiSendMessageOpts, FfiSignatureKind, FfiSubscribeError, FfiTransactionReference, GenericError,
    apply_signature_request, connect_to_backend, decode_actions, decode_attachment,
    decode_delete_message, decode_group_updated, decode_intent, decode_leave_request,
    decode_multi_remote_attachment, decode_reaction, decode_read_receipt, decode_remote_attachment,
    decode_reply, decode_text, decode_transaction_reference, encode_actions, encode_attachment,
    encode_delete_message, encode_intent, encode_leave_request, encode_multi_remote_attachment,
    encode_reaction, encode_read_receipt, encode_remote_attachment, encode_reply, encode_text,
    encode_transaction_reference, get_inbox_id_for_identifier, get_newest_message_metadata,
    identity::FfiIdentifier,
    inbox_owner::FfiInboxOwner,
    inbox_state_from_inbox_ids, is_connected,
    message::{
        FfiDeleteMessage, FfiEncodedContent, FfiGroupUpdated, FfiInbox, FfiLeaveRequest,
        FfiMetadataFieldChange, FfiTransactionMetadata,
    },
    mls::{
        MessageBackendBuilder,
        inbox_owner::FfiWalletInboxOwner,
        test_utils::{LocalBuilder, LocalTester, connect_to_backend_test},
    },
    revoke_installations,
    worker::FfiSyncWorkerMode,
};
use alloy::signers::local::PrivateKeySigner;
use futures::future::join_all;
use log::{Instrument, info_span};
use parking_lot::Mutex;
use prost::Message;
use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
    time::Duration,
};
use tokio::{
    sync::{Notify, futures::OwnedNotified},
    time::error::Elapsed,
};
use xmtp_api::ApiClientWrapper;
use xmtp_common::tmp_path;
use xmtp_common::{time::now_ns, wait_for_ge};
use xmtp_common::{wait_for_eq, wait_for_ok};
use xmtp_configuration::MAX_INSTALLATIONS_PER_INBOX;
use xmtp_content_types::{
    ContentCodec, attachment::AttachmentCodec, bytes_to_encoded_content, encoded_content_to_bytes,
    group_updated::GroupUpdatedCodec, membership_change::GroupMembershipChangeCodec,
    reaction::ReactionCodec, read_receipt::ReadReceiptCodec,
    remote_attachment::RemoteAttachmentCodec, reply::ReplyCodec, text::TextCodec,
    transaction_reference::TransactionReferenceCodec,
};
use xmtp_cryptography::utils::generate_local_wallet;
use xmtp_db::EncryptionKey;
use xmtp_db::MlsProviderExt;
use xmtp_db::XmtpMlsStorageProvider;
use xmtp_db::prelude::*;
use xmtp_id::associations::{
    MemberIdentifier, test_utils::WalletTestExt, unverified::UnverifiedSignature,
};
use xmtp_mls::{
    InboxOwner,
    groups::{GroupError, device_sync::worker::SyncMetric},
    utils::{PasskeyUser, Tester, TesterBuilder},
};
use xmtp_proto::xmtp::mls::message_contents::{
    ContentTypeId, EncodedContent,
    content_types::{ReactionAction, ReactionSchema, ReactionV2},
};

// Test module declarations
mod archive;
mod client;
mod content_types;
mod dms;
mod group_management;
mod identity;
mod networking;
mod static_methods;
mod streaming;
mod test_self_removal;

// Shared test callback struct
pub(crate) struct RustStreamCallback {
    num_messages: AtomicU32,
    messages: Mutex<Vec<FfiMessage>>,
    conversations: Mutex<Vec<Arc<FfiConversation>>>,
    consent_updates: Mutex<Vec<FfiConsent>>,
    preference_updates: Mutex<Vec<FfiPreferenceUpdate>>,
    notify: Arc<Notify>,
    inbox_id: Option<String>,
    installation_id: Option<String>,
}

impl Default for RustStreamCallback {
    fn default() -> Self {
        RustStreamCallback {
            num_messages: Default::default(),
            messages: Default::default(),
            conversations: Default::default(),
            consent_updates: Default::default(),
            preference_updates: Default::default(),
            notify: Arc::new(Notify::new()),
            inbox_id: None,
            installation_id: None,
        }
    }
}

impl RustStreamCallback {
    pub fn message_count(&self) -> u32 {
        self.num_messages.load(Ordering::SeqCst)
    }

    pub fn consent_updates_count(&self) -> usize {
        self.consent_updates.lock().len()
    }

    pub fn enable_notifications(&self) -> OwnedNotified {
        self.notify.clone().notified_owned()
    }

    pub async fn wait_for_delivery(&self, timeout_secs: Option<u64>) -> Result<(), Elapsed> {
        tokio::time::timeout(
            std::time::Duration::from_secs(timeout_secs.unwrap_or(60)),
            async { self.notify.notified().await },
        )
        .await?;
        Ok(())
    }

    pub fn from_client(client: &FfiXmtpClient) -> Self {
        RustStreamCallback {
            inbox_id: Some(client.inner_client.inbox_id().to_string()),
            installation_id: Some(hex::encode(client.inner_client.installation_public_key())),
            ..Default::default()
        }
    }
}

impl FfiMessageCallback for RustStreamCallback {
    fn on_message(&self, message: FfiMessage) {
        let mut messages = self.messages.lock();
        log::info!(
            inbox_id = self.inbox_id,
            installation_id = self.installation_id,
            "ON MESSAGE Received\n-------- \n{}\n----------",
            String::from_utf8_lossy(&message.content)
        );
        messages.push(message);
        let _ = self.num_messages.fetch_add(1, Ordering::SeqCst);
        self.notify.notify_one();
    }

    fn on_error(&self, error: FfiSubscribeError) {
        log::error!("{}", error)
    }

    fn on_close(&self) {
        log::error!("closed");
    }
}

impl FfiConversationCallback for RustStreamCallback {
    fn on_conversation(&self, group: Arc<FfiConversation>) {
        log::debug!(
            inbox_id = self.inbox_id,
            installation_id = self.installation_id,
            "received conversation"
        );
        let _ = self.num_messages.fetch_add(1, Ordering::SeqCst);
        let mut convos = self.conversations.lock();
        convos.push(group);
        self.notify.notify_one();
    }

    fn on_error(&self, error: FfiSubscribeError) {
        log::error!("{}", error)
    }

    fn on_close(&self) {
        log::error!("closed");
    }
}

impl FfiConsentCallback for RustStreamCallback {
    fn on_consent_update(&self, mut consent: Vec<FfiConsent>) {
        log::debug!(
            inbox_id = self.inbox_id,
            installation_id = self.installation_id,
            "received consent update"
        );
        let mut consent_updates = self.consent_updates.lock();
        consent_updates.append(&mut consent);
        self.notify.notify_one();
    }

    fn on_error(&self, error: FfiSubscribeError) {
        log::error!("{}", error)
    }

    fn on_close(&self) {
        log::error!("closed");
    }
}

impl FfiPreferenceCallback for RustStreamCallback {
    fn on_preference_update(&self, mut preference: Vec<FfiPreferenceUpdate>) {
        log::debug!(
            inbox_id = self.inbox_id,
            installation_id = self.installation_id,
            "\n\n=======================received consent update==============\n\n"
        );
        self.preference_updates.lock().append(&mut preference);
        self.notify.notify_one();
    }

    fn on_error(&self, error: FfiSubscribeError) {
        log::error!("{}", error)
    }

    fn on_close(&self) {
        log::error!("closed");
    }
}

// Callback for message deletion streaming tests
pub(crate) struct RustMessageDeletionCallback {
    deleted_messages: Mutex<Vec<Arc<FfiDecodedMessage>>>,
    notify: Arc<Notify>,
}

impl Default for RustMessageDeletionCallback {
    fn default() -> Self {
        RustMessageDeletionCallback {
            deleted_messages: Default::default(),
            notify: Arc::new(Notify::new()),
        }
    }
}

impl RustMessageDeletionCallback {
    pub fn deleted_message_count(&self) -> usize {
        self.deleted_messages.lock().len()
    }

    pub fn deleted_messages(&self) -> Vec<Arc<FfiDecodedMessage>> {
        self.deleted_messages.lock().clone()
    }

    pub async fn wait_for_delivery(&self, timeout_secs: Option<u64>) -> Result<(), Elapsed> {
        tokio::time::timeout(
            std::time::Duration::from_secs(timeout_secs.unwrap_or(60)),
            async { self.notify.notified().await },
        )
        .await?;
        Ok(())
    }
}

impl FfiMessageDeletionCallback for RustMessageDeletionCallback {
    fn on_message_deleted(&self, message: Arc<FfiDecodedMessage>) {
        log::info!(
            "ON MESSAGE DELETED Received\n-------- \nid: {:?}\n----------",
            message.id()
        );
        self.deleted_messages.lock().push(message);
        self.notify.notify_one();
    }
}

#[derive(Default)]
#[allow(dead_code)]
pub(crate) struct RustMessageEditCallback {
    edited_messages: Mutex<Vec<Arc<FfiDecodedMessage>>>,
    notify: Arc<Notify>,
}

#[allow(dead_code)]
impl RustMessageEditCallback {
    pub fn edited_message_count(&self) -> usize {
        self.edited_messages.lock().len()
    }

    pub fn edited_messages(&self) -> Vec<Arc<FfiDecodedMessage>> {
        self.edited_messages.lock().clone()
    }

    pub async fn wait_for_delivery(&self, timeout_secs: Option<u64>) -> Result<(), Elapsed> {
        tokio::time::timeout(
            Duration::from_secs(timeout_secs.unwrap_or(120)),
            self.notify.notified(),
        )
        .await
    }
}

impl FfiMessageEditCallback for RustMessageEditCallback {
    fn on_message_edited(&self, message: Arc<FfiDecodedMessage>) {
        log::info!(
            "ON MESSAGE EDITED Received\n-------- \nid: {:?}\n----------",
            message.id()
        );
        self.edited_messages.lock().push(message);
        self.notify.notify_one();
    }

    fn on_error(&self, error: FfiSubscribeError) {
        log::error!("ON MESSAGE EDIT ERROR: {:?}", error);
    }
}

// Helper functions
pub(crate) fn static_enc_key() -> EncryptionKey {
    [2u8; 32]
}

pub(crate) async fn register_client_with_wallet(
    wallet: &FfiWalletInboxOwner,
    client: &FfiXmtpClient,
) {
    register_client_with_wallet_no_panic(wallet, client)
        .await
        .unwrap()
}

pub(crate) async fn register_client_with_wallet_no_panic(
    wallet: &FfiWalletInboxOwner,
    client: &FfiXmtpClient,
) -> Result<(), GenericError> {
    let signature_request = client.signature_request().unwrap();

    signature_request
        .add_ecdsa_signature(
            wallet
                .sign(signature_request.signature_text().await.unwrap())
                .unwrap(),
        )
        .await?;

    client.register_identity(signature_request).await?;

    Ok(())
}

/// Create a new test client with a given wallet.
pub(crate) async fn new_test_client_with_wallet(wallet: PrivateKeySigner) -> Arc<FfiXmtpClient> {
    new_test_client_with_wallet_and_history_sync_url(
        wallet,
        None,
        Some(FfiSyncWorkerMode::Disabled),
    )
    .await
}

pub(crate) async fn new_test_client_with_wallet_and_history_sync_url(
    wallet: PrivateKeySigner,
    history_sync_url: Option<String>,
    sync_worker_mode: Option<FfiSyncWorkerMode>,
) -> Arc<FfiXmtpClient> {
    let ffi_inbox_owner = FfiWalletInboxOwner::with_wallet(wallet);
    let ident = ffi_inbox_owner.identifier();

    let nonce = 1;
    let inbox_id = ident.inbox_id(nonce).unwrap();

    let client = create_client(
        connect_to_backend_test().await,
        connect_to_backend_test().await,
        Some(tmp_path()),
        Some([0u8; 32].to_vec()),
        &inbox_id,
        ident,
        nonce,
        None,
        history_sync_url,
        sync_worker_mode,
        None,
        None,
    )
    .await
    .unwrap();

    let conn = client.inner_client.context.db();
    conn.register_triggers();

    register_client_with_wallet(&ffi_inbox_owner, &client).await;

    client
}

pub(crate) async fn new_test_client_no_panic(
    wallet: PrivateKeySigner,
    sync_server_url: Option<String>,
) -> Result<Arc<FfiXmtpClient>, GenericError> {
    let ffi_inbox_owner = FfiWalletInboxOwner::with_wallet(wallet);
    let ident = ffi_inbox_owner.identifier();
    let nonce = 1;
    let inbox_id = ident.inbox_id(nonce).unwrap();

    let client = create_client(
        connect_to_backend_test().await,
        connect_to_backend_test().await,
        Some(tmp_path()),
        Some(xmtp_db::EncryptedMessageStore::<()>::generate_enc_key().into()),
        &inbox_id,
        ident,
        nonce,
        None,
        sync_server_url,
        None,
        None,
        None,
    )
    .await?;

    let conn = client.inner_client.context.db();
    conn.register_triggers();

    register_client_with_wallet_no_panic(&ffi_inbox_owner, &client).await?;

    Ok(client)
}

pub(crate) async fn new_test_client() -> Arc<FfiXmtpClient> {
    let wallet = PrivateKeySigner::random();
    new_test_client_with_wallet(wallet).await
}

// Helper trait for signing with wallet
pub(crate) trait SignWithWallet {
    async fn add_wallet_signature(&self, wallet: &PrivateKeySigner);
}

impl SignWithWallet for FfiSignatureRequest {
    async fn add_wallet_signature(&self, wallet: &PrivateKeySigner) {
        let signature_text = self.inner.lock().await.signature_text();

        self.inner
            .lock()
            .await
            .add_signature(wallet.sign(&signature_text).unwrap(), &self.scw_verifier)
            .await
            .unwrap();
    }
}

// Extension methods for FfiConversation
impl FfiConversation {
    pub(crate) async fn update_installations(&self) -> Result<(), GroupError> {
        self.inner.update_installations().await?;
        Ok(())
    }
}
