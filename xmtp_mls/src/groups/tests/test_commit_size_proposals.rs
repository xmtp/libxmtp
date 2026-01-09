use crate::context::XmtpSharedContext;
use crate::groups::mls_ext::{CommitLogStorer, build_group_join_config};
use crate::groups::mls_sync::{generate_commit_with_rollback, with_rollback};
use crate::groups::send_message_opts::SendMessageOpts;
use crate::groups::{GroupError, MessageDisappearingSettings, MlsGroup};
use crate::identity::NewKeyPackageResult;
use crate::identity::parse_credential;
use crate::utils::TestXmtpMlsContext;
use openmls::credentials::BasicCredential;
use openmls::group::MlsGroup as OpenMlsGroup;
use openmls::prelude::hash_ref::HashReference;
use openmls::prelude::tls_codec::Serialize;
use openmls::prelude::*;
use tls_codec::Deserialize;
use xmtp_common::time::now_ns;
use xmtp_db::group_message::{GroupMessageKind, MsgQueryArgs};
use xmtp_db::{
    TransactionalKeyStore, XmtpMlsStorageProvider, XmtpOpenMlsProviderRef, prelude::QueryGroup,
};
use xmtp_db::{
    consent_record::StoredConsentRecord,
    group::{GroupMembershipState, StoredGroup},
};
use xmtp_id::InboxOwner;
use xmtp_mls_common::{
    group_metadata::extract_group_metadata, group_mutable_metadata::extract_group_mutable_metadata,
};

struct ProposalsAndCommit {
    proposals: Vec<MlsMessageOut>,
    commit: MlsMessageOut,
    welcome: MlsMessageOut,
    old_commit: MlsMessageOut,
    old_welcome: MlsMessageOut,
}

async fn generate_add_proposals_and_commit<Context: XmtpSharedContext>(
    groups: &[MlsGroup<Context>],
    testers: &[crate::utils::test::tester_utils::Tester],
    key_packages: &[NewKeyPackageResult],
) -> ProposalsAndCommit {
    let provider = groups[0].context.mls_provider();
    let signer = testers[0].identity().installation_keys.clone();
    let storage = groups[0].context.mls_storage();
    groups[0]
        .load_mls_group_with_lock_async(async |mut mls_group| {
            let (old_commit, old_welcome, _) =
                with_rollback(storage, &mut mls_group, |group, provider| {
                    group
                        .update_group_membership(
                            provider,
                            &testers[0].identity().installation_keys,
                            &key_packages
                                .iter()
                                .map(|k| k.key_package.clone())
                                .collect::<Vec<_>>(),
                            &[],
                            group.extensions().clone(),
                        )
                        .map_err(|e| GroupError::Any(e.into()))
                })
                .unwrap();
            let epoch = mls_group.epoch();
            let proposals = key_packages
                .iter()
                .map(|k| {
                    let (proposal, _) = mls_group
                        .propose_add_member(&provider, &signer, &k.key_package)
                        .map_err(|e| GroupError::Any(e.into()))
                        .unwrap();
                    proposal
                })
                .collect::<Vec<_>>();
            let (commit, welcome, _) = mls_group
                .commit_to_pending_proposals(&provider, &signer)
                .map_err(|e| GroupError::Any(e.into()))
                .unwrap();
            mls_group.merge_pending_commit(&provider).unwrap();
            let p_and_c = ProposalsAndCommit {
                proposals,
                commit: commit,
                welcome: welcome.unwrap(),
                old_commit: old_commit,
                old_welcome: old_welcome.unwrap(),
            };
            Ok::<_, GroupError>(p_and_c)
        })
        .await
        .unwrap()
}

