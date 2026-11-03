//! Identity update loads, publishes and revocations, through clients.

use xmtp_mls::identity_updates::*;

mod tests {
    #![allow(unused)] // b/c wasm & native
    use super::{
        batch_get_association_state_with_verifier, is_member_of_association_state,
        load_identity_updates,
    };
    use alloy::primitives::{BlockNumber, Bytes};
    use alloy::signers::Signer;
    use prost::Message;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use xmtp_common::rand_vec;
    use xmtp_cryptography::utils::generate_local_wallet;
    use xmtp_db::{
        ConnectionExt, db_connection::DbConnection, identity_update::StoredIdentityUpdate,
        prelude::*,
    };
    use xmtp_id::{
        InboxOwner,
        associations::{
            AssociationState, MemberIdentifier,
            builder::{SignatureRequest, SignatureRequestError},
            test_utils::{MockSmartContractSignatureVerifier, WalletTestExt, add_wallet_signature},
            unverified::UnverifiedSignature,
        },
    };
    use xmtp_id::{
        associations::{
            AccountId, Identifier,
            unsigned_actions::UnsignedCreateInbox,
            unverified::{UnverifiedAction, UnverifiedCreateInbox, UnverifiedIdentityUpdate},
        },
        scw_verifier::{
            ScwInput, SmartContractSignatureVerifier, ValidationResponse, VerifierError,
        },
    };
    use xmtp_mls::prelude::*;
    use xmtp_mls::utils::test::identity::get_association_state;
    use xmtp_mls::{
        Client, XmtpApi,
        builder::ClientBuilder,
        context::XmtpSharedContext,
        groups::group_membership::GroupMembership,
        identity_updates::{IdentityUpdateError, IdentityUpdates},
        tester,
        utils::{FullXmtpClient, Tester},
    };
    use xmtp_proto::types::GroupId;
    use xmtp_proto::xmtp::identity::associations::IdentityUpdate as IdentityUpdateProto;

    #[rstest::rstest]
    #[xmtp_common::test]
    async fn test_is_member_of_association_state() {
        let wallet = generate_local_wallet();
        let client = ClientBuilder::new_test_client(&wallet).await;

        let wallet2 = generate_local_wallet();
        let client_identity_updates = IdentityUpdates::new(&client.context);

        let mut request = client_identity_updates
            .associate_identity(wallet2.identifier())
            .await
            .unwrap();
        add_wallet_signature(&mut request, &wallet2).await;
        client_identity_updates
            .apply_signature_request(request)
            .await
            .unwrap();

        let conn = client.context.db();

        // The installation, wallet1 address, and the newly associated wallet2 address
        // Use wait_for_eq to handle eventual consistency after apply_signature_request
        xmtp_common::wait_for_eq(
            || async {
                client_identity_updates
                    .get_latest_association_state(&conn, client.inbox_id())
                    .await
                    .unwrap()
                    .members()
                    .len()
            },
            3,
        )
        .await
        .unwrap();

        let api_client = client.context.api();

        // Check that the second wallet is associated with our new static helper
        let is_member = is_member_of_association_state(
            api_client,
            client.inbox_id(),
            &wallet2.member_identifier(),
            None,
        )
        .await
        .unwrap();

        assert!(is_member);
    }

    #[rstest::rstest]
    #[xmtp_common::test]
    async fn create_inbox_round_trip() {
        let wallet = generate_local_wallet();
        let wallet_ident = wallet.identifier();
        let client = ClientBuilder::new_test_client(&wallet).await;

        let mut signature_request: SignatureRequest = client
            .identity_updates()
            .create_inbox(wallet_ident.clone(), None)
            .await
            .unwrap();
        let inbox_id = signature_request.inbox_id().to_string();

        add_wallet_signature(&mut signature_request, &wallet).await;

        client
            .identity_updates()
            .apply_signature_request(signature_request)
            .await
            .unwrap();

        let association_state = get_association_state(&client, &inbox_id).await;

        assert_eq!(association_state.members().len(), 2);
        assert_eq!(association_state.recovery_identifier(), &wallet_ident);
        assert!(association_state.get(&wallet_ident.into()).is_some())
    }

