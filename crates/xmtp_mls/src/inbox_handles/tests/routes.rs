//! Routes that take accounts or handles.

use super::{
    LookupScript, ScriptedLookups, random_handle, scripted_client, wait_until_claim_visible,
};
use crate::{
    builder::ClientBuilderError,
    client::ClientError,
    context::XmtpSharedContext,
    groups::GroupError,
    identity::{IdentityError, IdentityStrategy},
    inbox_handles::{HandleError, IdentityTarget},
    tester,
};
use parking_lot::Mutex;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
use xmtp_cryptography::utils::generate_local_wallet;
use xmtp_db::{NotFound, StorageError, XmtpTestDb, prelude::*};
use xmtp_id::associations::{
    Identifier,
    test_utils::{WalletTestExt, add_wallet_signature},
};
use xmtp_mls_common::{group::DMMetadataOptions, group_metadata::DmMembers};

/// The inbox ids of a group's members.
async fn member_ids<Context: crate::context::XmtpSharedContext>(
    group: &crate::groups::MlsGroup<Context>,
) -> Result<HashSet<String>, GroupError> {
    Ok(group
        .members()
        .await?
        .into_iter()
        .map(|member| member.inbox_id)
        .collect())
}

fn no_inbox(error: &ClientError) -> bool {
    matches!(error, ClientError::Handle(HandleError::NotFound))
}

// verifies: IDENT-085
#[xmtp_common::test(unwrap_try = true)]
async fn every_handle_route_rejects_an_unclaimed_answer() {
    tester!(alix, disable_workers);
    tester!(bo);
    // A route that trusted the answer would find this DM, or change this group.
    alix.find_or_create_dm(bo.inbox_id(), None).await?;
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let (client, script) = scripted_client(&alix).await?;
    let handle = random_handle();
    script
        .lock()
        .lies
        .insert(handle.to_string(), bo.inbox_id().to_owned());
    let target = IdentityTarget::Handle(handle);
    let targets = std::slice::from_ref(&target);

    assert!(no_inbox(
        &client.resolve_targets(targets).await.unwrap_err()
    ));
    assert!(client.find_dm_by_target(&target).await?.is_none());
    assert!(no_inbox(
        &client
            .find_or_create_dm_by_target(&target, DMMetadataOptions::default())
            .await
            .unwrap_err()
    ));
    assert!(no_inbox(
        &client
            .create_group_with_targets(targets, None, None)
            .await
            .unwrap_err()
    ));
    let group = client.group(&group.group_id)?;
    assert!(matches!(
        group.add_members_by_targets(targets).await,
        Err(GroupError::Client(ref error)) if no_inbox(error)
    ));
    assert!(matches!(
        group.remove_members_by_targets(targets).await,
        Err(GroupError::Client(ref error)) if no_inbox(error)
    ));
    assert_eq!(script.lock().publishes, 0, "nothing is published");
}

#[xmtp_common::test(unwrap_try = true)]
async fn mixed_targets_make_one_commit() {
    tester!(alix);
    tester!(bo);
    tester!(caro);
    let handle = random_handle();
    bo.claim_handle(&handle).await?;
    wait_until_claim_visible(&alix, &handle).await;
    let group = alix.create_group(None, None)?;
    let epoch = group.epoch().await?;

    group
        .add_members_by_targets(&[
            IdentityTarget::Handle(handle),
            IdentityTarget::Account(caro.identifier()),
        ])
        .await?;

    assert_eq!(group.epoch().await?, epoch + 1);
    let members: HashSet<_> = group
        .members()
        .await?
        .into_iter()
        .map(|member| member.inbox_id)
        .collect();
    assert!(members.contains(bo.inbox_id()));
    assert!(members.contains(caro.inbox_id()));
}

