//! POC test demonstrating MLS proposal send/receive through XMTP infrastructure.

#![allow(clippy::unwrap_used)]

#[cfg(target_arch = "wasm32")]
wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

/// Test: Send and receive proposal
#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test]
async fn test_proposals() {
    use crate::builder::ClientBuilder;
    use crate::context::XmtpSharedContext;
    use openmls::prelude::tls_codec::Serialize;
    use xmtp_configuration::CREATE_PQ_KEY_PACKAGE_EXTENSION;
    use xmtp_cryptography::utils::generate_local_wallet;

    let alix = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let bo = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let charlie = ClientBuilder::new_test_client(&generate_local_wallet()).await;

    // Alix creates a group and adds Bo
    let alix_group = alix.create_group(None, None).unwrap();
    alix_group
        .add_members_by_inbox_id(&[bo.inbox_id()])
        .await
        .unwrap();

    // Bo syncs to get the group
    bo.sync_welcomes().await.unwrap();
    let bo_groups = bo.find_groups(Default::default()).unwrap();
    let bo_group = &bo_groups[0];

    // Get Charlie's key package for the proposal
    let charlie_provider = charlie.context.mls_provider();
    let charlie_kp = charlie
        .identity()
        .new_key_package(&charlie_provider, CREATE_PQ_KEY_PACKAGE_EXTENSION)
        .unwrap();

    // === SENDING THE PROPOSAL ===
    // Bo creates a proposal to add Charlie
    let proposal_bytes = bo_group
        .load_mls_group_with_lock(bo.context.mls_storage(), |mut mls_group| {
            let (proposal_msg, proposal_ref) = mls_group
                .propose_add_member(
                    &bo.context.mls_provider(),
                    &bo.identity().installation_keys,
                    &charlie_kp.key_package,
                )
                .unwrap();
            tracing::info!("Bo created Add proposal with ref: {:?}", proposal_ref);
            Ok(proposal_msg.tls_serialize_detached().unwrap())
        })
        .unwrap();

    // Send the proposal through XMTP network
    let messages = bo_group
        .prepare_group_messages(vec![(&proposal_bytes, false)])
        .unwrap();
    bo.context
        .api()
        .send_group_messages(messages)
        .await
        .unwrap();
    tracing::info!(
        "Bo sent proposal through XMTP network ({} bytes)",
        proposal_bytes.len()
    );

    // === RECEIVING THE PROPOSAL ===
    // Sync Alix - this will pull and process the proposal message
    alix_group.sync().await.unwrap();
    tracing::info!("Alix synced - proposal was received and stored");

    // Verify the proposal was received and stored
    let pending_count = alix_group
        .load_mls_group_with_lock(alix.context.mls_storage(), |mls_group| {
            Ok(mls_group.pending_proposals().count())
        })
        .unwrap();

    assert_eq!(pending_count, 1, "Alix should have 1 pending proposal");
    tracing::info!("Alix has {} pending proposals", pending_count);
}