    #[rstest::rstest]
    #[xmtp_common::test]
    async fn add_association() {
        let wallet_2 = generate_local_wallet();
        let wallet2_ident = wallet_2.identifier();

        tester!(client);

        let mut add_association_request = client
            .identity_updates()
            .associate_identity(wallet2_ident.clone())
            .await
            .unwrap();

        add_wallet_signature(&mut add_association_request, &wallet_2).await;

        client
            .identity_updates()
            .apply_signature_request(add_association_request)
            .await
            .unwrap();
        let association_state = get_association_state(&client, client.inbox_id()).await;

        let members = association_state
            .members_by_parent(&client.builder.owner.get_identifier().unwrap().into());
        // Those members should have timestamps
        for member in members {
            assert!(member.client_timestamp_ns.is_some());
        }

        assert_eq!(association_state.members().len(), 3);
        assert_eq!(
            *association_state.recovery_identifier(),
            client.builder.owner.get_identifier().unwrap()
        );
        assert!(association_state.get(&wallet2_ident.into()).is_some());
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg(not(target_arch = "wasm32"))]
    fn cache_association_state() {
        use std::sync::Arc;

        use xmtp_common::{assert_logged, traced_test::LOG_BUFFER};

        use xmtp_mls::{
            utils::LocalTester, worker::device_sync::DeviceSyncClient,
            worker::metrics::WorkerMetrics,
        };

        xmtp_common::traced_test!(async {
            let client = Tester::new().await;
            let inbox_id = client.inbox_id();
            let metrics = WorkerMetrics::new(client.context.installation_id());
            let device_sync = DeviceSyncClient::new(&client.context, Arc::new(metrics));
            device_sync.wait_for_sync_worker_init().await;

            let wallet_2 = generate_local_wallet();

            get_association_state(&client, inbox_id).await;

            assert_logged!("Loaded association", 0);
            // TODO: Verify state is actually in db instead of just checking logs
            assert_logged!("Wrote association", 1);

            let association_state = get_association_state(&client, inbox_id).await;

            assert_eq!(association_state.members().len(), 2);
            assert_eq!(
                association_state.recovery_identifier(),
                &client.builder.owner.identifier()
            );
            assert!(
                association_state
                    .get(&client.builder.owner.identifier().into())
                    .is_some()
            );

            assert_logged!("Loaded association", 1);
            assert_logged!("Wrote association", 1);

            let mut add_association_request = client
                .identity_updates()
                .associate_identity(wallet_2.identifier())
                .await
                .unwrap();

            add_wallet_signature(&mut add_association_request, &wallet_2).await;

            client
                .identity_updates()
                .apply_signature_request(add_association_request)
                .await
                .unwrap();

            assert_logged!("Wrote association", 2);
            // Count cache reads from the following calls.
            LOG_BUFFER.with(|buf| buf.clear());
            get_association_state(&client, inbox_id).await;

            assert_logged!("Loaded association", 1);
            assert_logged!("Wrote association", 0);

            let association_state = get_association_state(&client, inbox_id).await;

            assert_logged!("Loaded association", 2);
            assert_logged!("Wrote association", 0);

            assert_eq!(association_state.members().len(), 3);
            assert_eq!(
                association_state.recovery_identifier(),
                &client.builder.owner.identifier()
            );
            assert!(
                association_state
                    .get(&wallet_2.member_identifier())
                    .is_some()
            );
        });
    }

