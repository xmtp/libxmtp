//! Welcome pointer fixtures, and single attempts at a pending Welcome, for
//! tests on either side of the engine.

use crate::context::CoreContext;
use crate::groups::context::XmtpSharedContext;
use crate::groups::mls_ext::WelcomePointersExtension;
use crate::groups::test_support::EnginePeer;
use crate::groups::welcome_sync::{WelcomeHeadOutcome, WelcomeService};
use crate::groups::{GroupError, MlsGroup};
use prost::Message;
use xmtp_configuration::WELCOME_HPKE_LABEL;
use xmtp_id::key_package::WrapperAlgorithm;
use xmtp_mls_group_keys::mls_ext::payload_encryption::{wrap_payload_hpke, wrap_payload_symmetric};
use xmtp_proto::types::{Cursor, WelcomeMessage};
use xmtp_proto::xmtp::mls::message_contents::WelcomeMetadata;
use xmtp_proto::xmtp::mls::message_contents::welcome_pointer::WelcomeV1Pointer;
use xmtp_proto::xmtp::mls::message_contents::{
    WelcomePointeeEncryptionAeadType, WelcomePointer as WelcomePointerProto,
    WelcomePointerWrapperAlgorithm, WelcomeWrapperAlgorithm,
};

/// What one attempt at a pending Welcome left, as the Welcome service reports
/// it.
pub enum PendingWelcomeOutcome<C> {
    /// No pending row remained at the cursor.
    Idle { cursor: Cursor },
    /// The row stays pending with a retry or blocked diagnostic.
    Waiting {
        cursor: Cursor,
        code: String,
        /// Blocked rows wait for the coordinator's startup and periodic scans.
        blocked: bool,
    },
    /// A dependency must be resolved outside the state writer first.
    Need { cursor: Cursor },
    /// The join or safe rejection and the pending row's completion committed.
    Progress {
        cursor: Cursor,
        result: Result<Option<MlsGroup<C>>, GroupError>,
    },
}

impl<C> From<WelcomeHeadOutcome<C>> for PendingWelcomeOutcome<C> {
    fn from(outcome: WelcomeHeadOutcome<C>) -> Self {
        match outcome {
            WelcomeHeadOutcome::Idle { cursor } => Self::Idle { cursor },
            WelcomeHeadOutcome::Waiting {
                cursor,
                code,
                blocked,
            } => Self::Waiting {
                cursor,
                code,
                blocked,
            },
            WelcomeHeadOutcome::Need { cursor, .. } => Self::Need { cursor },
            WelcomeHeadOutcome::Progress { cursor, result } => Self::Progress { cursor, result },
        }
    }
}

/// Resolve the dependencies of the pending Welcome at `cursor` once, as the
/// incoming coordinator does.
pub async fn resolve_pending_welcome<C: XmtpSharedContext>(
    service: &WelcomeService<C>,
    cursor: Cursor,
) -> Result<PendingWelcomeOutcome<C>, GroupError> {
    service
        .resolve_pending_welcome(cursor)
        .await
        .map(Into::into)
}

/// Reread the pending Welcome at `cursor` once, as the coordinator does after
/// a dependency completes without marking an identity reference absent.
pub fn retry_pending_welcome<C: XmtpSharedContext>(
    service: &WelcomeService<C>,
    cursor: Cursor,
) -> Result<PendingWelcomeOutcome<C>, GroupError> {
    service.retry_pending_welcome(cursor, None).map(Into::into)
}

/// Publish a commit adding `inbox_id` and return its plaintext Welcome and join
/// anchor without sending the Welcome.
pub async fn publish_commit_without_welcome<C: XmtpSharedContext>(
    group: &MlsGroup<C>,
    inbox_id: &str,
) -> (Vec<u8>, u64) {
    let intent = group
        .get_membership_update_intent(&[inbox_id], &[])
        .await
        .unwrap();
    let signer = &group.context.identity().installation_keys;
    let context = &group.context;
    let old_membership = group
        .with_group_snapshot(|openmls_group| {
            Ok(xmtp_mls_validation::commit::extract_group_membership(
                openmls_group.extensions(),
            )?)
        })
        .unwrap();
    let new_membership = intent.apply_to_group_membership(&old_membership);
    let changes = crate::groups::mls_sync::calculate_membership_changes_with_keypackages(
        context,
        &group.group_id,
        &new_membership,
        &old_membership,
        None,
    )
    .await
    .unwrap();
    let (action, payloads) = crate::state_tx::state_write(context.mls_storage(), |tx| {
        tx.with_group(group.group_id, |openmls_group, storage| {
            let publish_intent_data =
                crate::groups::mls_sync::update_group_membership::apply_update_group_membership_intent(storage, openmls_group, intent, changes, &[], signer)?
                    .unwrap();
            let post_commit_action = crate::groups::intents::PostCommitAction::from_bytes(
                publish_intent_data.post_commit_data().unwrap().as_slice(),
            )?;
            let crate::groups::intents::PostCommitAction::SendWelcomes(action) = post_commit_action;
            let staged_commit = publish_intent_data.staged_commit().unwrap();
            openmls_group.merge_staged_commit(
                &xmtp_db::XmtpOpenMlsProviderRef::new(storage),
                crate::groups::mls_sync::decode_staged_commit(staged_commit.as_slice())?,
            ).unwrap();

            Ok::<_, crate::groups::GroupError>(xmtp_db::TransactionOutcome::Continue((action, publish_intent_data.payloads_to_publish)))
        })
    })
    .unwrap()
    .into_continued();
    let commit_units = group
        .prepare_group_messages(
            payloads
                .iter()
                .map(|payload| (payload.as_slice(), false))
                .collect(),
        )
        .unwrap();
    let commit_receipt = context
        .api()
        .send_group_messages(commit_units)
        .await
        .unwrap()
        .pop()
        .unwrap();
    (
        action.welcome_message,
        commit_receipt.cursor.unwrap().sequence_id,
    )
}

