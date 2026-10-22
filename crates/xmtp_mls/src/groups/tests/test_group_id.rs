use std::collections::HashSet;

use crate::groups::{MlsGroup, PreconfiguredPolicies};
use crate::tester;
use xmtp_db::group::{ConversationType, GroupMembershipState, QueryGroup};
use xmtp_mls_common::{
    app_data::{component_id::ComponentId, group_root::read_group_root},
    group::GroupMetadataOptions,
    group_keys::GroupKeys,
};
use xmtp_proto::xmtp::mls::message_contents::{
    OneshotMessage, ReaddRequest, oneshot_message::MessageType,
};

/// Every conversation type a client creates is keyed: its id is the 32-byte
/// public key of the group signing key derived from the root it stores, each
/// conversation has its own root, and no conversation stores a commit-log
/// signer.
// verifies: GMOD-040, GMOD-041, META-075
#[xmtp_common::test(unwrap_try = true)]
async fn every_new_conversation_is_named_by_its_root() {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);

    let context = || alix.context.clone();
    let oneshot = OneshotMessage {
        message_type: Some(MessageType::ReaddRequest(ReaddRequest {
            group_id: vec![1; 32],
            latest_commit_sequence_id: 1,
        })),
    };
    let created = [
        (ConversationType::Group, alix.create_group(None, None)?),
        (ConversationType::Group, alix.create_group(None, None)?),
        (ConversationType::Group, bo.create_group(None, None)?),
        (
            ConversationType::Dm,
            MlsGroup::create_dm_and_insert(
                &alix.context,
                GroupMembershipState::Allowed,
                bo.inbox_id().to_string(),
                GroupMetadataOptions::default(),
                None,
            )?,
        ),
        (
            ConversationType::Sync,
            MlsGroup::create_and_insert(
                context(),
                ConversationType::Sync,
                PreconfiguredPolicies::default().to_policy_set(),
                GroupMetadataOptions::default(),
                None,
            )?,
        ),
        (
            ConversationType::Oneshot,
            MlsGroup::create_and_insert(
                context(),
                ConversationType::Oneshot,
                PreconfiguredPolicies::default().to_policy_set(),
                GroupMetadataOptions::default(),
                Some(oneshot),
            )?,
        ),
    ];

    let mut roots = HashSet::new();
    let mut ids = HashSet::new();
    for (conversation_type, group) in &created {
        let (mls_id, root, signer, registered) = group.with_group_snapshot(|mls_group| {
            let extensions = mls_group.extensions();
            let dictionary = extensions.app_data_dictionary().map(|d| d.dictionary());
            let registry = crate::groups::app_data::load_component_registry(mls_group)
                .map_err(|_| crate::groups::GroupError::InvalidWelcomeMetadata)?;
            Ok((
                mls_group.group_id().as_slice().to_vec(),
                read_group_root(extensions)
                    .map_err(|_| crate::groups::GroupError::InvalidWelcomeMetadata)?,
                dictionary.is_some_and(|d| d.contains(&ComponentId::COMMIT_LOG_SIGNER.as_u16())),
                registry.contains(&ComponentId::GROUP_ROOT_KEY),
            ))
        })?;
        let root = root.unwrap_or_else(|| panic!("a new {conversation_type:?} stores its root"));
        assert_eq!(group.group_id.len(), 32, "{conversation_type:?}");
        assert_eq!(mls_id, group.group_id.to_vec(), "{conversation_type:?}");
        assert_eq!(
            GroupKeys::derive(&root).group_id().as_slice(),
            group.group_id.as_slice(),
            "a {conversation_type:?} id is its root's group signing key"
        );
        assert!(!signer, "a new {conversation_type:?} stores no signer");
        assert!(registered, "{conversation_type:?} registers GROUP_ROOT_KEY");
        let stored = group.context.db().find_group(&group.group_id)??;
        assert_eq!(stored.conversation_type, *conversation_type);
        assert!(
            roots.insert(*root.as_bytes()),
            "two conversations share a root"
        );
        assert!(ids.insert(group.group_id), "two conversations share an id");
    }
}