    #[rstest::rstest]
    #[xmtp_common::test]
    async fn get_installation_diff() {
        let wallet_1 = generate_local_wallet();
        let wallet_2 = generate_local_wallet();
        let wallet_3 = generate_local_wallet();

        let client_1 = ClientBuilder::new_test_client(&wallet_1).await;
        let client_2 = ClientBuilder::new_test_client(&wallet_2).await;
        let client_3 = ClientBuilder::new_test_client(&wallet_3).await;

        let client_2_installation_key = client_2.installation_public_key().to_vec();
        let client_3_installation_key = client_3.installation_public_key().to_vec();

        let mut inbox_ids: Vec<String> = vec![];

        // Create an inbox with 2 history items for each client
        for (client, wallet) in [
            (client_1, wallet_1),
            (client_2, wallet_2),
            (client_3, wallet_3),
        ] {
            let mut signature_request: SignatureRequest = client
                .identity_updates()
                .create_inbox(wallet.identifier(), None)
                .await
                .unwrap();
            let inbox_id = signature_request.inbox_id().to_string();
            inbox_ids.push(inbox_id);

            add_wallet_signature(&mut signature_request, &wallet).await;
            client
                .identity_updates()
                .apply_signature_request(signature_request)
                .await
                .unwrap();
            let new_wallet = generate_local_wallet();
            let mut add_association_request = client
                .identity_updates()
                .associate_identity(new_wallet.identifier())
                .await
                .unwrap();

            add_wallet_signature(&mut add_association_request, &new_wallet).await;

            client
                .identity_updates()
                .apply_signature_request(add_association_request)
                .await
                .unwrap();
        }

        // Create a new client to test group operations with
        let other_client = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let other_conn = other_client.context.db();
        let ids = inbox_ids.iter().map(AsRef::as_ref).collect::<Vec<&str>>();
        // Load all the identity updates for the new inboxes
        load_identity_updates(other_client.context.api(), &other_conn, ids.as_slice())
            .await
            .expect("load should succeed");

        // Get the latest sequence IDs so we can construct the updates
        let latest_sequence_ids = other_conn.get_latest_sequence_id(ids.as_slice()).unwrap();

        let inbox_1_first_sequence_id = other_conn
            .get_identity_updates(inbox_ids[0].clone(), None, None)
            .unwrap()
            .first()
            .unwrap()
            .sequence_id;

        let mut original_group_membership = GroupMembership::new();
        original_group_membership.add(inbox_ids[0].to_string(), inbox_1_first_sequence_id as u64);
        original_group_membership.add(
            inbox_ids[1].to_string(),
            *latest_sequence_ids.get(&inbox_ids[1]).unwrap() as u64,
        );

        let mut new_group_membership = original_group_membership.clone();
        // Update the first inbox to have a higher sequence ID, but no new installations
        new_group_membership.add(
            inbox_ids[0].to_string(),
            *latest_sequence_ids.get(&inbox_ids[0]).unwrap() as u64,
        );
        new_group_membership.add(
            inbox_ids[2].to_string(),
            *latest_sequence_ids.get(&inbox_ids[2]).unwrap() as u64,
        );
        new_group_membership.remove(&inbox_ids[1]);

        let membership_diff = original_group_membership.diff(&new_group_membership);

        let installation_diff = other_client
            .identity_updates()
            .get_installation_diff(
                &other_conn,
                &GroupId::default(),
                &original_group_membership,
                &new_group_membership,
                &membership_diff,
            )
            .await
            .unwrap();

        assert_eq!(installation_diff.added_installations.len(), 1);
        assert!(
            installation_diff
                .added_installations
                .contains_key(&client_3_installation_key.to_vec()),
        );
        assert_eq!(installation_diff.removed_installations.len(), 1);
        assert!(
            installation_diff
                .removed_installations
                .contains(&client_2_installation_key.to_vec())
        );
    }