/// A well-formed welcome pointer to a fresh destination.
pub fn random_pointer() -> WelcomeV1Pointer {
    WelcomeV1Pointer {
        destination: xmtp_common::rand_vec::<32>(),
        aead_type: WelcomePointeeEncryptionAeadType::Chacha20Poly1305.into(),
        encryption_key: xmtp_common::rand_vec::<32>(),
        data_nonce: xmtp_common::rand_vec::<12>(),
        welcome_metadata_nonce: xmtp_common::rand_vec::<12>(),
    }
}

/// The pointee that `pointer` opens: `welcome` and a Welcome metadata naming
/// `message_cursor`, both sealed under the pointer's key.
pub fn sealed_pointee(
    pointer: &WelcomeV1Pointer,
    welcome: &[u8],
    message_cursor: u64,
) -> xmtp_proto::backend_v1::WelcomeMessage {
    use xmtp_proto::backend_v1::welcome_message::{V1, Version};
    let seal = |plaintext: &[u8], nonce: &[u8]| {
        wrap_payload_symmetric(
            plaintext,
            WelcomePointersExtension::preferred_type(),
            &pointer.encryption_key,
            nonce,
        )
        .unwrap()
    };
    xmtp_proto::backend_v1::WelcomeMessage {
        version: Some(Version::V1(V1 {
            installation_key: pointer.destination.clone(),
            data: seal(welcome, &pointer.data_nonce),
            hpke_public_key: vec![],
            wrapper_algorithm: WelcomeWrapperAlgorithm::SymmetricKey.into(),
            welcome_metadata: seal(
                &WelcomeMetadata { message_cursor }.encode_to_vec(),
                &pointer.welcome_metadata_nonce,
            ),
        })),
    }
}

/// Wrap `pointer` for a new post-quantum key package of `recipient`, publish
/// it from `sender`, and store it as the recipient's pending Welcome.
pub async fn publish_pointer<P: EnginePeer>(
    sender: &P,
    recipient: &P,
    pointer: &WelcomeV1Pointer,
) -> WelcomeMessage {
    use xmtp_proto::backend_v1::welcome_message::{Version, WelcomePointer};
    use xmtp_proto::xmtp::mls::message_contents::welcome_pointer::Version as PointerVersion;

    let package = crate::identity_lifecycle::new_key_package(
        recipient.context().identity(),
        &recipient.context().mls_provider(),
        true,
    )
    .unwrap();
    let public_key = package.pq_pub_key.as_deref().unwrap();
    let plaintext = WelcomePointerProto {
        version: Some(PointerVersion::WelcomeV1Pointer(pointer.clone())),
    }
    .encode_to_vec();
    let encrypted = wrap_payload_hpke(
        &plaintext,
        &[],
        public_key,
        WrapperAlgorithm::XWingMLKEM768Draft6,
        WELCOME_HPKE_LABEL,
    )
    .unwrap()
    .0;
    sender
        .context()
        .api()
        .send_welcome_messages(&[xmtp_proto::backend_v1::WelcomeMessage {
            version: Some(Version::WelcomePointer(WelcomePointer {
                installation_key: recipient.context().installation_id().to_vec(),
                welcome_pointer: encrypted,
                hpke_public_key: public_key.to_vec(),
                wrapper_algorithm: WelcomePointerWrapperAlgorithm::XwingMlkem768Draft6.into(),
            })),
        }])
        .await
        .unwrap();
    let welcome = sender
        .context()
        .api()
        .query_welcome_messages(recipient.context().installation_id())
        .await
        .unwrap()
        .pop()
        .expect("published pointer");
    crate::groups::welcome_sync::pending_welcome_for_test(recipient.context(), &welcome)
        .await
        .unwrap();
    welcome
}
