use super::*;

async fn member_ids(group: &crate::Group) -> Result<Vec<InboxId>, XmtpError> {
    let mut ids = group
        .members()
        .await?
        .into_iter()
        .map(|member| member.inbox_id)
        .collect::<Vec<_>>();
    ids.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(ids)
}

fn sorted(mut ids: Vec<InboxId>) -> Vec<InboxId> {
    ids.sort_by(|a, b| a.0.cmp(&b.0));
    ids
}

// Every account-identity route performs the same membership change as its inbox form.
#[xmtp_common::test(unwrap_try = true)]
async fn identity_routes_change_membership_by_account() {
    let alix = Client::create(crate::generate_local_signer().await, options()).await?;
    let bo = Client::create(crate::generate_local_signer().await, options()).await?;
    let caro = Client::create(crate::generate_local_signer().await, options()).await?;
    let conversations = alix.conversations();

    let empty = conversations
        .create_group_with_identities(vec![], None)
        .await?;
    assert_eq!(member_ids(&empty).await?, vec![alix.inbox_id()]);

    let group = conversations
        .create_group_with_identities(vec![bo.identity()], None)
        .await?;
    assert_eq!(
        member_ids(&group).await?,
        sorted(vec![alix.inbox_id(), bo.inbox_id()])
    );

    let added = group.add_members_by_identity(vec![caro.identity()]).await?;
    assert_eq!(added.added, vec![caro.inbox_id()]);
    assert_eq!(
        member_ids(&group).await?,
        sorted(vec![alix.inbox_id(), bo.inbox_id(), caro.inbox_id()])
    );

    group
        .remove_members_by_identity(vec![bo.identity()])
        .await?;
    assert_eq!(
        member_ids(&group).await?,
        sorted(vec![alix.inbox_id(), caro.inbox_id()])
    );

    let dm = conversations
        .create_dm_with_identity(caro.identity(), None)
        .await?;
    assert_eq!(dm.peer_inbox_id().await?, Some(caro.inbox_id()));
    let again = conversations
        .create_dm_with_identity(caro.identity(), None)
        .await?;
    assert_eq!(again.id(), dm.id());
    let by_inbox = conversations.create_dm(caro.inbox_id(), None).await?;
    assert_eq!(by_inbox.id(), dm.id());
}

// A malformed element rejects the whole call, including valid elements before it.
#[xmtp_common::test(unwrap_try = true)]
async fn identity_routes_reject_a_malformed_element() {
    let alix = Client::create(crate::generate_local_signer().await, options()).await?;
    let bo = Client::create(crate::generate_local_signer().await, options()).await?;
    let malformed = PublicIdentity {
        identifier: "not an address".into(),
        kind: PublicIdentityKind::Ethereum,
    };
    let conversations = alix.conversations();
    let group = conversations.create_group(vec![], None).await?;
    let mixed = vec![bo.identity(), malformed.clone()];
    assert!(
        conversations
            .create_group_with_identities(mixed.clone(), None)
            .await
            .is_err()
    );
    assert!(
        conversations
            .create_dm_with_identity(malformed, None)
            .await
            .is_err()
    );
    assert!(group.add_members_by_identity(mixed.clone()).await.is_err());
    assert_eq!(member_ids(&group).await?, vec![alix.inbox_id()]);
    group.add_members(vec![bo.inbox_id()]).await?;
    assert!(group.remove_members_by_identity(mixed).await.is_err());
    assert_eq!(
        member_ids(&group).await?,
        sorted(vec![alix.inbox_id(), bo.inbox_id()])
    );
    let listed = conversations.list(None).await?;
    assert_eq!(listed.len(), 1, "a rejected create stored a conversation");
}