    /// The diff must reject an inbox that a commit adds at `sequence_id: 0`.
    /// It must not read the receiver's latest identity state. If it does,
    /// receivers with different identity history fork the group.
    #[rstest::rstest]
    #[xmtp_common::test]
    async fn get_installation_diff_rejects_added_inbox_at_sequence_zero() {
        let wallet = generate_local_wallet();
        let client = ClientBuilder::new_test_client(&wallet).await;

        let mut signature_request: SignatureRequest = client
            .identity_updates()
            .create_inbox(wallet.identifier(), None)
            .await
            .unwrap();
        let inbox_id = signature_request.inbox_id().to_string();
        add_wallet_signature(&mut signature_request, &wallet).await;
        client
            .identity_updates()
            .apply_signature_request(signature_request)
            .await
            .unwrap();

        // This client holds all the identity updates. A read of the latest
        // state would succeed. So the `0` causes the rejection below.
        let other_client = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let other_conn = other_client.context.db();
        load_identity_updates(
            other_client.context.api(),
            &other_conn,
            &[inbox_id.as_str()],
        )
        .await
        .expect("load should succeed");
        let latest_sequence_id = *other_conn
            .get_latest_sequence_id(&[inbox_id.as_str()])
            .unwrap()
            .get(&inbox_id)
            .unwrap();

        let old_membership = GroupMembership::new();

        // The same add at the real sequence id succeeds.
        let mut honest_membership = GroupMembership::new();
        honest_membership.add(inbox_id.clone(), latest_sequence_id as u64);
        other_client
            .identity_updates()
            .get_installation_diff(
                &other_conn,
                &GroupId::default(),
                &old_membership,
                &honest_membership,
                &old_membership.diff(&honest_membership),
            )
            .await
            .expect("an add at a real sequence id is valid");

        // Now the attack. Same inbox, same receiver, but sequence id 0.
        let mut crafted_membership = GroupMembership::new();
        crafted_membership.add(inbox_id.clone(), 0);
        let crafted_diff = old_membership.diff(&crafted_membership);
        assert_eq!(crafted_diff.added_inboxes, vec![&inbox_id]);

        let result = other_client
            .identity_updates()
            .get_installation_diff(
                &other_conn,
                &GroupId::default(),
                &old_membership,
                &crafted_membership,
                &crafted_diff,
            )
            .await;

        assert!(
            result.is_err(),
            "an inbox added at sequence_id 0 must be rejected, not resolved at latest"
        );
    }

    /// One identity update revokes at most its action limit, so a larger
    /// revocation fails before any request exists to sign, from either
    /// helper. A revocation at the limit is one request with that many actions.
    #[xmtp_common::test(unwrap_try = true)]
    async fn revoke_requests_over_action_limit_fail_before_signing() {
        use xmtp_configuration::MAX_IDENTITY_UPDATE_ACTIONS;
        use xmtp_mls::client::ClientError;
        tester!(alix);
        let installations = |count| (0..count).map(|_| rand_vec::<32>()).collect::<Vec<_>>();
        let identities = |count| {
            (0..count)
                .map(|_| generate_local_wallet().identifier())
                .collect::<Vec<_>>()
        };
        let revoke_installations = |count| {
            super::revoke_installations_with_verifier(
                &alix.identifier(),
                alix.inbox_id(),
                installations(count),
            )
        };

        let over = MAX_IDENTITY_UPDATE_ACTIONS + 1;
        for error in [
            revoke_installations(over).unwrap_err(),
            alix.identity_updates()
                .revoke_identities(identities(over))
                .await
                .unwrap_err(),
        ] {
            assert!(
                matches!(
                    error,
                    IdentityUpdateError::InvalidSignatureRequest(
                        SignatureRequestError::TooManyActions(count)
                    ) if count == over
                ),
                "{error}"
            );
        }

        for mut request in [
            revoke_installations(MAX_IDENTITY_UPDATE_ACTIONS)?,
            alix.identity_updates()
                .revoke_identities(identities(MAX_IDENTITY_UPDATE_ACTIONS))
                .await?,
        ] {
            add_wallet_signature(&mut request, &alix.builder.owner).await;
            assert_eq!(
                request.build_identity_update()?.actions.len(),
                MAX_IDENTITY_UPDATE_ACTIONS
            );
        }
    }

