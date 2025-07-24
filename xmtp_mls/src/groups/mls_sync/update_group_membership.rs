use super::*;
use crate::groups::{
    build_group_membership_extension,
    intents::{PostCommitAction, UpdateGroupMembershipIntentData},
    validated_commit::extract_group_membership,
    GroupError,
};
use openmls::{
    extensions::Extensions,
    prelude::{tls_codec::Serialize, LeafNodeIndex, MlsGroup as OpenMlsGroup},
};
use openmls_traits::signatures::Signer;

// Takes UpdateGroupMembershipIntentData and applies it to the openmls group
// returning the commit and post_commit_action
#[tracing::instrument(level = "trace", skip_all)]
pub(super) async fn apply_update_group_membership_intent(
    context: &impl XmtpSharedContext,
    openmls_group: &mut OpenMlsGroup,
    intent_data: UpdateGroupMembershipIntentData,
    signer: impl Signer,
) -> Result<Option<PublishIntentData>, GroupError> {
    let extensions: Extensions = openmls_group.extensions().clone();
    let old_group_membership = extract_group_membership(&extensions)?;
    let new_group_membership = intent_data.apply_to_group_membership(&old_group_membership);
    let membership_diff = old_group_membership.diff(&new_group_membership);

    let changes_with_kps = calculate_membership_changes_with_keypackages(
        context,
        &new_group_membership,
        &old_group_membership,
    )
    .await?;
    let leaf_nodes_to_remove: Vec<LeafNodeIndex> =
        get_removed_leaf_nodes(openmls_group, &changes_with_kps.removed_installations);

    if leaf_nodes_to_remove.contains(&openmls_group.own_leaf_index()) {
        tracing::info!("Cannot remove own leaf node");
        return Ok(None);
    }

    if leaf_nodes_to_remove.is_empty()
        && changes_with_kps.new_key_packages.is_empty()
        && membership_diff.updated_inboxes.is_empty()
    {
        return Ok(None);
    }

    // Update the extensions to have the new GroupMembership
    let mut new_extensions = extensions.clone();

    new_extensions.add_or_replace(build_group_membership_extension(&new_group_membership));

    let (commit, post_commit_action, staged_commit) =
        context.mls_storage().transaction(|conn| {
            let storage = conn.key_store();
            let provider = XmtpOpenMlsProvider::new(storage);
            // Create the commit
            let (commit, maybe_welcome_message, _) = openmls_group.update_group_membership(
                &provider,
                &signer,
                &changes_with_kps.new_key_packages,
                &leaf_nodes_to_remove,
                new_extensions,
            )?;

            let post_commit_action = match maybe_welcome_message {
                Some(welcome_message) => Some(PostCommitAction::from_welcome(
                    welcome_message,
                    changes_with_kps.new_installations,
                )?),
                None => None,
            };

            let staged_commit = get_and_clear_pending_commit(openmls_group, provider.storage())?
                .ok_or_else(|| GroupError::MissingPendingCommit)?;

            Ok::<_, GroupError>((commit, post_commit_action, staged_commit))
        })?;

    Ok(Some(PublishIntentData {
        payload_to_publish: commit.tls_serialize_detached()?,
        post_commit_action: post_commit_action.map(|action| action.to_bytes()),
        staged_commit: Some(staged_commit),
        should_send_push_notification: false,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, RwLock};

    use crate::{
        groups::{
            build_group_config, build_mutable_metadata_extension_default,
            build_mutable_permissions_extension, build_protected_metadata_extension,
            build_starting_group_membership_extension,
        },
        identity::create_credential,
        test::mock::{context, NewMockContext},
    };
    use openmls::{group::MlsGroupCreateConfig, prelude::CredentialWithKey};
    use rstest::*;
    use xmtp_cryptography::configuration::CIPHERSUITE;
    use xmtp_cryptography::XmtpInstallationCredential;
    use xmtp_db::mock::MockDbQuery;

    fn generate_config(
        creator_inbox: &str,
        members: &[&str],
    ) -> Result<MlsGroupCreateConfig, GroupError> {
        let mut membership = GroupMembership::new();
        membership.add(creator_inbox.to_string(), 0);
        members
            .iter()
            .for_each(|m| membership.add(m.to_string(), 0));
        let _group_membership = build_group_membership_extension(&membership);
        let protected_metadata =
            build_protected_metadata_extension(creator_inbox, ConversationType::Group)?;
        let mutable_metadata =
            build_mutable_metadata_extension_default(creator_inbox, Default::default())?;
        let group_membership = build_starting_group_membership_extension(creator_inbox, 0);
        let mutable_permissions = build_mutable_permissions_extension(Default::default())?;
        let group_config = build_group_config(
            protected_metadata,
            mutable_metadata,
            group_membership,
            mutable_permissions,
        )?;
        Ok(group_config)
    }

    #[rstest]
    #[xmtp_common::test]
    #[allow(clippy::readonly_write_lock, clippy::await_holding_lock)]
    async fn applies_group_membership_intent(mut context: NewMockContext) {
        let mut credentials = HashMap::new();
        let installation_key = XmtpInstallationCredential::new();
        let key_pair = openmls_basic_credential::SignatureKeyPair::from(installation_key.clone());
        key_pair.store(&context.mls_storage).unwrap();
        let signature_key = installation_key.clone().into();
        let credential = CredentialWithKey {
            credential: create_credential("alice").unwrap(),
            signature_key,
        };
        credentials.insert(CIPHERSUITE, credential);
        // create a mocked, MLS client + group using openmls test framework
        let client = openmls::test_utils::test_framework::client::Client::<_> {
            identity: b"alice".to_vec(),
            credentials,
            provider: XmtpOpenMlsProviderRef::new(&context.mls_storage),
            groups: RwLock::new(HashMap::new()),
        };
        let config = generate_config("alice", &["bob", "caro", "eve"]).unwrap();
        let id = client.create_group(config, CIPHERSUITE).unwrap();
        let installation = XmtpInstallationCredential::new();

        let db_calls = || {
            let mut mock_db = MockDbQuery::new();
            mock_db
                .expect_get_latest_sequence_id()
                .returning(|_ids| Ok(HashMap::new()));
            mock_db
        };
        context.store.expect_db().returning(db_calls);

        let mut groups = client.groups.write().unwrap();
        let g = groups.get_mut(&id).unwrap();

        // once context is in an arc, can no longer set expectations
        let context = Arc::new(context);
        let intent = apply_update_group_membership_intent(
            context.as_ref(),
            g,
            UpdateGroupMembershipIntentData {
                membership_updates: HashMap::new(),
                removed_members: Vec::new(),
                failed_installations: Vec::new(),
            },
            installation,
        )
        .await
        .unwrap();
        assert!(intent.is_none());
    }
}
