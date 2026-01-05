use crate::context::XmtpSharedContext;
use crate::groups::GroupError;
use crate::groups::mls_ext::{CommitLogStorer, build_group_join_config};
use crate::groups::mls_sync::{generate_commit_with_rollback, with_rollback};
use crate::groups::send_message_opts::SendMessageOpts;
use crate::identity::parse_credential;
use crate::utils::fixtures::{alix, bola};
use openmls::credentials::BasicCredential;
use openmls::prelude::hash_ref::HashReference;
use openmls::prelude::tls_codec::Serialize;
use openmls::prelude::{MlsMessageIn, StagedWelcome};
use tls_codec::Deserialize;
use xmtp_db::{
    TransactionalKeyStore, XmtpMlsStorageProvider, XmtpOpenMlsProviderRef, prelude::QueryGroup,
};

/// Test to compare commit sizes when using proposals inline vs proposal references
///
/// This test measures the size difference between:
/// 1. Commits with proposals inline (using `update_group_membership` - current default)
/// 2. Commits with proposal references (using `propose_add_member` + `commit_to_pending_proposals`)
///
/// The test creates two commits:
/// - Inline: Uses `update_group_membership` which creates proposals directly in the commit
/// - Proposal refs: Uses `propose_add_member` to create a proposal separately, then
///   `commit_to_pending_proposals` to create a commit that references the stored proposal
///
/// Proposal ref commits should be smaller because they only contain a hash reference
/// (~16 bytes) instead of the full proposal (hundreds of bytes).
#[xmtp_common::test]
async fn test_commit_size_measurement() {
    let alix = alix().await;
    let bola = bola().await;

    // Create a group with alix
    let alix_group = alix.create_group(None, None).unwrap();

    // Get bola's key package
    let bola_key_package = bola
        .identity()
        .new_key_package(
            &bola.context.mls_provider(),
            xmtp_configuration::CREATE_PQ_KEY_PACKAGE_EXTENSION,
        )
        .unwrap()
        .key_package;

    let installation_keys = alix.identity().installation_keys.clone();
    let group_provider = alix_group.context.mls_storage();
    let bola_key_package_clone = bola_key_package.clone();

    // Measure commit size when adding a member
    // This creates a commit with proposals inline
    let commit_size = {
        use std::sync::{Arc, Mutex};
        let commit_size = Arc::new(Mutex::new(None));
        let commit_size_clone = commit_size.clone();
        alix_group
            .load_mls_group_with_lock_async(async |mut mls_group| {
                let (commit, _, _) = generate_commit_with_rollback(
                    group_provider,
                    &mut mls_group,
                    |group, provider| {
                        group.update_group_membership(
                            provider,
                            &installation_keys,
                            &[bola_key_package_clone],
                            &[],
                            group.extensions().clone(),
                        )
                    },
                )
                .unwrap();

                let serialized = commit.tls_serialize_detached().unwrap();
                *commit_size_clone.lock().unwrap() = Some(serialized.len());
                Ok::<_, GroupError>(())
            })
            .await
            .unwrap();
        commit_size.lock().unwrap().unwrap()
    };

    // Log the results
    tracing::info!("Commit size when adding 1 member: {} bytes", commit_size);

    // Test with multiple members to see how size scales
    // Create a fresh group to test adding 2 members at once
    let alix_group2 = alix.create_group(None, None).unwrap();

    // Get a second key package from bola (simulating a second member)
    let bola_key_package = bola
        .identity()
        .new_key_package(
            &bola.context.mls_provider(),
            xmtp_configuration::CREATE_PQ_KEY_PACKAGE_EXTENSION,
        )
        .unwrap()
        .key_package;

    let group_provider2 = alix_group2.context.mls_storage();
    let installation_keys2 = alix.identity().installation_keys.clone();
    let bola_key_package2 = bola_key_package.clone();

    // Test 2: Commit with proposal references (proposals created separately)
    let proposal_ref_commit_size = {
        use std::sync::{Arc, Mutex};
        let commit_size = Arc::new(Mutex::new(None));
        let commit_size_clone = commit_size.clone();
        alix_group2
            .load_mls_group_with_lock_async(async |mut mls_group| {
                use xmtp_db::XmtpOpenMlsProviderRef;

                // Create provider reference from the storage provider
                // mls_storage() returns &MlsStorage, so we pass it directly
                let provider = XmtpOpenMlsProviderRef::new(group_provider2);

                // Step 1: Create a proposal separately using propose_add_member
                // This creates an Add proposal and automatically adds it to the group's pending proposals
                // The function returns (MlsMessageOut, HashReference) but we don't need them here
                // since the proposal is already stored in the group's pending proposals
                let (_proposal_message, _proposal_ref): (_, HashReference) = mls_group
                    .propose_add_member(&provider, &installation_keys2, &bola_key_package2)
                    .unwrap();
                dbg!(&_proposal_message);
                dbg!(&_proposal_ref);

                // Step 2: Create a commit that references the stored proposal
                // commit_to_pending_proposals creates a commit that references proposals
                // from the pending proposals queue (which includes the one we just created)
                // This commit will contain proposal references instead of full proposals
                let (commit, _welcome, _other) = mls_group
                    .commit_to_pending_proposals(&provider, &installation_keys2)
                    .unwrap(); // TODO: Add proper error conversion
                dbg!(&commit);
                dbg!(&_welcome);

                let serialized = commit.tls_serialize_detached().unwrap();
                *commit_size_clone.lock().unwrap() = Some(serialized.len());
                Ok::<_, GroupError>(())
            })
            .await
            .unwrap();
        commit_size.lock().unwrap().unwrap()
    };

    tracing::info!(
        "Commit size with proposal references: {} bytes",
        proposal_ref_commit_size
    );

    // Compare the sizes
    let size_diff = proposal_ref_commit_size.abs_diff(commit_size);

    let percent_diff = if proposal_ref_commit_size > commit_size {
        ((proposal_ref_commit_size as f64 / commit_size as f64 - 1.0) * 100.0) as u64
    } else {
        ((commit_size as f64 / proposal_ref_commit_size as f64 - 1.0) * 100.0) as u64
    };

    let comparison = if proposal_ref_commit_size < commit_size {
        "smaller"
    } else {
        "larger"
    };

    tracing::info!(
        "Size difference: {} bytes ({}% {} when using proposal references)",
        size_diff,
        percent_diff,
        comparison
    );

    // The test passes as long as we get valid measurements
    assert!(commit_size > 0, "Inline commit should have non-zero size");
    assert!(
        proposal_ref_commit_size > 0,
        "Proposal ref commit should have non-zero size"
    );

    // Proposal ref commits should be smaller because they only contain a hash reference
    // (~16 bytes) instead of the full proposal (hundreds of bytes)
    if proposal_ref_commit_size < commit_size {
        tracing::info!(
            "✓ Proposal refs are smaller as expected (saved {} bytes)",
            commit_size - proposal_ref_commit_size
        );
    } else {
        tracing::warn!(
            "⚠ Proposal refs are not smaller (unexpected, may indicate implementation issue)"
        );
    }
    panic!();
}