    #[rstest::rstest]
    #[xmtp_common::test]
    pub async fn revoke_wallet() {
        let recovery_wallet = generate_local_wallet();
        let second_wallet = generate_local_wallet();
        let client = ClientBuilder::new_test_client(&recovery_wallet).await;

        let mut add_wallet_signature_request = client
            .identity_updates()
            .associate_identity(second_wallet.identifier())
            .await
            .unwrap();

        add_wallet_signature(&mut add_wallet_signature_request, &second_wallet).await;

        client
            .identity_updates()
            .apply_signature_request(add_wallet_signature_request)
            .await
            .unwrap();

        let association_state_after_add = get_association_state(&client, client.inbox_id()).await;
        assert_eq!(association_state_after_add.identifiers().len(), 2);

        // Make sure the inbox ID is correctly registered
        let inbox_ids = client
            .context
            .api()
            .get_inbox_ids(vec![second_wallet.identifier().into()])
            .await
            .unwrap();
        assert_eq!(inbox_ids.len(), 1);

        // Now revoke the second wallet

        let mut revoke_signature_request = client
            .identity_updates()
            .revoke_identities(vec![second_wallet.identifier()])
            .await
            .unwrap();
        add_wallet_signature(&mut revoke_signature_request, &recovery_wallet).await;

        client
            .identity_updates()
            .apply_signature_request(revoke_signature_request)
            .await
            .unwrap();

        // Make sure that the association state has removed the second wallet
        let association_state_after_revoke =
            get_association_state(&client, client.inbox_id()).await;
        assert_eq!(association_state_after_revoke.identifiers().len(), 1);

        // Make sure the inbox ID is correctly unregistered
        let inbox_ids = client
            .context
            .api()
            .get_inbox_ids(vec![second_wallet.identifier().into()])
            .await
            .unwrap();
        assert_eq!(inbox_ids, vec![None]);
    }

    #[rstest::rstest]
    #[xmtp_common::test]
    pub async fn revoke_installation() {
        use xmtp_events::{ClientEvent, EventFilter, EventKind};
        let wallet = generate_local_wallet();
        let client1: FullXmtpClient = ClientBuilder::new_test_client(&wallet).await;
        let client2: FullXmtpClient = ClientBuilder::new_test_client(&wallet).await;

        let association_state = get_association_state(&client1, client1.inbox_id()).await;
        // Ensure there are two installations on the inbox
        assert_eq!(association_state.installation_ids().len(), 2);
        let events = client1.context.events().subscribe(
            EventFilter::new([EventKind::IdentityOwnInstallationRevoked]),
            Some(10),
        );

        // Now revoke the second client
        let mut revoke_installation_request = client1
            .identity_updates()
            .revoke_installations(vec![client2.installation_public_key().to_vec()])
            .await
            .unwrap();
        add_wallet_signature(&mut revoke_installation_request, &wallet).await;
        assert!(events.drain().is_empty());
        client1
            .identity_updates()
            .apply_signature_request(revoke_installation_request)
            .await
            .unwrap();

        // Make sure there is only one installation on the inbox
        let association_state = get_association_state(&client1, client1.inbox_id()).await;
        assert_eq!(association_state.installation_ids().len(), 1);
        let revoked = events.drain();
        assert_eq!(revoked.len(), 1);
        assert!(matches!(&revoked[0].client,
            Some(ClientEvent::IdentityOwnInstallationRevoked(event))
                if event.installation_key == client2.installation_public_key()
                   && !event.is_this_installation));
    }

