use std::path::Path;

use crate::{ErrorCategory, StorageOptions, XmtpError, client};
use futures::FutureExt;
use xmtp_db::{PlatformStorageError, StorageError};

wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

#[xmtp_common::test(unwrap_try = true)]
fn storage_busy_keeps_direct_and_wrapped_causes() {
    #[derive(Debug, thiserror::Error)]
    #[error("wrapped storage: {0}")]
    struct Wrapped(#[source] StorageError);
    impl crate::error::CoreError for Wrapped {}

    for error in [
        client::map_wasm_storage_error(PlatformStorageError::DatabaseInUse),
        client::map_wasm_storage_error(StorageError::from(PlatformStorageError::DatabaseInUse)),
        client::map_wasm_storage_error(Wrapped(PlatformStorageError::DatabaseInUse.into())),
        client::map_wasm_storage_error(xmtp_db::ConnectionError::Platform(
            PlatformStorageError::DatabaseInUse,
        )),
        client::map_wasm_storage_error(StorageError::from(xmtp_db::ConnectionError::Platform(
            PlatformStorageError::DatabaseInUse,
        ))),
    ] {
        let XmtpError::StorageBusy(details) = error else {
            panic!("expected StorageBusy, got {error:?}");
        };
        assert_eq!(details.code, "StorageBusy");
        assert!(matches!(details.category, ErrorCategory::Storage));
        assert!(details.retryable);
    }
}

#[xmtp_common::test(unwrap_try = true)]
fn storage_invalid_path_keeps_direct_and_wrapped_causes() {
    for error in [
        client::map_wasm_storage_error(PlatformStorageError::InvalidDatabasePath),
        client::map_wasm_storage_error(StorageError::from(
            PlatformStorageError::InvalidDatabasePath,
        )),
        client::map_wasm_storage_error(StorageError::from(xmtp_db::ConnectionError::Platform(
            PlatformStorageError::InvalidDatabasePath,
        ))),
    ] {
        let XmtpError::InvalidInput(details) = error else {
            panic!("expected InvalidInput, got {error:?}");
        };
        assert_eq!(details.code, "InvalidInput");
        assert!(matches!(details.category, ErrorCategory::Input));
        assert!(!details.retryable);
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn storage_uri_create_and_build_fail_before_pool_init() {
    let options = StorageOptions::default();
    for path in ["file:unsafe.db", "sqlite://unsafe.db"] {
        assert!(matches!(
            client::open_store(&options, Some(path)).await,
            Err(XmtpError::InvalidInput(_))
        ));
        assert!(matches!(
            client::open_existing_store(&options, Path::new(path)).await,
            Err(XmtpError::InvalidInput(_))
        ));
        assert!(xmtp_db::get_sqlite().is_none());
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn storage_build_during_clear_is_busy() {
    xmtp_db::opfs_pool_capacity().await?;
    let mut clear = std::pin::pin!(xmtp_db::clear_opfs_databases());
    assert!(futures::poll!(&mut clear).is_pending());
    let options = StorageOptions::default();
    let missing = Path::new("missing.db");
    let result = client::open_existing_store(&options, missing).now_or_never();
    assert!(matches!(result, Some(Err(XmtpError::StorageBusy(_)))));
    clear.await?;
    assert!(matches!(
        client::open_existing_store(&options, missing).await,
        Err(XmtpError::IdentityNotFound(_))
    ));
    assert!(!xmtp_db::opfs_database_exists("missing.db").await?);
}
