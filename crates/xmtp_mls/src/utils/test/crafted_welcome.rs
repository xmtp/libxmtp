//! Welcomes for MLS groups that a test builds by hand, so a joiner can be
//! offered a group context that no client would create: one without a root,
//! with a root for another id, or with a 16-byte group id.
//!
//! The Welcome goes straight to the joiner's welcome topic. No group message
//! is published, so the Welcome does not depend on what the backend admits
//! on the group's topic. Its metadata names cursor 1 as the join anchor. The
//! creator signs with its own installation key but keeps no group state.

use std::collections::BTreeMap;

use openmls::group::{GroupId as OpenMlsGroupId, MlsGroup as OpenMlsGroup};
use openmls::prelude::CredentialWithKey;
use prost::Message;
use tls_codec::Serialize;
use xmtp_db::prelude::*;
use xmtp_mls_common::{
    app_data::{
        component_id::ComponentId,
        creation::{InitialGroupKind, initial_dictionary},
        migration::encode_group_membership_dict,
    },
    group::GroupMetadataOptions,
    group_keys::GroupRoot,
    inbox_id::InboxId,
    mls_ext::payload_encryption::wrap_payload_hpke,
};
use xmtp_proto::{
    backend_v1::{
        WelcomeMessage as WelcomeMessageInput,
        welcome_message::{V1 as WelcomeMessageInputV1, Version as WelcomeMessageInputVersion},
    },
    types::ConversationType,
    xmtp::mls::message_contents::{
        GroupMembershipEntry, WelcomeMetadata,
        group_membership_entry::{V1 as MembershipV1, Version as MembershipVersion},
    },
};

use crate::context::XmtpSharedContext;
use crate::groups::{build_group_config, group_permissions::PolicySet, intents::Installation};
use crate::utils::FullXmtpClient;

/// `creator` builds a group named `group_id` whose dictionary holds `root`
/// (or no root), adds `joiner`'s current key package, and publishes only the
/// Welcome. Everything else matches a group the creator could have made, so
/// the root is the only reason a joiner may refuse it.
pub(crate) async fn send_crafted_welcome(
    creator: &FullXmtpClient,
    joiner: &FullXmtpClient,
    group_id: &[u8],
    root: Option<&GroupRoot>,
) {
    send_crafted_welcome_of_type(creator, joiner, group_id, root, ConversationType::Group).await
}

/// [`send_crafted_welcome`] for a group of `conversation_type`, such as a
/// one-shot group with no one-shot message.
// A test drives an OpenMLS convenience commit; its GroupInfo never leaves the test.
#[allow(clippy::disallowed_methods)]
pub(crate) async fn send_crafted_welcome_of_type(
    creator: &FullXmtpClient,
    joiner: &FullXmtpClient,
    group_id: &[u8],
    root: Option<&GroupRoot>,
    conversation_type: ConversationType,
) {
    let mut dictionary = initial_dictionary(
        InitialGroupKind::Group {
            conversation_type,
            oneshot_message: None,
        },
        &PolicySet::default().to_proto().unwrap(),
        &GroupMetadataOptions::default(),
        creator.inbox_id(),
        root,
        &[],
    )
    .unwrap();
    // Name each member's identity state, as an add commit would.
    let membership = [creator, joiner]
        .into_iter()
        .map(|client| {
            let sequence_id = client
                .context
                .db()
                .get_latest_sequence_id_for_inbox(client.inbox_id())
                .unwrap();
            (
                InboxId::from_hex(client.inbox_id()).unwrap(),
                GroupMembershipEntry {
                    version: Some(MembershipVersion::V1(MembershipV1 {
                        sequence_id: sequence_id as u64,
                        failed_installations: vec![],
                    })),
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    dictionary.insert(
        ComponentId::GROUP_MEMBERSHIP.as_u16(),
        encode_group_membership_dict(&membership).unwrap(),
    );
    let config = build_group_config(dictionary).unwrap();

    let installation_id = joiner.context.installation_id().to_vec();
    let key_package = creator
        .get_key_packages_for_installation_ids(vec![installation_id.clone()])
        .await
        .unwrap()
        .remove(&installation_id)
        .expect("the joiner published a key package")
        .unwrap();
    let identity = creator.context.identity();
    // The crafted group lives in a throwaway store, so the creator's own
    // groups stay as they are and one id can be crafted more than once.
    let provider = openmls_rust_crypto::OpenMlsRustCrypto::default();
    let mut group = OpenMlsGroup::new_with_group_id(
        &provider,
        &identity.installation_keys,
        &config,
        OpenMlsGroupId::from_slice(group_id),
        CredentialWithKey {
            credential: identity.credential(),
            signature_key: identity.installation_keys.public_slice().into(),
        },
    )
    .unwrap();
    let (_commit, welcome, _group_info) = group
        .add_members(
            &provider,
            &identity.installation_keys,
            std::slice::from_ref(&key_package.inner),
        )
        .unwrap();

    let installation = Installation::from_verified_key_package(&key_package).unwrap();
    let (data, welcome_metadata) = wrap_payload_hpke(
        &welcome.tls_serialize_detached().unwrap(),
        &WelcomeMetadata { message_cursor: 1 }.encode_to_vec(),
        &installation.hpke_public_key,
        installation.welcome_wrapper_algorithm,
        xmtp_configuration::WELCOME_HPKE_LABEL,
    )
    .unwrap();
    creator
        .context
        .api()
        .send_welcome_messages(&[WelcomeMessageInput {
            version: Some(WelcomeMessageInputVersion::V1(WelcomeMessageInputV1 {
                installation_key: installation.installation_key,
                data,
                hpke_public_key: installation.hpke_public_key,
                wrapper_algorithm: installation.welcome_wrapper_algorithm.into(),
                welcome_metadata,
            })),
        }])
        .await
        .unwrap();
}