    #[rstest::rstest]
    #[xmtp_common::test]
    pub async fn change_recovery_address() {
        let original_wallet = generate_local_wallet();
        let new_recovery_wallet = generate_local_wallet();
        let client = ClientBuilder::new_test_client(&original_wallet).await;

        // Verify initial state has the original wallet as recovery identifier
        let association_state_before = get_association_state(&client, client.inbox_id()).await;
        assert_eq!(
            association_state_before.recovery_identifier(),
            &original_wallet.identifier()
        );

        // Verify that the associated wallet at this stage includes the recovery address
        assert!(association_state_before.members().len() == 2);
        // Verify that one of the members is the recovery address
        let binding = association_state_before.members();
        let recovery_member = binding
            .iter()
            .find(|m| m.identifier == original_wallet.identifier());
        assert!(recovery_member.is_some());
        let recovery_member_timestamp = recovery_member.unwrap().client_timestamp_ns;
        // Right now we are not saving client side timestamps for recovery address, so this will be None
        assert!(recovery_member_timestamp.is_none());
        // Verify the other member is an installation key
        let installation_member = binding
            .iter()
            .find(|m| matches!(m.identifier, MemberIdentifier::Installation(_)));
        assert!(installation_member.is_some());
        assert!(
            installation_member
                .unwrap()
                .identifier
                .installation_key()
                .unwrap()
                == client.installation_public_key().to_vec()
        );
        let installation_member_timestamp = installation_member.unwrap().client_timestamp_ns;
        assert!(installation_member_timestamp.is_some());

        // Recovery moves only to a member, so associate the new recovery wallet first
        let mut associate_request = client
            .identity_updates()
            .associate_identity(new_recovery_wallet.identifier())
            .await
            .unwrap();
        add_wallet_signature(&mut associate_request, &new_recovery_wallet).await;
        client
            .identity_updates()
            .apply_signature_request(associate_request)
            .await
            .unwrap();

        // Create a signature request to change the recovery address
        let mut change_recovery_request = client
            .identity_updates()
            .change_recovery_identifier(new_recovery_wallet.identifier())
            .await
            .unwrap();

        // Add the original wallet's signature (since it's the current recovery address)
        add_wallet_signature(&mut change_recovery_request, &original_wallet).await;

        // Apply the signature request
        client
            .identity_updates()
            .apply_signature_request(change_recovery_request)
            .await
            .unwrap();

        // Verify the recovery address has been updated
        let association_state_after = get_association_state(&client, client.inbox_id()).await;
        assert_eq!(
            association_state_after.recovery_identifier(),
            &new_recovery_wallet.identifier()
        );

        // Verify that the members are the original wallet, the new recovery wallet, and the
        // installation
        assert!(association_state_after.members().len() == 3);
        // Verify that one of the members is the recovery address
        let binding = association_state_after.members();
        let recovery_member = binding
            .iter()
            .find(|m| m.identifier == original_wallet.identifier());
        assert!(recovery_member.is_some());
        let recovery_member_timestamp = recovery_member.unwrap().client_timestamp_ns;
        // Right now we are not saving client side timestamps for recovery address, so this will be None
        assert!(recovery_member_timestamp.is_none());
        // Verify the other member is an installation key
        let installation_member = binding
            .iter()
            .find(|m| matches!(m.identifier, MemberIdentifier::Installation(_)));
        assert!(installation_member.is_some());
        assert!(
            installation_member
                .unwrap()
                .identifier
                .installation_key()
                .unwrap()
                == client.installation_public_key().to_vec()
        );
        let installation_member_timestamp = installation_member.unwrap().client_timestamp_ns;
        assert!(installation_member_timestamp.is_some());

        // Verify that the original wallet can no longer perform recovery operations
        // by attempting to revoke an installation with the original wallet
        let installation_id = client.installation_public_key().to_vec();
        let mut revoke_installation_request = client
            .identity_updates()
            .revoke_installations(vec![installation_id])
            .await
            .unwrap();

        // Try to sign with the original wallet (will error since signer is not in the request)
        // add_wallet_signature(&mut revoke_installation_request, &original_wallet).await;
        let signature_text = revoke_installation_request.signature_text();
        let sig = original_wallet
            .sign_message(signature_text.as_bytes())
            .await
            .unwrap()
            .as_bytes()
            .to_vec();
        let unverified_sig = UnverifiedSignature::new_recoverable_ecdsa(sig);
        let scw_verifier = MockSmartContractSignatureVerifier::new(false);

        let attempt_to_revoke_with_original_wallet = revoke_installation_request
            .add_signature(unverified_sig, &scw_verifier)
            .await;

        assert!(matches!(
            attempt_to_revoke_with_original_wallet,
            Err(SignatureRequestError::UnknownSigner)
        ));

        // Now try with the new recovery wallet (which should succeed)
        let installation_id = client.installation_public_key().to_vec();
        let mut revoke_installation_request = client
            .identity_updates()
            .revoke_installations(vec![installation_id])
            .await
            .unwrap();

        // Sign with the new recovery wallet
        add_wallet_signature(&mut revoke_installation_request, &new_recovery_wallet).await;

        // This should succeed because the new wallet is now the recovery address
        client
            .identity_updates()
            .apply_signature_request(revoke_installation_request)
            .await
            .unwrap();

        // Verify the installation was revoked
        let association_state_final = get_association_state(&client, client.inbox_id()).await;
        assert_eq!(association_state_final.installation_ids().len(), 0);
    }

