//! Checks every metadata call makes: inbox ID spelling and a closed client.

use super::*;

/// Inbox ID text must be the lowercase hex results are keyed by, so an
/// uppercase spelling is an invalid argument wherever an inbox ID is read.
#[xmtp_common::test(unwrap_try = true)]
async fn inbox_ids_must_be_lowercase() {
    let alix = client_with(alix_catalogue()).await;
    let group = alix.conversations().create_group(vec![], None).await?;
    let upper = "AB".repeat(32);
    let nickname = field(NICKNAME, None);

    let refused = [
        group
            .user_data(None, Some(vec![InboxId::try_from(upper.clone())?]))
            .await
            .map(drop),
        group
            .map_value(nickname.clone(), FieldKey::InboxId(upper.clone()))
            .await
            .map(drop),
        group
            .update_metadata_field(
                nickname.clone(),
                ComponentMutation::MapDelta(vec![MapMutation::Insert(
                    FieldKey::InboxId(upper),
                    string("x"),
                )]),
            )
            .await,
    ];
    let invalid = || {
        Some((
            "InvalidArgument".to_owned(),
            "InvalidArgument".to_owned(),
            "Input".to_owned(),
            false,
        ))
    };
    assert_eq!(
        refused.map(|result| result.err().map(kind)),
        [invalid(), invalid(), invalid()]
    );
    let lower = InboxId::try_from("ab".repeat(32))?;
    assert_eq!(
        group.user_data(None, Some(vec![lower.clone()])).await?,
        HashMap::from([(lower, vec![])])
    );

    alix.end().await?;
}

/// A metadata call on an ended client fails `ClientClosed`, for a read and
/// for a write.
#[xmtp_common::test(unwrap_try = true)]
async fn metadata_calls_after_end_are_closed() {
    let alix = client_with(alix_catalogue()).await;
    let group = alix.conversations().create_group(vec![], None).await?;
    alix.end().await?;

    expect_kind(
        group.metadata_fields().await.unwrap_err(),
        "ClientClosed",
        ErrorCategory::Lifecycle,
    );
    expect_kind(
        group
            .update_user_data(vec![set(field(NICKNAME, None), "A")])
            .await
            .unwrap_err(),
        "ClientClosed",
        ErrorCategory::Lifecycle,
    );
}
