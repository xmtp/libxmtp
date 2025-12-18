use crate::context::XmtpSharedContext;
use crate::groups::GroupError;
use crate::groups::mls_sync::generate_commit_with_rollback;
use crate::utils::fixtures::{alix, bola};
use openmls::prelude::hash_ref::HashReference;
use openmls::prelude::tls_codec::Serialize;

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
            .load_mls_group_with_lock_async(|mut mls_group| async move {
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
            .load_mls_group_with_lock_async(|mut mls_group| async move {
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

                        total_proposal_size +=
                            proposal_msg.tls_serialize_detached().unwrap().len();
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

                let commit_size = commit.tls_serialize_detached().unwrap().len();
                let welcome_size = welcome
                    .as_ref()
                    .map(|w| w.tls_serialize_detached().unwrap().len())
                    .unwrap_or(0);

                tracing::info!(
                    "Commit size: {} bytes, Welcome size: {} bytes",
                    commit_size,
                    welcome_size
                );

                Ok::<_, GroupError>((commit_size, welcome_size))
            })
            .unwrap()
    };

    tracing::info!(
        "Total bytes sent: proposal={} + commit={} = {} bytes",
        proposal_bytes.len(),
        commit_bytes.0,
        proposal_bytes.len() + commit_bytes.0
    );
}
