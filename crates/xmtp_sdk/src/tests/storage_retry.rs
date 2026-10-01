//! A database failure keeps its retry policy on a read and is never
//! retryable on a send or commit.

use super::*;
use crate::{
    ComponentMutation, FieldKey, FieldValue, SendOptions, StandardContent, UserFieldUpdate,
    WellKnownMetadataField as WellKnown, metadata_field_ref,
};
use xmtp_db::ConnectionExt;

/// Each failed call's variant, code and retryability.
fn kinds(results: Vec<Result<(), XmtpError>>) -> Vec<(String, String, bool)> {
    results
        .into_iter()
        .map(|result| {
            let error = result.expect_err("the call fails while storage is disconnected");
            let (name, details) = match error {
                XmtpError::Storage(details) => ("Storage", details),
                XmtpError::Unknown(details) => ("Unknown", details),
                other => panic!("unexpected error {other:?}"),
            };
            (name.to_owned(), details.code, details.retryable)
        })
        .collect()
}

/// With the database disconnected, every metadata read fails `Storage`
/// and retryable, as its cause is. Every send, publish and metadata write
/// fails `Storage` and not retryable, because the failed call may already
/// have queued or published its work.
#[xmtp_common::test(unwrap_try = true)]
async fn storage_failures_are_retryable_on_reads_only() {
    let mut settings = options();
    let path = std::env::temp_dir().join(format!(
        "xmtp-sdk-storage-retry-{}-{}.db3",
        std::process::id(),
        xmtp_common::time::now_ns()
    ));
    settings.storage.location = StorageLocation::Explicit {
        db_path: path.to_string_lossy().into_owned(),
        attachments_dir: path
            .with_extension("attachments")
            .to_string_lossy()
            .into_owned(),
    };
    let client = Client::create(crate::generate_local_signer().await, settings).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    let text = || crate::encode_standard(StandardContent::Text("hi".into()));
    let prepared = group.prepare_message(text()?, None).await?;
    let name = || metadata_field_ref(WellKnown::GroupName);
    let display_name = || metadata_field_ref(WellKnown::UserDisplayName);

    client.inner.context.db().disconnect()?;
    let reads = kinds(vec![
        group.metadata_fields().await.map(drop),
        group.metadata_field("group_name".into()).await.map(drop),
        group.metadata_value(name()).await.map(drop),
        group.metadata_values(vec![name()]).await.map(drop),
        group
            .map_value(
                display_name(),
                FieldKey::InboxId(client.inbox_id().into_checked()?),
            )
            .await
            .map(drop),
        group.user_data(None, None).await.map(drop),
    ]);
    let writes = kinds(vec![
        group
            .update_metadata_field(
                name(),
                ComponentMutation::Replace(FieldValue::String("renamed".into())),
            )
            .await,
        group
            .update_user_data(vec![UserFieldUpdate {
                field: display_name(),
                value: Some(FieldValue::String("Alix".into())),
            }])
            .await,
        group.send(text()?, None).await.map(drop),
        group
            .send(
                text()?,
                Some(SendOptions {
                    optimistic: true,
                    ..SendOptions::default()
                }),
            )
            .await
            .map(drop),
        group.publish_messages().await,
        group.publish_message(prepared).await,
    ]);
    client.inner.context.db().reconnect()?;
    client.end().await?;
    let _ = std::fs::remove_file(&path);

    let storage = |retryable| ("Storage".to_owned(), "Storage".to_owned(), retryable);
    assert_eq!(reads, vec![storage(true); 6]);
    assert_eq!(writes, vec![storage(false); 6]);
}
