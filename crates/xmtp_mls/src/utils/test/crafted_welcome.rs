//! Welcomes for MLS groups that a test builds by hand, so a joiner can be
//! offered a group context that no client would create: one without a root,
//! with a root for another id, or with a 16-byte group id. With the root of a
//! group it was a member of, the creator can also build a second MLS group
//! under that group's id, as any former member can.
//!
//! The Welcome goes straight to the joiner's welcome topic. No group message
//! is published, so the Welcome does not depend on what the backend admits
//! on the group's topic. Unless a helper says otherwise, its metadata names
//! cursor 1 as the join anchor. The creator signs with its own installation
//! key but keeps no group state.

use std::collections::BTreeMap;

use openmls::group::{GroupId as OpenMlsGroupId, MlsGroup as OpenMlsGroup};
use openmls::prelude::CredentialWithKey;
use openmls::treesync::LeafNodeParameters;
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

/// What a crafted Welcome claims besides its group id and root. The default
/// is a group the creator could have made, at epoch 1 with join anchor 1.
pub(crate) struct CraftedWelcome<'a> {
    pub(crate) conversation_type: ConversationType,
    /// The `message_cursor` the Welcome names as its join anchor.
    pub(crate) anchor: u64,
    /// Commits the creator merges before the add; the Welcome's epoch is one more.
    pub(crate) prior_epoch: u64,
    /// The `CREATOR_INBOX_ID` claim. `None` names the creator's own inbox.
    pub(crate) claimed_creator: Option<&'a str>,
    /// The DM member other than the creator. `None` names the joiner. A peer
    /// other than the joiner gives a DM pair that excludes the joiner.
    pub(crate) dm_peer: Option<&'a str>,
}

impl Default for CraftedWelcome<'_> {
    fn default() -> Self {
        Self {
            conversation_type: ConversationType::Group,
            anchor: 1,
            prior_epoch: 0,
            claimed_creator: None,
            dm_peer: None,
        }
    }
}

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
pub(crate) async fn send_crafted_welcome_of_type(
    creator: &FullXmtpClient,
    joiner: &FullXmtpClient,
    group_id: &[u8],
    root: Option<&GroupRoot>,
    conversation_type: ConversationType,
) {
    let options = CraftedWelcome {
        conversation_type,
        ..Default::default()
    };
    send_crafted_welcome_with(creator, joiner, group_id, root, options).await
}

/// `creator` builds the group `options` describes under `group_id`, with
/// `root` (or no root) in its dictionary, adds `joiner`'s current key
/// package, and publishes only the Welcome.
// A test drives OpenMLS convenience commits; their GroupInfo never leaves the test.
#[allow(clippy::disallowed_methods)]
pub(crate) async fn send_crafted_welcome_with(
    creator: &FullXmtpClient,
    joiner: &FullXmtpClient,
    group_id: &[u8],
    root: Option<&GroupRoot>,
    options: CraftedWelcome<'_>,
) {
    let CraftedWelcome {
        conversation_type,
        anchor,
        prior_epoch,
        claimed_creator,
        dm_peer,
    } = options;
    let (kind, policies) = match conversation_type {
        ConversationType::Dm => (
            InitialGroupKind::Dm {
                target_inbox_id: dm_peer.unwrap_or(joiner.inbox_id()),
            },
            PolicySet::new_dm(),
        ),
        conversation_type => (
            InitialGroupKind::Group {
                conversation_type,
                oneshot_message: None,
            },
            PolicySet::default(),
        ),
    };
    let mut dictionary = initial_dictionary(
        kind,
        &policies.to_proto().unwrap(),
        &GroupMetadataOptions::default(),
        creator.inbox_id(),
        root,
        &[],
    )
    .unwrap();
    if let Some(claimed_creator) = claimed_creator {
        dictionary.insert(
            ComponentId::CREATOR_INBOX_ID.as_u16(),
            InboxId::from_hex(claimed_creator)
                .unwrap()
                .tls_serialize_detached()
                .unwrap(),
        );
    }
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
    for _ in 0..prior_epoch {
        group
            .self_update(
                &provider,
                &identity.installation_keys,
                LeafNodeParameters::default(),
            )
            .unwrap();
        group.merge_pending_commit(&provider).unwrap();
    }
    let (_commit, welcome, _group_info) = group
        .add_members(
            &provider,
            &identity.installation_keys,
            std::slice::from_ref(&key_package.inner),
        )
        .unwrap();

    let installation = Installation::from_verified_key_package(&key_package);
    let (data, welcome_metadata) = wrap_payload_hpke(
        &welcome.tls_serialize_detached().unwrap(),
        &WelcomeMetadata {
            message_cursor: anchor,
        }
        .encode_to_vec(),
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
