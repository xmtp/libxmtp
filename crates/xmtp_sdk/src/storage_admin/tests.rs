use super::*;
use crate::{StorageOptions, client};
use futures::FutureExt;

#[xmtp_common::test(unwrap_try = true)]
async fn admin_end_fences_and_drains_accepted_calls() {
    let (first, second) = futures::try_join!(StorageAdmin::open(), StorageAdmin::open())?;
    let mut clear = std::pin::pin!(first.clear_all());
    assert!(futures::poll!(&mut clear).is_pending());
    let mut end = std::pin::pin!(first.end());
    assert!(futures::poll!(&mut end).is_pending());
    assert!(matches!(
        first.file_count().now_or_never(),
        Some(Err(XmtpError::ClientClosed(_)))
    ));
    assert!(matches!(
        second.file_count().now_or_never(),
        Some(Err(XmtpError::StorageBusy(_)))
    ));
    clear.await?;
    end.await?;
    first.end().await?;
    assert_eq!(second.file_count().await?, 0);
    assert!(second.pool_capacity().await? > 0);
    second.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn admin_uses_guarded_file_operations() {
    let admin = StorageAdmin::open().await?;
    let source = "admin-source.db".to_owned();
    let target = "admin-restored.db".to_owned();
    let store = client::open_store(&StorageOptions::default(), Some(&source)).await?;
    let before = admin.list_files().await?;
    assert!(before.contains(&source));
    assert_eq!(admin.file_count().await? as usize, before.len());
    assert!(admin.file_exists(source.clone()).await?);
    assert!(matches!(
        admin.export_db(source.clone()).await,
        Err(XmtpError::StorageBusy(_))
    ));
    assert!(matches!(
        admin.delete_file(source.clone()).await,
        Err(XmtpError::StorageBusy(_))
    ));
    assert!(matches!(
        admin.clear_all().await,
        Err(XmtpError::StorageBusy(_))
    ));
    assert_eq!(admin.list_files().await?, before);
    store.release_connection()?;
    let data = admin.export_db(source.clone()).await?;
    assert!(data.starts_with(b"SQLite format 3\0"));
    admin.import_db(target.clone(), data).await?;
    assert!(admin.file_exists(target.clone()).await?);
    assert!(admin.delete_file(source.clone()).await?);
    assert!(!admin.delete_file(source).await?);
    admin.clear_all().await?;
    assert!(admin.list_files().await?.is_empty());
    assert!(!admin.file_exists(target).await?);
    admin.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn admin_rejects_uri_paths_without_changing_files() {
    let admin = StorageAdmin::open().await?;
    let before = admin.list_files().await?;
    for path in ["file:admin.db", "sqlite://admin.db"] {
        for result in [
            admin.file_exists(path.into()).await.map(|_| ()),
            admin.export_db(path.into()).await.map(|_| ()),
            admin.import_db(path.into(), vec![]).await,
            admin.delete_file(path.into()).await.map(|_| ()),
        ] {
            assert!(matches!(result, Err(XmtpError::InvalidInput(_))));
        }
    }
    assert_eq!(admin.list_files().await?, before);
    admin.end().await?;
}