async fn welcome_testers(
    testers: &[crate::utils::test::tester_utils::Tester],
    welcome: &Welcome,
) -> Vec<MlsGroup<TestXmtpMlsContext>> {
    let mut groups = vec![];
    for tester in testers {
        let tester_storage = tester.context.mls_storage();
        let tester_provider = XmtpOpenMlsProviderRef::new(tester_storage);

        // Build staged welcome from the OpenMLS Welcome message
        let join_config = build_group_join_config();
        let builder =
            StagedWelcome::build_from_welcome(&tester_provider, &join_config, welcome.clone())
                .map_err(|e| GroupError::Any(e.into()))
                .unwrap();

        let processed_welcome = builder.processed_welcome();
        let psks = processed_welcome.psks();
        if !psks.is_empty() {
            panic!("No PSK support for welcome");
        }

        let staged_welcome = builder
            .skip_lifetime_validation()
            .build()
            .map_err(|e| GroupError::Any(e.into()))
            .unwrap();

        // Get sender information from the staged welcome
        let added_by_node = staged_welcome
            .welcome_sender()
            .map_err(|e| GroupError::Any(e.into()))
            .unwrap();
        let added_by_credential = BasicCredential::try_from(added_by_node.credential().clone())
            .map_err(|e| GroupError::Any(e.into()))
            .unwrap();
        let added_by_inbox_id = parse_credential(added_by_credential.identity())
            .map_err(|e| GroupError::Any(e.into()))
            .unwrap();
        let added_by_installation_id = added_by_node.signature_key().as_slice().to_vec();

        // Create the OpenMLS group and full XMTP MlsGroup with database entries

        // Create the OpenMLS group from the staged welcome
        let mls_group = OpenMlsGroup::from_welcome_logged(
            &tester_provider,
            staged_welcome,
            &added_by_inbox_id,
            &added_by_installation_id,
        )
        .map_err(|e| GroupError::Any(e.into()))
        .unwrap();

        // Extract metadata from the group
        let group_id = mls_group.group_id().to_vec();
        let metadata = extract_group_metadata(mls_group.extensions())
            .map_err(|e| GroupError::Any(e.into()))
            .unwrap();
        let mutable_metadata = extract_group_mutable_metadata(&mls_group).ok();
        let conversation_type = metadata.conversation_type;
        let dm_members = metadata.dm_members;

        // Create and store the StoredGroup in the database
        let stored_group = tester
            .context
            .mls_storage()
            .transaction(|conn| {
                let storage = conn.key_store();
                let db = storage.db();

                // Extract disappearing settings from mutable metadata if available
                // For simplicity in the test, we'll just set to None

                let disappearing_settings: Option<MessageDisappearingSettings> = None;

                // Since the tester is already fully in the group via the welcome message,
                // set membership state to Allowed (not Pending) to avoid duplicate signature key errors
                // when syncing tries to process any pending intents
                let stored_group = StoredGroup::builder()
                    .id(group_id.clone())
                    .created_at_ns(now_ns())
                    .added_by_inbox_id(&added_by_inbox_id)
                    .conversation_type(conversation_type)
                    .membership_state(GroupMembershipState::Allowed)
                    .dm_id(dm_members.map(String::from))
                    .message_disappear_from_ns(disappearing_settings.as_ref().map(|m| m.from_ns))
                    .message_disappear_in_ns(disappearing_settings.as_ref().map(|m| m.in_ns))
                    .should_publish_commit_log(false) // For test, we don't need to publish commit log
                    .installations_last_checked(now_ns()) // If this isn't set we get errors trying to sync
                    .build()
                    .map_err(|e| GroupError::Any(e.into()))?;

                let stored_group = db.insert_or_replace_group(stored_group)?;
                StoredConsentRecord::stitch_dm_consent(&db, &stored_group)?;

                Ok::<_, GroupError>(stored_group)
            })
            .unwrap();

        let tester_group = tester.group(&group_id).unwrap();

        groups.push(tester_group);
    }
    let sync_groups = futures::future::join_all(groups.iter().map(|g| g.sync())).await;
    for group in sync_groups {
        group.unwrap();
    }
    groups
}