#[xmtp_common::test(unwrap_try = true)]
async fn handle_routes_reach_the_claimed_inbox() {
    tester!(alix);
    tester!(bo);
    tester!(caro);
    let handle = random_handle();
    bo.claim_handle(&handle).await?;
    wait_until_claim_visible(&alix, &handle).await;
    let bo_target = IdentityTarget::Handle(handle);
    let caro_target = IdentityTarget::Account(caro.identifier());

    // The DM by handle is the DM by inbox id.
    let dm = alix
        .find_or_create_dm_by_target(&bo_target, DMMetadataOptions::default())
        .await?;
    let by_inbox = alix.find_or_create_dm(bo.inbox_id(), None).await?;
    assert_eq!(dm.group_id, by_inbox.group_id);
    let found = alix.find_dm_by_target(&bo_target).await?.expect("the DM");
    assert_eq!(found.group_id, dm.group_id);

    // A mixed list creates the group, and later leaves it, in one commit each.
    let group = alix
        .create_group_with_targets(&[bo_target.clone(), caro_target.clone()], None, None)
        .await?;
    assert_eq!(group.epoch().await?, 1);
    let members = member_ids(&group).await?;
    assert!(members.contains(bo.inbox_id()) && members.contains(caro.inbox_id()));
    group
        .remove_members_by_targets(&[bo_target, caro_target])
        .await?;
    assert_eq!(group.epoch().await?, 2);
    assert_eq!(
        member_ids(&group).await?,
        HashSet::from([alix.inbox_id().to_owned()])
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn account_targets_keep_the_account_path() {
    tester!(alix);
    tester!(bo);
    tester!(caro);
    let bo_target = IdentityTarget::Account(bo.identifier());
    let caro_target = IdentityTarget::Account(caro.identifier());

    let dm = alix
        .find_or_create_dm_by_target(&bo_target, DMMetadataOptions::default())
        .await?;
    let found = alix.find_dm_by_target(&bo_target).await?.expect("the DM");
    assert_eq!(found.group_id, dm.group_id);

    let group = alix
        .create_group_with_targets(std::slice::from_ref(&bo_target), None, None)
        .await?;
    group
        .add_members_by_targets(std::slice::from_ref(&caro_target))
        .await?;
    assert!(member_ids(&group).await?.contains(caro.inbox_id()));
    group
        .remove_members_by_targets(std::slice::from_ref(&caro_target))
        .await?;
    assert!(!member_ids(&group).await?.contains(caro.inbox_id()));

    // An account with no inbox keeps the account path's error.
    let unknown = IdentityTarget::Account(Identifier::rand_ethereum());
    assert!(matches!(
        group.add_members_by_targets(&[unknown]).await,
        Err(GroupError::AddressNotFound(_))
    ));
}

#[xmtp_common::test(unwrap_try = true)]
async fn mixed_targets_treat_unknown_accounts_as_the_account_path_does() {
    tester!(alix);
    tester!(bo);
    tester!(caro);
    let handle = random_handle();
    bo.claim_handle(&handle).await?;
    wait_until_claim_visible(&alix, &handle).await;
    let bo_target = IdentityTarget::Handle(handle);
    let unknown_identifier = Identifier::rand_ethereum();
    let unknown = IdentityTarget::Account(unknown_identifier.clone());
    // Each failure names exactly the unknown account.
    let missing = vec![unknown_identifier.to_string()];
    let create_missing = |result: Result<(), ClientError>| match result {
        Err(ClientError::Group(error)) => match *error {
            GroupError::AddressNotFound(missing) => Some(missing),
            _ => None,
        },
        _ => None,
    };

    // Create: the account-only error, with or without a handle in the list.
    let account_only = alix
        .create_group_with_targets(std::slice::from_ref(&unknown), None, None)
        .await;
    assert_eq!(
        create_missing(account_only.map(drop)),
        Some(missing.clone())
    );
    let mixed = alix
        .create_group_with_targets(&[bo_target.clone(), unknown.clone()], None, None)
        .await;
    assert_eq!(create_missing(mixed.map(drop)), Some(missing.clone()));

    // Add: the account-only error, and no commit.
    let group = alix
        .create_group_with_targets(&[IdentityTarget::Account(caro.identifier())], None, None)
        .await?;
    let epoch = group.epoch().await?;
    for targets in [
        vec![unknown.clone()],
        vec![bo_target.clone(), unknown.clone()],
    ] {
        assert!(matches!(
            group.add_members_by_targets(&targets).await,
            Err(GroupError::AddressNotFound(found)) if found == missing
        ));
    }
    assert_eq!(group.epoch().await?, epoch);

    // Remove: the unknown account is skipped, with or without a handle.
    group
        .remove_members_by_targets(std::slice::from_ref(&unknown))
        .await?;
    group
        .add_members_by_targets(std::slice::from_ref(&bo_target))
        .await?;
    group
        .remove_members_by_targets(&[bo_target, unknown])
        .await?;
    assert!(!member_ids(&group).await?.contains(bo.inbox_id()));
}

/// Whether `error` is an account lookup that found no inbox.
fn no_account_inbox(error: &ClientError) -> bool {
    matches!(
        error,
        ClientError::Storage(StorageError::NotFound(NotFound::InboxIdForAddress(_)))
    )
}

/// Whether `result` is a group create whose add found no inbox for an account.
fn create_found_no_account<T>(result: Result<T, ClientError>) -> bool {
    matches!(result, Err(ClientError::Group(error)) if matches!(*error, GroupError::AddressNotFound(_)))
}

// verifies: IDENT-096
#[xmtp_common::test(unwrap_try = true)]
async fn every_account_route_rejects_an_unheld_answer() {
    tester!(alix, disable_workers);
    tester!(bo);
    tester!(caro);
    let (client, script) = scripted_client(&alix).await?;
    // The backend names bo for caro's account, which bo's log never holds.
    let account = caro.identifier();
    script
        .lock()
        .lies
        .insert(account.to_string(), bo.inbox_id().to_owned());
    let target = IdentityTarget::Account(account.clone());
    let targets = std::slice::from_ref(&target);
    let accounts = std::slice::from_ref(&account);
    let dm_with_bo = DmMembers {
        member_one_inbox_id: alix.inbox_id(),
        member_two_inbox_id: bo.inbox_id(),
    };

    assert!(no_account_inbox(
        &client.resolve_targets(targets).await.unwrap_err()
    ));
    // A route that trusted the answer would create a DM with bo.
    for result in [
        client
            .find_or_create_dm_by_target(&target, DMMetadataOptions::default())
            .await,
        client
            .find_or_create_dm_by_identity(account.clone(), None)
            .await,
    ] {
        assert!(no_account_inbox(&result.unwrap_err()));
    }
    assert!(
        client
            .context
            .db()
            .find_active_dm_group(&dm_with_bo)?
            .is_none()
    );
    // A route that trusted the answer would find this DM.
    alix.find_or_create_dm(bo.inbox_id(), None).await?;
    assert!(client.find_dm_by_target(&target).await?.is_none());

    assert!(create_found_no_account(
        client.create_group_with_targets(targets, None, None).await
    ));
    assert!(create_found_no_account(
        client
            .create_group_with_identifiers(accounts, None, None)
            .await
    ));

    // A route that trusted the answer would add bo.
    let group = client.create_group(None, None)?;
    let epoch = group.epoch().await?;
    assert!(matches!(
        group.add_members_by_identity(accounts).await,
        Err(GroupError::AddressNotFound(_))
    ));
    assert!(matches!(
        group.add_members_by_targets(targets).await,
        Err(GroupError::AddressNotFound(_))
    ));
    assert_eq!(group.epoch().await?, epoch);

    // A route that trusted the answer would remove bo.
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let group = client.group(&group.group_id)?;
    group.remove_members_by_identity(accounts).await?;
    group.remove_members_by_targets(targets).await?;
    assert!(member_ids(&group).await?.contains(bo.inbox_id()));
}

// verifies: IDENT-096
#[xmtp_common::test(unwrap_try = true)]
async fn an_inbox_the_account_left_is_rejected() {
    tester!(alix, disable_workers);
    tester!(bo);
    let wallet = generate_local_wallet();
    let identity_updates = bo.identity_updates();
    let mut add = identity_updates
        .associate_identity(wallet.identifier())
        .await?;
    add_wallet_signature(&mut add, &wallet).await;
    identity_updates.apply_signature_request(add).await?;
    let mut revoke = identity_updates
        .revoke_identities(vec![wallet.identifier()])
        .await?;
    add_wallet_signature(&mut revoke, &bo.builder.owner).await;
    identity_updates.apply_signature_request(revoke).await?;
    let (client, script) = scripted_client(&alix).await?;
    // The backend names bo for the wallet that bo has revoked. Bo's states
    // from the add until the revocation hold the wallet; the newest does not.
    script
        .lock()
        .lies
        .insert(wallet.identifier().to_string(), bo.inbox_id().to_owned());

    let target = IdentityTarget::Account(wallet.identifier());
    assert!(no_account_inbox(
        &client
            .resolve_targets(std::slice::from_ref(&target))
            .await
            .unwrap_err()
    ));
    assert!(no_account_inbox(
        &client
            .find_or_create_dm_by_identity(wallet.identifier(), None)
            .await
            .unwrap_err()
    ));
    let dm_with_bo = DmMembers {
        member_one_inbox_id: alix.inbox_id(),
        member_two_inbox_id: bo.inbox_id(),
    };
    assert!(
        client
            .context
            .db()
            .find_active_dm_group(&dm_with_bo)?
            .is_none()
    );
}

// verifies: IDENT-096
#[xmtp_common::test(unwrap_try = true)]
async fn an_identity_for_an_account_rejects_an_unheld_answer() {
    tester!(bo);
    let wallet = generate_local_wallet();
    // A build that looks the wallet's inbox up, without opting in to the
    // identifier check.
    let build = async |lies: HashMap<String, String>| {
        let api = ScriptedLookups {
            inner: bo.context.api().api_client.raw_for_test().clone(),
            script: Arc::new(Mutex::new(LookupScript {
                lies,
                ..Default::default()
            })),
        };
        crate::Client::builder(IdentityStrategy::for_identifier(wallet.identifier(), 0))
            .api_client(api)
            .with_remote_verifier()?
            .store(xmtp_db::TestDb::create_ephemeral_store().await)
            .default_mls_store()?
            .with_disable_workers(true)
            .build()
            .await
    };

    // The backend names bo for a wallet that bo's log never holds.
    let lie = HashMap::from([(wallet.identifier().to_string(), bo.inbox_id().to_owned())]);
    let lied = build(lie).await;
    assert!(
        matches!(
            &lied,
            Err(ClientBuilderError::Identity(IdentityError::IdentifierNotInInbox { inbox_id }))
                if inbox_id == bo.inbox_id()
        ),
        "build: {:?}",
        lied.err()
    );
    // A new wallet still gets the inbox it creates, which has no update yet.
    let client = build(HashMap::new()).await?;
    assert_eq!(client.inbox_id(), wallet.identifier().inbox_id(0)?);
}
