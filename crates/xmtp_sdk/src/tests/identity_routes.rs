use super::*;

async fn member_ids(group: &crate::Group) -> Result<Vec<InboxId>, XmtpError> {
    let mut ids = group
        .members()
        .await?
        .into_iter()
        .map(|member| member.inbox_id)
        .collect::<Vec<_>>();
    ids.sort_by_key(|id| id.clone().into_checked().unwrap_or_default());
    Ok(ids)
}

fn sorted(mut ids: Vec<InboxId>) -> Vec<InboxId> {
    ids.sort_by_key(|id| id.clone().into_checked().unwrap_or_default());
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

// The account lookup finds the DM with a registered peer, and finds none for a
// registered account without a DM or an account that has no inbox.
#[xmtp_common::test(unwrap_try = true)]
async fn dm_lookup_by_account_finds_only_an_existing_dm() {
    let alix = Client::create(crate::generate_local_signer().await, options()).await?;
    let bo = Client::create(crate::generate_local_signer().await, options()).await?;
    let caro = Client::create(crate::generate_local_signer().await, options()).await?;
    let conversations = alix.conversations();
    let dm = conversations.create_dm(bo.inbox_id(), None).await?;

    let found = conversations.get_dm_by_identity(bo.identity()).await?;
    assert_eq!(found.map(|dm| dm.id()), Some(dm.id()));
    assert!(
        conversations
            .get_dm_by_identity(caro.identity())
            .await?
            .is_none()
    );
    let unregistered = PublicIdentity {
        identifier: "1111111111111111111111111111111111111111".into(),
        kind: PublicIdentityKind::Ethereum,
    };
    assert!(
        conversations
            .get_dm_by_identity(unregistered)
            .await?
            .is_none()
    );
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

// A DM with the client's own inbox fails by inbox ID and by account, and
// stores no conversation.
#[xmtp_common::test(unwrap_try = true)]
async fn dm_with_own_inbox_fails_and_stores_no_conversation() {
    let alix = Client::create(crate::generate_local_signer().await, options()).await?;
    let conversations = alix.conversations();
    let by_inbox = conversations.create_dm(alix.inbox_id(), None).await.err();
    let by_identity = conversations
        .create_dm_with_identity(alix.identity(), None)
        .await
        .err();
    for error in [by_inbox, by_identity] {
        let error = error.expect("a DM with the own inbox must fail");
        assert!(
            error.to_string().contains("self-reference"),
            "the DM must fail on its own inbox, got {error:?}"
        );
    }
    assert!(conversations.list(None).await?.is_empty());
    assert!(
        conversations
            .get_dm_by_inbox_id(alix.inbox_id())
            .await?
            .is_none()
    );
    alix.end().await?;
}

// A DM with an account that has no inbox fails because no inbox is found, and
// the account lookup then finds no DM.
#[xmtp_common::test(unwrap_try = true)]
async fn dm_with_unregistered_account_fails_and_stores_no_conversation() {
    let alix = Client::create(crate::generate_local_signer().await, options()).await?;
    let unregistered = signer::identity(crate::generate_local_signer().await).await?;
    let conversations = alix.conversations();
    let error = conversations
        .create_dm_with_identity(unregistered.clone(), None)
        .await
        .err()
        .expect("a DM with an unregistered account must fail");
    let expected = format!("inbox id for address {} not found", unregistered.identifier);
    assert!(
        error.to_string().contains(&expected),
        "the DM must fail on the inbox lookup, got {error:?}"
    );
    assert!(
        conversations
            .get_dm_by_identity(unregistered)
            .await?
            .is_none()
    );
    assert!(conversations.list(None).await?.is_empty());
    alix.end().await?;
}

// verifies: PROC-036
#[xmtp_common::test(unwrap_try = true)]
async fn inbox_member_add_error_logs_omit_installation_ids() {
    check_member_add_error_logs(false).await?;
}

// verifies: PROC-036
#[xmtp_common::test(unwrap_try = true)]
async fn identity_member_add_error_logs_omit_installation_ids() {
    check_member_add_error_logs(true).await?;
}

async fn check_member_add_error_logs(by_identity: bool) -> Result<(), XmtpError> {
    use tracing::instrument::WithSubscriber;
    use xmtp_logging::{Level, LogRecord, LogSinkTarget, SinkError, test_logging::LogCapture};
    use xmtp_mls::utils::test_mocks_helpers::set_test_mode_upload_malformed_keypackage;

    #[derive(Default)]
    struct Capture(parking_lot::Mutex<Vec<LogRecord>>);
    impl LogSinkTarget for Capture {
        fn on_record(&self, record: LogRecord) -> Result<(), SinkError> {
            self.0.lock().push(record);
            Ok(())
        }
    }
    struct RestoreKeyPackages;
    impl Drop for RestoreKeyPackages {
        fn drop(&mut self) {
            set_test_mode_upload_malformed_keypackage(false, None);
        }
    }

    let alix = Client::create(crate::generate_local_signer().await, options()).await?;
    let bo = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = alix.conversations().create_group(vec![], None).await?;
    let failed_id = bo.installation_id().to_bytes()?;
    set_test_mode_upload_malformed_keypackage(true, Some(vec![failed_id.clone()]));
    let _restore = RestoreKeyPackages;
    // The caller capture checks facade return logs. The core pipeline has its
    // own capture because native SDK worker tasks use another dispatcher.
    for invalid in [false, true] {
        let sink = Arc::new(Capture::default());
        let capture = LogCapture::with_sink(Level::Trace, Some(sink.clone()));
        let result = async {
            if by_identity {
                let member = if invalid {
                    PublicIdentity {
                        identifier: "invalid-address".into(),
                        kind: PublicIdentityKind::Ethereum,
                    }
                } else {
                    bo.identity()
                };
                group.add_members_by_identity(vec![member]).await
            } else {
                let member = if invalid {
                    InboxId::unchecked("invalid-inbox".into())
                } else {
                    bo.inbox_id()
                };
                group.add_members(vec![member]).await
            }
        }
        .with_subscriber(capture.dispatch())
        .await;
        let error = result.expect_err("member add must fail");
        if !invalid {
            assert!(matches!(error, XmtpError::Unknown(_)));
            assert!(
                error.to_string().contains(&hex::encode(&failed_id)),
                "returned failure keeps installation ID"
            );
        }
        let json = capture.output();
        let records = sink.0.lock();
        for sensitive in [hex::encode(&failed_id), format!("{failed_id:?}")] {
            assert!(
                !json.contains(&sensitive),
                "full installation ID reached facade JSON log"
            );
            for record in records.iter() {
                assert!(
                    !record.message.contains(&sensitive),
                    "full installation ID reached facade sink message"
                );
                assert!(
                    !format!("{:?}", record.fields).contains(&sensitive),
                    "full installation ID reached facade sink fields"
                );
            }
        }
        assert!(
            records.iter().any(|record| record.level == Level::Error
                && record.fields.get("error").map(String::as_str) == Some("operation failed")),
            "facade must emit constant error for both worker and validation failures"
        );
    }
    Ok(())
}