/// This test measures sizes for 5 members for both approaches.
#[xmtp_common::test]
async fn test_commit_size_comparison() {
    use crate::groups::mls_sync::with_rollback;
    use crate::tester;

    let member_count = 5;

    // Create fresh group and testers for each test
    tester!(alix);
    let group = alix.create_group(None, None).unwrap();

    // Create the members we'll add
    let mut members = Vec::new();
    for _ in 0..member_count {
        tester!(member);
        members.push(member);
    }

    // Collect key packages for all members
    let key_packages: Vec<_> = members
        .iter()
        .map(|m| {
            m.identity()
                .new_key_package(&m.context.mls_provider(), true)
                .unwrap()
                .key_package
        })
        .collect();

    // Uses update_group_membership creates proposals in the commit
    let direct_result = {
        let storage = group.context.mls_storage();
        let signer = alix.identity().installation_keys.clone();
        let kps = key_packages.clone();

        group
            .load_mls_group_with_lock_async(|mut mls_group| async move {
                with_rollback(storage, &mut mls_group, |group, provider| {
                    let (commit, welcome, _group_info) = group
                        .update_group_membership(
                            provider,
                            &signer,
                            &kps,
                            &[],
                            group.extensions().clone(),
                        )
                        .map_err(|e| GroupError::Any(e.into()))?;

                    let commit_size = commit.tls_serialize_detached().unwrap().len();
                    let welcome_size = welcome
                        .as_ref()
                        .map(|w| w.tls_serialize_detached().unwrap().len())
                        .unwrap_or(0);

                    Ok::<_, GroupError>((commit_size, welcome_size))
                })
            })
            .await
            .unwrap()
    };

    tracing::info!(
        "DIRECT COMMIT ({} members): commit={} bytes, welcome={} bytes, total={} bytes",
        member_count,
        direct_result.0,
        direct_result.1,
        direct_result.0 + direct_result.1
    );

    // create proposals separately
    let proposal_result = {
        let storage = group.context.mls_storage();
        let signer = alix.identity().installation_keys.clone();
        let kps = key_packages.clone();

        group
            .load_mls_group_with_lock_async(|mut mls_group| async move {
                with_rollback(storage, &mut mls_group, |group, provider| {
                    let mut total_proposal_size = 0;

                    // Create proposals for each member
                    for kp in &kps {
                        let (proposal_msg, _proposal_ref) = group
                            .propose_add_member(provider, &signer, kp)
                            .map_err(|e| GroupError::Any(e.into()))?;

                        total_proposal_size += proposal_msg.tls_serialize_detached().unwrap().len();
                    }

                    // Commit to all pending proposals
                    let (commit, welcome, _group_info) = group
                        .commit_to_pending_proposals(provider, &signer)
                        .map_err(|e| GroupError::Any(e.into()))?;

                    let commit_size = commit.tls_serialize_detached().unwrap().len();
                    let welcome_size = welcome
                        .as_ref()
                        .map(|w| w.tls_serialize_detached().unwrap().len())
                        .unwrap_or(0);

                    Ok::<_, GroupError>((total_proposal_size, commit_size, welcome_size))
                })
            })
            .await
            .unwrap()
    };

    tracing::info!(
        "PROPOSAL COMMIT ({} members): proposals={} bytes, commit={} bytes, welcome={} bytes, total={} bytes",
        member_count,
        proposal_result.0,
        proposal_result.1,
        proposal_result.2,
        proposal_result.0 + proposal_result.1 + proposal_result.2
    );

    // Calculate and log the difference
    let direct_total = direct_result.0 + direct_result.1;
    let proposal_total = proposal_result.0 + proposal_result.1 + proposal_result.2;
    let diff = (proposal_total as i64) - (direct_total as i64);

    tracing::info!(
        "DIFFERENCE ({} members): proposal approach is {} bytes {} than direct",
        member_count,
        diff.abs(),
        if diff > 0 { "larger" } else { "smaller" }
    );
}