    /// Inboxes [`store_scw_inboxes`] stores for each batch read.
    const INBOXES: u64 = 20;

    /// Records the most verify batches in flight at once. The batch for the
    /// inbox created at block `n` yields `INBOXES - n` times, so later inboxes
    /// finish first.
    #[derive(Default)]
    struct BatchesInFlight {
        now: AtomicUsize,
        most: AtomicUsize,
    }

    #[xmtp_common::async_trait]
    impl SmartContractSignatureVerifier for BatchesInFlight {
        async fn is_valid_signature(
            &self,
            account_id: AccountId,
            hash: [u8; 32],
            signature: Bytes,
            block_number: Option<BlockNumber>,
        ) -> Result<ValidationResponse, VerifierError> {
            let input = ScwInput::checked(account_id, hash, signature, block_number)?;
            Ok(self.verify_many(&[input]).await?.remove(0))
        }

        async fn verify_many(
            &self,
            inputs: &[ScwInput],
        ) -> Result<Vec<ValidationResponse>, VerifierError> {
            let now = self.now.fetch_add(1, Ordering::SeqCst) + 1;
            self.most.fetch_max(now, Ordering::SeqCst);
            for _ in inputs[0].block_number().unwrap_or_default()..INBOXES {
                xmtp_common::task::yield_now().await;
            }
            self.now.fetch_sub(1, Ordering::SeqCst);
            Ok(inputs
                .iter()
                .map(|_| ValidationResponse {
                    is_valid: true,
                    block_number: Some(1),
                    error: None,
                })
                .collect())
        }
    }

    /// Store [`INBOXES`] inboxes, the one at index `n` created by a smart
    /// contract wallet at block `n`, and return their identifiers in order.
    fn store_scw_inboxes<C: ConnectionExt>(conn: &DbConnection<C>) -> Vec<(String, Option<i64>)> {
        (0..INBOXES)
            .map(|block| {
                let wallet = Identifier::rand_ethereum();
                let inbox_id = wallet.inbox_id(0).unwrap();
                let create = UnverifiedAction::CreateInbox(UnverifiedCreateInbox::new(
                    UnsignedCreateInbox {
                        nonce: 0,
                        account_identifier: wallet.clone(),
                    },
                    UnverifiedSignature::new_smart_contract_wallet(
                        vec![1],
                        AccountId::new_evm(1, wallet.to_string()),
                        block,
                    ),
                ));
                let update = IdentityUpdateProto::from(UnverifiedIdentityUpdate::new(
                    inbox_id.clone(),
                    1,
                    vec![create],
                ));
                conn.insert_or_ignore_identity_updates(&[StoredIdentityUpdate::new(
                    inbox_id.clone(),
                    1,
                    0,
                    update.encode_to_vec(),
                )])
                .unwrap();
                (inbox_id, None)
            })
            .collect()
    }

