use std::sync::Arc;

use crate::context::XmtpSharedContext;
use crate::groups::mls_ext::{CommitLogStorer, build_group_join_config};
use crate::groups::mls_sync::with_rollback;
use crate::groups::send_message_opts::SendMessageOpts;
use crate::groups::{GroupError, MessageDisappearingSettings, MlsGroup};
use crate::identity::NewKeyPackageResult;
use crate::identity::parse_credential;
use crate::utils::TestXmtpMlsContext;
use crate::utils::test::tester_utils::Tester;
use openmls::credentials::BasicCredential;
use openmls::group::MlsGroup as OpenMlsGroup;
use openmls::prelude::tls_codec::Serialize;
use openmls::prelude::*;
use rand::distributions::uniform::SampleRange;
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
use xmtp_mls_common::group_metadata::extract_group_metadata;

const ITERATION_COUNT: usize = 50;
const TESTERS_PER_ITERATION: std::ops::RangeInclusive<usize> = 1..=1;
const INSTALLATIONS_PER_TESTER: std::ops::RangeInclusive<usize> = 9..=9;
const PCT_FILL_LEAF_NODES: f64 = 0.0625;
const FILL_LEAF_NODE_EVERY: usize = 8;

struct ProposalsAndCommit {
    proposals: Vec<MlsMessageOut>,
    adds: usize,
    removes: usize,
    add_avg_size: usize,
    remove_avg_size: usize,
    add_total_size: usize,
    remove_total_size: usize,
    commit: MlsMessageOut,
    welcome: Option<MlsMessageOut>,
    old_commit_len: usize,
    old_welcome_len: usize,
    old_commit_builder_len: usize,
    old_welcome_builder_len: usize,
}

trait TransmuteTree {
    fn ratchet_tree(&self) -> Vec<Option<openmls::treesync::Node>>;
}

impl TransmuteTree for OpenMlsGroup {
    fn ratchet_tree(&self) -> Vec<Option<openmls::treesync::Node>> {
        let tree = self.export_ratchet_tree();
        // Safety: this isn't technically safe, but it's easier than changing all the deps to get this exposed for now.
        unsafe { std::mem::transmute(tree) }
    }
}