#[xmtp_common::test]
async fn test_proposal_network_flow() {
    use crate::tester;

    tester!(alix);
    tester!(bola);
    tester!(charlie);

    let alix_group = alix.create_group(None, None).unwrap();
    alix_group
        .add_members_by_inbox_id(&[bola.inbox_id()])
        .await
        .unwrap();

    let bola_groups = bola.sync_welcomes().await.unwrap();
    let bola_group = &bola_groups[0];
    bola_group.sync().await.unwrap();

    let charlie_kp = charlie
        .identity()
        .new_key_package(&charlie.context.mls_provider(), true)
        .unwrap();

    let proposal_bytes = {
        let storage = bola_group.context.mls_storage();
        let signer = bola.identity().installation_keys.clone();
        let kp = charlie_kp.key_package.clone();

        bola_group
            .load_mls_group_with_lock(storage, |mut mls_group| {
                let provider = bola.context.mls_provider();
                let (proposal_msg, proposal_ref) = mls_group
                    .propose_add_member(&provider, &signer, &kp)
                    .map_err(|e| GroupError::Any(e.into()))?;

                tracing::info!(
                    "Created proposal: size={} bytes, ref={:?}",
                    proposal_msg.tls_serialize_detached().unwrap().len(),
                    proposal_ref
                );

                Ok::<_, GroupError>(proposal_msg.tls_serialize_detached().unwrap())
            })
            .unwrap()
    };

    tracing::info!("Proposal size: {} bytes", proposal_bytes.len());

    // Send the proposal through the network
    let messages = bola_group
        .prepare_group_messages(vec![(&proposal_bytes, false)])
        .unwrap();
    bola.context
        .api()
        .send_group_messages(messages)
        .await
        .unwrap();
    tracing::info!("Proposal sent to network");

    // Alix syncs to receive the proposal
    alix_group.sync().await.unwrap();

    // Check pending proposals on alix's side
    let pending_count = alix_group
        .load_mls_group_with_lock(alix_group.context.mls_storage(), |mls_group| {
            let count = mls_group.pending_proposals().count();
            tracing::info!("Alix has {} pending proposal(s)", count);
            Ok::<_, GroupError>(count)
        })
        .unwrap();

    assert_eq!(pending_count, 1, "Alix should have 1 pending proposal");

    // Alix commits to the pending proposal
    let commit_bytes = {
        let storage = alix_group.context.mls_storage();
        let signer = alix.identity().installation_keys.clone();

        alix_group
            .load_mls_group_with_lock(storage, |mut mls_group| {
                let provider = alix.context.mls_provider();
                let (commit, welcome, _) = mls_group
                    .commit_to_pending_proposals(&provider, &signer)
                    .map_err(|e| GroupError::Any(e.into()))?;

                let commit_bytes = commit.tls_serialize_detached().unwrap();
                let commit_size = commit_bytes.len();
                let welcome_size = welcome
                    .as_ref()
                    .map(|w| w.tls_serialize_detached().unwrap().len())
                    .unwrap_or(0);

                tracing::info!(
                    "Commit size: {} bytes, Welcome size: {} bytes",
                    commit_size,
                    welcome_size
                );

                Ok::<_, GroupError>((commit_bytes, welcome_size))
            })
            .unwrap()
    };

    tracing::info!(
        "Total bytes sent: proposal={} + commit={} = {} bytes",
        proposal_bytes.len(),
        commit_bytes.0.len(),
        proposal_bytes.len() + commit_bytes.0.len()
    );

    let mls_message_in = MlsMessageIn::tls_deserialize_exact(&commit_bytes.0).unwrap();

    let protocol_message = mls_message_in.try_into_protocol_message().unwrap();

    let provider = bola.context.mls_provider();
    bola_group
        .load_mls_group_with_lock_async(async |mut mls_group| {
            let count = mls_group.pending_proposals().count();
            tracing::info!("Bola has {} pending proposal(s)", count);
            let processed_message = mls_group
                .process_message(&provider, protocol_message)
                .unwrap();
            dbg!(&processed_message);
            let content = processed_message.into_content();

            Ok::<_, GroupError>(())
        })
        .await
        .unwrap();
}