    /// A batch read of many inboxes derives at most eight states at once, so
    /// a group's members never open one verify batch each at the same time.
    /// The states come back in the order of the identifiers, although later
    /// ones finish first.
    #[xmtp_common::test(unwrap_try = true)]
    async fn batch_association_state_reads_keep_eight_in_flight() {
        let verifier = Arc::new(BatchesInFlight::default());
        let client = ClientBuilder::new_test_builder(&generate_local_wallet())
            .await
            .with_scw_verifier(verifier.clone())
            .build()
            .await?;
        let conn = client.context.db();
        let inbox_ids = |identifiers: &[(String, Option<i64>)]| {
            identifiers
                .iter()
                .map(|(inbox_id, _)| inbox_id.clone())
                .collect::<Vec<_>>()
        };
        let state_ids = |states: Vec<AssociationState>| {
            states
                .iter()
                .map(|state| state.inbox_id().to_string())
                .collect::<Vec<_>>()
        };

        let identifiers = store_scw_inboxes(&conn);
        let states =
            batch_get_association_state_with_verifier(&conn, &identifiers, &verifier).await?;
        assert_eq!(verifier.most.swap(0, Ordering::SeqCst), 8);
        assert_eq!(state_ids(states), inbox_ids(&identifiers));

        let identifiers = store_scw_inboxes(&conn);
        let states = IdentityUpdates::new(&client.context)
            .batch_get_association_state(&conn, &identifiers)
            .await?;
        assert_eq!(verifier.most.load(Ordering::SeqCst), 8);
        assert_eq!(state_ids(states), inbox_ids(&identifiers));
    }
}

mod conflict_tests {
    use super::*;
    use xmtp_cryptography::utils::generate_local_wallet;
    use xmtp_id::associations::test_utils::{WalletTestExt, add_wallet_signature};
    use xmtp_mls::{context::CoreContext, tester};

    #[xmtp_common::test(flavor = "multi_thread", worker_threads = 4, unwrap_try = true)]
    async fn two_clients_racing_identity_updates_keep_both_associations() {
        tester!(alix, disable_workers);
        tester!(alix2, from: alix, disable_workers);
        use xmtp_proto::api::HasStats;
        let first_stats = alix.context.api().api_client.mls_stats();
        let second_stats = alix2.context.api().api_client.mls_stats();
        const MAX_RACE_ROUNDS: usize = 16;
        const UPDATES_PER_RACE: usize = 2;
        for _ in 0..MAX_RACE_ROUNDS {
            let first_wallet = generate_local_wallet();
            let second_wallet = generate_local_wallet();
            let mut first = alix
                .identity_updates()
                .associate_identity(first_wallet.identifier())
                .await?;
            let mut second = alix2
                .identity_updates()
                .associate_identity(second_wallet.identifier())
                .await?;
            add_wallet_signature(&mut first, &first_wallet).await;
            add_wallet_signature(&mut second, &second_wallet).await;
            let first_updates = alix.identity_updates();
            let second_updates = alix2.identity_updates();
            first_stats.clear();
            second_stats.clear();
            let (first, second) = {
                xmtp_common::wasm_or_native! {
                    native => {
                        // Start both RPCs on separate threads. Require a retry below.
                        let barrier = std::sync::Barrier::new(UPDATES_PER_RACE);
                        let runtime = tokio::runtime::Handle::current();
                        std::thread::scope(|scope| {
                            let first = scope.spawn(|| {
                                barrier.wait();
                                runtime.block_on(first_updates.apply_signature_request(first))
                            });
                            let second = scope.spawn(|| {
                                barrier.wait();
                                runtime.block_on(second_updates.apply_signature_request(second))
                            });
                            (first.join().unwrap(), second.join().unwrap())
                        })
                    },
                    wasm => {
                        futures::join!(
                            first_updates.apply_signature_request(first),
                            second_updates.apply_signature_request(second),
                        )
                    }
                }
            };
            first?;
            second?;
            let publishes = first_stats.publish.get_count() + second_stats.publish.get_count();
            let conn = alix.context.db();
            load_identity_updates(alix.context.api(), &conn, &[alix.inbox_id()]).await?;
            let state = alix
                .identity_updates()
                .get_association_state(&conn, alix.inbox_id(), None)
                .await?;
            assert!(state.get(&first_wallet.identifier().into()).is_some());
            assert!(state.get(&second_wallet.identifier().into()).is_some());
            if publishes > UPDATES_PER_RACE {
                tracing::info!(publishes, "Both identity updates survived a publish retry");
                return;
            }
        }
        panic!("No identity publish retry occurred in {MAX_RACE_ROUNDS} races");
    }
}