#[xmtp_common::test]
async fn test_commit_sizes_with_proposals() {
    const TESTER_COUNT: usize = 10;
    crate::tester!(alix);

    let mut testers = vec![alix];

    let mut groups = vec![testers[0].create_group(None, None).unwrap()];

    for i in 0..TESTER_COUNT {
        tracing::warn!("TESTER {i}");
        crate::tester!(tester);

        let new_testers = vec![tester];

        let key_packages = new_testers
            .iter()
            .map(|t| {
                t.identity()
                    .new_key_package(&t.context.mls_provider(), true)
                    .unwrap()
            })
            .collect::<Vec<_>>();
        let proposals_and_commit =
            generate_add_proposals_and_commit(&groups, &testers, &key_packages).await;
        let proposals = proposals_and_commit
            .proposals
            .iter()
            .map(|p| p.tls_serialize_detached().unwrap())
            .collect::<Vec<_>>();

        let proposals_size = proposals.iter().map(|p| p.len()).sum::<usize>();

        let protocol_messages = proposals
            .iter()
            .map(|p| {
                MlsMessageIn::tls_deserialize_exact(p)
                    .unwrap()
                    .try_into_protocol_message()
                    .unwrap()
            })
            .collect::<Vec<_>>();

        let commit = proposals_and_commit
            .commit
            .tls_serialize_detached()
            .unwrap();
        let commit_message = MlsMessageIn::tls_deserialize_exact(&commit)
            .unwrap()
            .try_into_protocol_message()
            .unwrap();

        // skip over originating group
        let add_proposals_and_commit = groups.iter().enumerate().skip(1).map(|(i, g)| {
            let protocol_messages = protocol_messages.clone();
            let commit_message = commit_message.clone();
            let storage = g.context.mls_storage();
            async move {
                g.sync().await.unwrap();
                g.load_mls_group_with_lock_async(async move |mut mls_group| {
                    let provider = g.context.mls_provider();
                    let epoch = mls_group.epoch();
                    tracing::info!("GROUP {i} EPOCH {epoch}");
                    for protocol_message in protocol_messages {
                        let x = mls_group
                            .process_message(&provider, protocol_message)
                            .unwrap();
                        let ProcessedMessageContent::ProposalMessage(proposal) = x.into_content()
                        else {
                            panic!("Expected ProposalMessage");
                        };
                        mls_group
                            .store_pending_proposal(storage, *proposal)
                            .unwrap();
                    }
                    let x = mls_group
                        .process_message(&provider, commit_message)
                        .unwrap();
                    let ProcessedMessageContent::StagedCommitMessage(staged_commit) =
                        x.into_content()
                    else {
                        panic!("Expected StagedCommitMessage");
                    };
                    mls_group
                        .merge_staged_commit(&provider, *staged_commit)
                        .unwrap();
                    Ok::<_, GroupError>(())
                })
                .await
                .unwrap();
            }
        });

        futures::future::join_all(add_proposals_and_commit).await;

        let old_commit = proposals_and_commit
            .old_commit
            .tls_serialize_detached()
            .unwrap();
        let old_welcome = proposals_and_commit
            .old_welcome
            .tls_serialize_detached()
            .unwrap();
        let welcome_bytes = proposals_and_commit
            .welcome
            .tls_serialize_detached()
            .unwrap();
        let welcome_size = welcome_bytes.len();

        tracing::warn!(
            i,
            proposals_size,
            old_commit_size = old_commit.len(),
            old_welcome_size = old_welcome.len(),
            commit_size = commit.len(),
            welcome_size,
            welcome_diff = (old_welcome.len() as isize - welcome_size as isize),
            commit_diff =
                (old_commit.len() as isize - commit.len() as isize - proposals_size as isize),
            commit_pct =
                (commit.len() as f64 + proposals_size as f64) / (old_commit.len() as f64) * 100.0,
            "Commit sizes"
        );

        // Process welcome directly into OpenMLS group, bypassing network calls and validation
        // MlsMessageOut when serialized contains the Welcome message
        // We need to deserialize as MlsMessageIn first, then extract the Welcome
        let mls_message_in = MlsMessageIn::tls_deserialize_exact(&welcome_bytes)
            .map_err(|e| GroupError::Any(e.into()))
            .unwrap();

        // Extract Welcome from MlsMessageIn
        // MlsMessageIn contains the Welcome in its body
        let MlsMessageBodyIn::Welcome(openmls_welcome) = mls_message_in.extract() else {
            panic!("Expected Welcome message, got different message type");
        };

        let new_groups = welcome_testers(&new_testers, &openmls_welcome).await;

        // The tester is already fully in the group via the welcome message.
        // The group membership extension should already reflect this.
        // However, if there's a pending intent to add the tester (created before the welcome
        // was processed), syncing will try to process it and fail with "Duplicate signature key".
        // The intent system should detect that the tester is already in the group and skip
        // the intent, but to be safe, we ensure the group state is consistent first.
        //
        // Note: We don't process the commit message here because it was already applied
        // via the welcome. Processing it again would fail.

        // Sync the group. The intent system should detect that the tester is already
        // in the group and skip any conflicting intents.
        // tester_group.sync().await.unwrap();
        // Group is now fully set up and ready for the tester to use

        // Have the new tester send a message directly to the API, bypassing all checks and intents
        let message_bytes = format!("Hello from new tester! {}", i);

        // TODO: this should be tester_group, but if it is used, we end up with a panic due to duplicate signatures.
        // For some reason waiting a commit fixes it :shrug:
        let last_group = new_groups.last().unwrap();

        last_group
            .send_message(message_bytes.as_bytes(), SendMessageOpts::default())
            .await
            .unwrap();

        // Send a self-update commit to update the last_group's leaf node and fill the tree
        let last_tester = new_testers.last().unwrap();
        let storage = last_group.context.mls_storage();
        let commit_bytes = last_group
            .load_mls_group_with_lock_async(async |mut mls_group| {
                use openmls::treesync::LeafNodeParameters;

                let provider = XmtpOpenMlsProviderRef::new(storage);
                let signer = last_tester.identity().installation_keys.clone();

                let bundle = mls_group
                    .self_update(&provider, &signer, LeafNodeParameters::default())
                    .map_err(|e| GroupError::Any(e.into()))?;

                // Merge the pending commit to apply it locally
                mls_group
                    .merge_pending_commit(&provider)
                    .map_err(|e| GroupError::Any(e.into()))?;
                tracing::warn!(epoch = %mls_group.epoch(), tree = %mls_group.export_ratchet_tree(), "Merged leaf update commit");
                Ok::<_, GroupError>(bundle.commit().tls_serialize_detached().unwrap())
            })
            .await
            .unwrap();

        let commit_message = MlsMessageIn::tls_deserialize_exact(&commit_bytes)
            .unwrap()
            .try_into_protocol_message()
            .unwrap();

        let commits = groups.iter().map(|g| async {
            g.load_mls_group_with_lock_async(async |mut mls_group| {
                let provider = g.context.mls_provider();
                let x = mls_group
                    .process_message(&provider, commit_message.clone())
                    .unwrap();
                let ProcessedMessageContent::StagedCommitMessage(staged_commit) = x.into_content()
                else {
                    panic!("Expected StagedCommitMessage");
                };
                mls_group
                    .merge_staged_commit(&provider, *staged_commit)
                    .unwrap();
                tracing::warn!(epoch = %mls_group.epoch(), "Merged leaf update commit");
                Ok::<_, GroupError>(())
            })
            .await?;
            g.sync().await?;
            Ok::<_, GroupError>(())
        });

        let results = futures::future::join_all(commits).await;
        for result in results {
            result.unwrap();
        }

        let group0 = groups.first().unwrap();
        // group0.sync().await.unwrap();
        // last_group.sync().await.unwrap();
        let group0_tree = group0
            .load_mls_group_with_lock_async(async |mls_group| {
                // mls_group.print_ratchet_tree("");
                Ok::<_, GroupError>(mls_group.export_ratchet_tree())
            })
            .await
            .unwrap();
        let last_group_tree = last_group
            .load_mls_group_with_lock_async(async |mls_group| {
                // mls_group.print_ratchet_tree("");
                Ok::<_, GroupError>(mls_group.export_ratchet_tree())
            })
            .await
            .unwrap();
        let group0_tree_str = format!("{}", group0_tree);
        let last_group_tree_str = format!("{}", last_group_tree);
        if group0_tree_str != last_group_tree_str {
            println!("Group 0 tree:\n{}", group0_tree_str);
            println!("Last group tree:\n{}", last_group_tree_str);
            tracing::warn!("Group 0 tree and last group tree are not equal");
        }

        // Only application messages (not system/membership messages)
        let app_messages = group0
            .find_messages(&MsgQueryArgs {
                kind: Some(GroupMessageKind::Application),
                ..Default::default()
            })
            .unwrap();
        tracing::warn!("Group 0 has {} application messages", app_messages.len());
        for message in app_messages {
            tracing::warn!(
                "Message: {}, sent at: {}",
                String::from_utf8_lossy(&message.decrypted_message_bytes),
                message.sent_at_ns
            );
        }

        // Only application messages (not system/membership messages)
        let app_messages = last_group
            .find_messages(&MsgQueryArgs {
                kind: Some(GroupMessageKind::Application),
                ..Default::default()
            })
            .unwrap();
        tracing::warn!(
            "Group last {} has {} application messages",
            i.wrapping_sub(1),
            app_messages.len()
        );
        for message in app_messages {
            tracing::warn!(
                "Message: {}, sent at: {}",
                String::from_utf8_lossy(&message.decrypted_message_bytes),
                message.sent_at_ns
            );
        }

        let group0_members = group0.members().await.unwrap();
        // let last_group_members = last_group.members().await.unwrap();
        let inbox_ids = group0_members
            .iter()
            .map(|m| &m.inbox_id)
            .collect::<Vec<_>>();
        // assert_eq!(
        //     inbox_ids,
        //     last_group_members
        //         .iter()
        //         .map(|m| &m.inbox_id)
        //         .collect::<Vec<_>>()
        // );
        dbg!(&inbox_ids);
        tracing::warn!("Group 0 members: {:?}", inbox_ids);
        tracing::warn!("Group 0 tree:\n{}", group0_tree);
        // dbg!(&group0_tree);

        // Add the tester to the group
        // TODO: this is a hack to get the new groups into the groups vector without naming them.
        let mut new_groups = new_groups;
        groups.append(&mut new_groups);
        let mut new_testers = new_testers;
        testers.append(&mut new_testers);
    }
}