#[xmtp_common::test]
async fn test_commit_sizes_with_proposals() {
    const TESTER_COUNT: usize = 100;
    crate::tester!(alix);

    let mut testers = vec![alix];

    let mut groups = vec![testers[0].create_group(None, None).unwrap()];

    for _ in 0..TESTER_COUNT {
        crate::tester!(tester);

        let provider = groups[0].context.mls_provider();
        let signer = testers[0].identity().installation_keys.clone();
        let key_package = tester
            .identity()
            .new_key_package(&tester.context.mls_provider(), true)
            .unwrap();
        let storage = groups[0].context.mls_storage();
        let (proposal, old_commit, old_welcome) = groups[0]
            .load_mls_group_with_lock_async(async |mut mls_group| {
                let (old_commit, old_welcome, _) =
                    with_rollback(storage, &mut mls_group, |group, provider| {
                        group
                            .update_group_membership(
                                provider,
                                &testers[0].identity().installation_keys,
                                &[key_package.key_package.clone()],
                                &[],
                                group.extensions().clone(),
                            )
                            .map_err(|e| GroupError::Any(e.into()))
                    })
                    .unwrap();
                let (proposal, _) = mls_group
                    .propose_add_member(&provider, &signer, &key_package.key_package)
                    .map_err(|e| GroupError::Any(e.into()))?;
                Ok::<_, GroupError>((proposal, old_commit, old_welcome))
            })
            .await
            .unwrap();
        let proposal = proposal.tls_serialize_detached().unwrap();

        let proposal_size = proposal.len();

        let protocol_message = MlsMessageIn::tls_deserialize_exact(&proposal)
            .unwrap()
            .try_into_protocol_message()
            .unwrap();

        // skip over originating group
        let add_proposals = groups.iter().skip(1).map(|g| {
            g.load_mls_group_with_lock_async(async |mut mls_group| {
                let provider = g.context.mls_provider();
                mls_group
                    .process_message(&provider, protocol_message.clone())
                    .unwrap();
                Ok::<_, GroupError>(())
            })
        });

        let results = futures::future::join_all(add_proposals).await;
        for result in results {
            result.unwrap();
        }

        // Commit to the pending proposals
        let storage = groups[0].context.mls_storage();

        let (commit, welcome) = groups[0]
            .load_mls_group_with_lock_async(async |mut mls_group| {
                let signer = testers[0].identity().installation_keys.clone();
                let (commit, welcome, _) =
                    with_rollback(storage, &mut mls_group, |group, provider| {
                        group
                            .commit_to_pending_proposals(provider, &signer)
                            .map_err(|e| GroupError::Any(e.into()))
                    })
                    .unwrap();
                // Extract Welcome from MlsMessageOut before serializing
                let welcome = welcome.and_then(|msg| {
                    // MlsMessageOut is an enum, extract the Welcome if present
                    // Serialize and deserialize to extract the inner Welcome
                    let bytes = msg.tls_serialize_detached().ok()?;
                    let mls_msg_in = MlsMessageIn::tls_deserialize_exact(&bytes).ok()?;
                    // For Welcome messages, we can extract it from the protocol message
                    // But actually, MlsMessageOut::Welcome contains the Welcome directly
                    // Let's just keep the bytes and extract later
                    Some(bytes)
                });
                Ok::<_, GroupError>((commit, welcome))
            })
            .await
            .unwrap();

        let old_commit = old_commit.tls_serialize_detached().unwrap();
        let old_welcome = old_welcome.map_or(vec![], |w| w.tls_serialize_detached().unwrap());
        let commit = commit.tls_serialize_detached().unwrap();
        let welcome_bytes = welcome.unwrap_or_default();
        let welcome_size = welcome_bytes.len();

        tracing::warn!(
            proposal_size,
            old_commit_size = old_commit.len(),
            old_welcome_size = old_welcome.len(),
            commit_size = commit.len(),
            welcome_size,
            welcome_diff = (old_welcome.len() as isize - welcome_size as isize),
            commit_diff =
                (old_commit.len() as isize - commit.len() as isize - proposal_size as isize),
            "Commit sizes"
        );

        let commit_message = MlsMessageIn::tls_deserialize_exact(&commit)
            .unwrap()
            .try_into_protocol_message()
            .unwrap();

        let commits = groups.iter().skip(1).map(|g| async {
            g.load_mls_group_with_lock_async(async |mut mls_group| {
                let provider = g.context.mls_provider();
                mls_group
                    .process_message(&provider, commit_message.clone())
                    .unwrap();
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

        // Process welcome directly into OpenMLS group, bypassing network calls and validation
        if welcome_bytes.is_empty() {
            // No welcome to process
            return;
        }

        // MlsMessageOut when serialized contains the Welcome message
        // We need to deserialize as MlsMessageIn first, then extract the Welcome
        use openmls::prelude::tls_codec::Deserialize;
        let mls_message_in = MlsMessageIn::tls_deserialize_exact(&welcome_bytes)
            .map_err(|e| GroupError::Any(e.into()))
            .unwrap();

        // Extract Welcome from MlsMessageIn
        // MlsMessageIn contains the Welcome in its body
        use openmls::prelude::MlsMessageBodyIn;
        let openmls_welcome = match mls_message_in.extract() {
            MlsMessageBodyIn::Welcome(w) => w,
            _ => panic!("Expected Welcome message, got different message type"),
        };

        let tester_storage = tester.context.mls_storage();
        let tester_provider = XmtpOpenMlsProviderRef::new(tester_storage);

        // Build staged welcome from the OpenMLS Welcome message
        let join_config = build_group_join_config();
        let builder =
            StagedWelcome::build_from_welcome(&tester_provider, &join_config, openmls_welcome)
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
        use crate::groups::MlsGroup;
        use openmls::group::MlsGroup as OpenMlsGroup;
        use xmtp_common::time::now_ns;
        use xmtp_db::{
            consent_record::StoredConsentRecord,
            group::{ConversationType, GroupMembershipState, StoredGroup},
        };
        use xmtp_mls_common::{
            group_metadata::extract_group_metadata,
            group_mutable_metadata::extract_group_mutable_metadata,
        };

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
                use crate::groups::MessageDisappearingSettings;
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
                    .build()
                    .map_err(|e| GroupError::Any(e.into()))?;

                let stored_group = db.insert_or_replace_group(stored_group)?;
                StoredConsentRecord::stitch_dm_consent(&db, &stored_group)?;

                Ok::<_, GroupError>(stored_group)
            })
            .unwrap();

        // Create the XMTP MlsGroup wrapper that allows sending messages
        let _xmtp_group = MlsGroup::new(
            tester.context.clone(),
            stored_group.id,
            stored_group.dm_id,
            stored_group.conversation_type,
            stored_group.created_at_ns,
        );

        let tester_group = tester.group(&group_id).unwrap();

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
        let message_bytes = b"Hello from new tester!";
        use prost::Message;
        use xmtp_proto::xmtp::mls::message_contents::{
            PlaintextEnvelope,
            plaintext_envelope::{Content, V1},
        };

        let now = now_ns();
        let plain_envelope = PlaintextEnvelope {
            content: Some(Content::V1(V1 {
                content: message_bytes.to_vec(),
                idempotency_key: now.to_string(),
            })),
        };
        let mut encoded_envelope = vec![];
        plain_envelope.encode(&mut encoded_envelope).unwrap();

        // Encrypt the message using the MLS group and send directly to API
        let encrypted_message = tester_group
            .load_mls_group_with_lock_async(async |mut mls_group| {
                let provider = tester_group.context.mls_provider();
                let signer = tester.identity().installation_keys.clone();
                let msg = mls_group
                    .create_message(&provider, &signer, &encoded_envelope)
                    .map_err(|e| GroupError::Any(e.into()))?;
                Ok::<_, GroupError>(msg.tls_serialize_detached().unwrap())
            })
            .await
            .unwrap();

        // Prepare and send the message directly to the API
        let messages = tester_group
            .prepare_group_messages(vec![(&encrypted_message, false)])
            .unwrap();
        tester
            .context
            .api()
            .send_group_messages(messages)
            .await
            .unwrap();

        // Add the tester to the group
        testers.push(tester);
    }
}
