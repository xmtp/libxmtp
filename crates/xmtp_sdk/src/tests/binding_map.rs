use super::*;

async fn build_on_new_database_fails_identity_not_found(
    allow_offline: bool,
) -> Result<(), XmtpError> {
    use xmtp_db::{Fetch, identity::StoredIdentity};

    let signer = crate::generate_local_signer().await;
    let identity = signer::identity(signer).await?;
    let inbox_id = InboxId::try_from(
        identity
            .to_core()?
            .inbox_id(0)
            .map_err(XmtpError::unknown)?,
    )?;
    let path = std::env::temp_dir().join(format!(
        "sdk-build-new-{}-{}-{}.db3",
        allow_offline,
        std::process::id(),
        xmtp_common::time::now_ns(),
    ));
    let mut settings = options();
    settings.storage.location = StorageLocation::Path(path.to_string_lossy().into_owned());
    settings.allow_offline = allow_offline;
    settings.backend = Some(BackendSource::Options {
        options: BackendOptions {
            url: "http://127.0.0.1:1".into(),
            ..Default::default()
        },
    });

    let result = Client::build(identity, settings.clone(), Some(inbox_id.clone())).await;
    assert!(
        matches!(result, Err(XmtpError::IdentityNotFound(ref details))
        if details.code == "IdentityNotFound"
            && matches!(details.category, crate::ErrorCategory::Identity)
            && !details.retryable)
    );
    assert!(!path.exists(), "build created a new database");
    let store = crate::client::open_store(&settings.storage, inbox_id.checked()?).await?;
    let stored: Option<StoredIdentity> = store.db().fetch(&()).map_err(XmtpError::unknown)?;
    assert!(stored.is_none(), "build registered a new identity");
    drop(store);
    std::fs::remove_file(path).map_err(XmtpError::unknown)?;
    Ok(())
}

mod build;
mod configuration;
mod content;