async fn generate_proposals_and_commit(
    testers: &[(Tester, MlsGroup<TestXmtpMlsContext>)],
    key_packages: &[NewKeyPackageResult],
    leaf_nodes_to_remove: &[LeafNodeIndex],
) -> ProposalsAndCommit {
    let provider = testers[0].1.context.mls_provider();
    let signer = testers[0].0.identity().installation_keys.clone();
    let storage = testers[0].1.context.mls_storage();
    let proposals_and_commit = testers[0]
        .1
        .load_mls_group_with_lock_async(async |mut mls_group| {
            tracing::warn!("Creating proposals for group 0 epoch {}", mls_group.epoch());
            let (old_commit, old_welcome) =
                with_rollback(storage, &mut mls_group, |group, provider| {
                    let (commit, welcome, _) = group
                        .update_group_membership(
                            provider,
                            &testers[0].0.identity().installation_keys,
                            &key_packages
                                .iter()
                                .map(|k| k.key_package.clone())
                                .collect::<Vec<_>>(),
                            leaf_nodes_to_remove,
                            group.extensions().clone(),
                        )
                        .unwrap();
                    Ok::<_, GroupError>((
                        commit.tls_serialize_detached().unwrap().len(),
                        welcome.unwrap().tls_serialize_detached().unwrap().len(),
                    ))
                })
                .unwrap();

            let (old_commit_builder, old_welcome_builder) =
                with_rollback(storage, &mut mls_group, |group, provider| {
                    let commit_builder = group
                        .commit_builder()
                        .propose_adds(key_packages.iter().map(|k| k.key_package.clone()))
                        .propose_removals(leaf_nodes_to_remove.iter().copied())
                        .force_self_update(true)
                        .load_psks(provider.storage())
                        .unwrap()
                        .build(provider.rand(), provider.crypto(), &signer, |_| true)
                        .unwrap();
                    let bundle = commit_builder.stage_commit(provider).unwrap();
                    Ok::<_, GroupError>((
                        bundle.commit().tls_serialize_detached().unwrap().len(),
                        bundle
                            .welcome()
                            .map(|w| w.tls_serialize_detached().unwrap().len())
                            .unwrap_or(0),
                    ))
                })
                .unwrap();

            let remove_proposals = leaf_nodes_to_remove
                .iter()
                .map(|i| {
                    tracing::warn!("Proposing to remove leaf node {i}");
                    let (proposal, _) = mls_group
                        .propose_remove_member(&provider, &signer, *i)
                        .unwrap();
                    proposal
                })
                .collect::<Vec<_>>();

            let add_proposals = key_packages
                .iter()
                .map(|k| {
                    let (proposal, _) = mls_group
                        .propose_add_member(&provider, &signer, &k.key_package)
                        .unwrap();
                    proposal
                })
                .collect::<Vec<_>>();
            let leaf_node = mls_group.own_leaf().unwrap();
            let self_update = mls_group
                .propose_self_update(
                    &provider,
                    &signer,
                    openmls::treesync::LeafNodeParameters::builder()
                        .with_capabilities(leaf_node.capabilities().clone())
                        .with_extensions(leaf_node.extensions().clone())
                        .unwrap()
                        .build(),
                )
                .unwrap()
                .0;

            let (commit, welcome, _) = mls_group
                .commit_to_pending_proposals(&provider, &signer)
                .unwrap();
            mls_group.merge_pending_commit(&provider).unwrap();
            let proposals_and_commit = ProposalsAndCommit {
                adds: add_proposals.len(),
                removes: remove_proposals.len(),
                add_avg_size: add_proposals
                    .iter()
                    .map(|p| p.tls_serialize_detached().unwrap().len())
                    .sum::<usize>()
                    / add_proposals.len().max(1),
                remove_avg_size: remove_proposals
                    .iter()
                    .map(|p| p.tls_serialize_detached().unwrap().len())
                    .sum::<usize>()
                    / remove_proposals.len().max(1),
                add_total_size: add_proposals
                    .iter()
                    .map(|p| p.tls_serialize_detached().unwrap().len())
                    .sum::<usize>(),
                remove_total_size: remove_proposals
                    .iter()
                    .map(|p| p.tls_serialize_detached().unwrap().len())
                    .sum::<usize>(),
                proposals: remove_proposals
                    .into_iter()
                    .chain(add_proposals)
                    .chain([self_update])
                    .collect(),
                commit,
                welcome,
                old_commit_len: old_commit,
                old_welcome_len: old_welcome,
                old_commit_builder_len: old_commit_builder,
                old_welcome_builder_len: old_welcome_builder,
            };
            Ok::<_, GroupError>(proposals_and_commit)
        })
        .await
        .unwrap();

    let proposals = proposals_and_commit
        .proposals
        .iter()
        .map(|p| p.tls_serialize_detached().unwrap())
        .collect::<Vec<_>>();

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
    let add_proposals_and_commit =
        testers
            .iter()
            .enumerate()
            .skip(1)
            .map(|(i, (_tester, group))| {
                let protocol_messages = protocol_messages.clone();
                let commit_message = commit_message.clone();
                let storage = group.context.mls_storage();
                async move {
                    // group.sync().await.unwrap();
                    group
                        .load_mls_group_with_lock_async(async move |mut mls_group| {
                            let provider = group.context.mls_provider();
                            let epoch = mls_group.epoch();
                            tracing::info!("GROUP {i} EPOCH {epoch}");
                            for (j, protocol_message) in protocol_messages.into_iter().enumerate() {
                                tracing::info!(
                                    // ?protocol_message,
                                    "Processing proposal message {j} for group {i} epoch {epoch}"
                                );
                                let x = mls_group
                                    .process_message(&provider, protocol_message)
                                    .unwrap();
                                let ProcessedMessageContent::ProposalMessage(proposal) =
                                    x.into_content()
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
    proposals_and_commit
}

async fn welcome_testers(
    testers: &[Tester],
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
                .unwrap();

        let processed_welcome = builder.processed_welcome();
        let psks = processed_welcome.psks();
        if !psks.is_empty() {
            panic!("No PSK support for welcome");
        }

        let staged_welcome = builder.skip_lifetime_validation().build().unwrap();

        // Get sender information from the staged welcome
        let added_by_node = staged_welcome.welcome_sender().unwrap();
        let added_by_credential =
            BasicCredential::try_from(added_by_node.credential().clone()).unwrap();
        let added_by_inbox_id = parse_credential(added_by_credential.identity()).unwrap();
        let added_by_installation_id = added_by_node.signature_key().as_slice().to_vec();

        // Create the OpenMLS group and full XMTP MlsGroup with database entries

        // Create the OpenMLS group from the staged welcome
        let mls_group = OpenMlsGroup::from_welcome_logged(
            &tester_provider,
            staged_welcome,
            &added_by_inbox_id,
            &added_by_installation_id,
        )
        .unwrap();

        // Extract metadata from the group
        let group_id = mls_group.group_id().to_vec();
        let metadata = extract_group_metadata(mls_group.extensions()).unwrap();
        // let mutable_metadata = extract_group_mutable_metadata(&mls_group).ok();
        let conversation_type = metadata.conversation_type;
        let dm_members = metadata.dm_members;

        // Create and store the StoredGroup in the database
        let _stored_group = tester
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
                    .unwrap();

                let stored_group = db.insert_or_replace_group(stored_group)?;
                StoredConsentRecord::stitch_dm_consent(&db, &stored_group)?;

                Ok::<_, GroupError>(stored_group)
            })
            .unwrap();

        let tester_group = tester.group(&group_id).unwrap();

        groups.push(tester_group);
    }
    // let sync_groups = futures::future::join_all(groups.iter().map(|g| g.sync())).await;
    // for group in sync_groups {
    //     group.unwrap();
    // }
    groups
}

async fn sync_groups(testers: impl Iterator<Item = &(Tester, MlsGroup<TestXmtpMlsContext>)>) {
    let sync_groups = futures::future::join_all(testers.map(|(_, g)| g.sync())).await;
    for group in sync_groups {
        group.unwrap();
    }
}

async fn fill_leaf_nodes(
    new_testers: impl Iterator<Item = &(Tester, MlsGroup<TestXmtpMlsContext>)>,
    testers: impl Iterator<Item = &(Tester, MlsGroup<TestXmtpMlsContext>)> + Clone,
) {
    for (last_tester, last_group) in new_testers {
        let storage = last_group.context.mls_storage();
        let commit_bytes = last_group
            .load_mls_group_with_lock_async(async |mut mls_group| {
                use openmls::treesync::LeafNodeParameters;

                let provider = XmtpOpenMlsProviderRef::new(storage);
                let signer = last_tester.identity().installation_keys.clone();

                let bundle = mls_group
                    .self_update(&provider, &signer, LeafNodeParameters::default())
                    .unwrap();

                // Merge the pending commit to apply it locally
                mls_group
                    .merge_pending_commit(&provider)
                    .unwrap();
                tracing::warn!(epoch = %mls_group.epoch(), "Merged leaf update commit");
                tracing::warn!("Tree: \n{}", mls_group.export_ratchet_tree());
                let tree = mls_group.ratchet_tree();
                let leaf_nodes = tree
                    .iter()
                    .filter_map(|node| {
                        let Some(openmls::treesync::Node::LeafNode(leaf)) = node else {
                            return None;
                        };
                        Some(leaf)
                    })
                    .collect::<Vec<_>>();
                let mut lns_key_packages = 0;
                let mut lns_updates = 0;
                let mut lns_commits = 0;
                leaf_nodes.iter().for_each(|leaf| match leaf.leaf_node_source() {
                    openmls::treesync::LeafNodeSource::Commit(_) => lns_commits += 1,
                    openmls::treesync::LeafNodeSource::KeyPackage(_) => lns_key_packages += 1,
                    openmls::treesync::LeafNodeSource::Update => lns_updates += 1,
                });
                let parent_nodes = tree
                    .iter()
                    .filter_map(|node| {
                        let Some(openmls::treesync::Node::ParentNode(parent)) = node else {
                            return None;
                        };
                        Some(parent)
                    })
                    .collect::<Vec<_>>();
                tracing::warn!(leaf_nodes = %leaf_nodes.len(), parent_nodes = %parent_nodes.len(), lns_key_packages, lns_updates, lns_commits, "Tree Debug");
                Ok::<_, GroupError>(bundle.commit().tls_serialize_detached().unwrap())
            })
            .await
            .unwrap();
        last_group.sync().await.unwrap();

        tracing::warn!(commit_bytes = %commit_bytes.len(), "Leaf update commit size");

        let commit_message = MlsMessageIn::tls_deserialize_exact(&commit_bytes)
            .unwrap()
            .try_into_protocol_message()
            .unwrap();

        let last_group_ptr = Arc::as_ptr(&last_group.context);

        let commits = testers
            .clone()
            .map(|(_, g)| g)
            .filter(|g| Arc::as_ptr(&g.context) != last_group_ptr)
            .map(|g| async {
                g.load_mls_group_with_lock_async(async |mut mls_group| {
                    let provider = g.context.mls_provider();
                    let x = mls_group
                        .process_message(&provider, commit_message.clone())
                        .unwrap();
                    let ProcessedMessageContent::StagedCommitMessage(staged_commit) =
                        x.into_content()
                    else {
                        panic!("Expected StagedCommitMessage");
                    };
                    mls_group
                        .merge_staged_commit(&provider, *staged_commit)
                        .unwrap();
                    tracing::info!(epoch = %mls_group.epoch(), "Merged leaf update commit");
                    Ok::<_, GroupError>(())
                })
                .await?;
                g.sync().await.unwrap();
                Ok::<_, GroupError>(())
            });
        let results = futures::future::join_all(commits).await;
        for result in results {
            result.unwrap();
        }
    }
}

#[xmtp_common::test]
async fn test_commit_sizes_with_proposals() {
    let mut rng = rand::thread_rng();
    let mut fill_leaf_node_counter = 0;
    crate::tester!(alix);

    let new_group = alix.create_group(None, None).unwrap();

    let mut testers = vec![(alix, new_group)];

    for i in 0..ITERATION_COUNT {
        tracing::warn!("TESTER {i}");
        sync_groups(testers.iter()).await;

        let mut new_testers = vec![];
        for _ in 0..(TESTERS_PER_ITERATION.sample_single(&mut rng)) {
            crate::tester!(tester);
            new_testers.push(tester);
        }

        let mut installations = vec![];
        for tester in &new_testers {
            for _ in 0..(INSTALLATIONS_PER_TESTER.sample_single(&mut rng)) {
                installations.push(tester.new_installation().await);
            }
        }
        let new_testers = new_testers
            .into_iter()
            .chain(installations)
            .collect::<Vec<_>>();

        let key_packages = new_testers
            .iter()
            .map(|t| {
                t.identity()
                    .new_key_package(&t.context.mls_provider(), true)
                    .unwrap()
            })
            .collect::<Vec<_>>();
        // remove one every 3rd iteration.
        let leaf_nodes_to_remove = if testers.len() > 4 {
            vec![LeafNodeIndex::new(testers.len() as u32 / 2)]
        } else {
            vec![]
        };
        let proposals_and_commit =
            generate_proposals_and_commit(&testers, &key_packages, &leaf_nodes_to_remove).await;

        testers.retain(|(_, group)| group.is_active().unwrap_or(false));
        if testers.is_empty() {
            panic!("No testers left");
        }

        let welcome_size = proposals_and_commit
            .welcome
            .as_ref()
            .map(|w| w.tls_serialize_detached().unwrap().len())
            .unwrap_or(0);
        let proposals_size = proposals_and_commit
            .proposals
            .iter()
            .map(|p| p.tls_serialize_detached().unwrap().len())
            .sum::<usize>();

        let commit_size = proposals_and_commit
            .commit
            .tls_serialize_detached()
            .unwrap()
            .len();

        tracing::warn!(
            i,
            proposals_size,
            adds = proposals_and_commit.adds,
            removes = proposals_and_commit.removes,
            add_avg_size = proposals_and_commit.add_avg_size,
            remove_avg_size = proposals_and_commit.remove_avg_size,
            add_total_size = proposals_and_commit.add_total_size,
            remove_total_size = proposals_and_commit.remove_total_size,
            old_commit_size = proposals_and_commit.old_commit_len,
            old_welcome_size = proposals_and_commit.old_welcome_len,
            old_commit_builder_size = proposals_and_commit.old_commit_builder_len,
            old_welcome_builder_size = proposals_and_commit.old_welcome_builder_len,
            commit_size = commit_size,
            welcome_size,
            welcome_diff = (proposals_and_commit.old_welcome_len as isize - welcome_size as isize),
            commit_diff = (proposals_and_commit.old_commit_len as isize
                - commit_size as isize
                - proposals_size as isize),
            commit_pct = (commit_size as f64 + proposals_size as f64)
                / (proposals_and_commit.old_commit_len as f64)
                * 100.0,
            "Commit sizes"
        );
        println!(
            "CSV: {},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
            i,
            TESTERS_PER_ITERATION.end(),
            INSTALLATIONS_PER_TESTER.end(),
            PCT_FILL_LEAF_NODES,
            proposals_size,
            testers.len(),
            proposals_and_commit.adds,
            proposals_and_commit.removes,
            proposals_and_commit.add_avg_size,
            proposals_and_commit.remove_avg_size,
            proposals_and_commit.add_total_size,
            proposals_and_commit.remove_total_size,
            proposals_and_commit.old_commit_len,
            proposals_and_commit.old_welcome_len,
            proposals_and_commit.old_commit_builder_len,
            proposals_and_commit.old_welcome_builder_len,
            commit_size,
            welcome_size,
        );
        if let Some(welcome) = &proposals_and_commit.welcome {
            let welcome_bytes = welcome.tls_serialize_detached().unwrap();
            // Process welcome directly into OpenMLS group, bypassing network calls and validation
            // MlsMessageOut when serialized contains the Welcome message
            // We need to deserialize as MlsMessageIn first, then extract the Welcome
            let mls_message_in = MlsMessageIn::tls_deserialize_exact(&welcome_bytes).unwrap();

            // Extract Welcome from MlsMessageIn
            // MlsMessageIn contains the Welcome in its body
            let MlsMessageBodyIn::Welcome(openmls_welcome) = mls_message_in.extract() else {
                panic!("Expected Welcome message, got different message type");
            };

            let new_groups = welcome_testers(&new_testers, &openmls_welcome).await;
            assert_eq!(new_groups.len(), new_testers.len());
            let tester_start = testers.len();
            for (tester, group) in new_testers.into_iter().zip(new_groups) {
                testers.push((tester, group));
            }
            sync_groups(testers.iter()).await;
            for (i, tester) in testers[tester_start..].iter().enumerate() {
                fill_leaf_node_counter += 1;
                if fill_leaf_node_counter % FILL_LEAF_NODE_EVERY == 0 {
                    fill_leaf_nodes([tester].into_iter(), testers.iter()).await;
                }
                let message_bytes = format!("Hello from new tester! {}", tester_start + i);

                tester
                    .1
                    .send_message(message_bytes.as_bytes(), SendMessageOpts::default())
                    .await
                    .unwrap();
            }
        }
        sync_groups(testers.iter()).await;

        let group0 = &testers.first().unwrap().1;
        let last_group = &testers.last().unwrap().1;
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
            tracing::info!(
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

        tracing::warn!("Group 0 tree:\n{}", group0_tree);
    }
    fill_leaf_nodes([testers.first().unwrap()].into_iter(), testers.iter()).await;
    let group0_tree = testers
        .first()
        .unwrap()
        .1
        .load_mls_group_with_lock_async(async |mls_group| {
            Ok::<_, GroupError>(mls_group.export_ratchet_tree())
        })
        .await
        .unwrap();
    let group0_tree_str = format!("{}", group0_tree);
    tracing::warn!("Group 0 tree:\n{}", group0_tree_str);
    // dbg!(&group0_tree);
}
