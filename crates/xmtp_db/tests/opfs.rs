xmtp_common::if_wasm! {
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    use xmtp_db::DbConnection;
    use xmtp_db::EncryptedMessageStore;
    use xmtp_db::identity::StoredIdentity;
    use xmtp_db::{StorageOption};
    use xmtp_db::{init_sqlite, get_sqlite, ConnectionExt};

    pub async fn with_opfs<'a, F, R>(path: impl Into<Option<&'a str>>, f: F) -> R
    where
        F: FnOnce(xmtp_db::DefaultDbConnection) -> R,
    {
        init_sqlite().await;
        let o: Option<&'a str> = path.into();
        let p = o.map(String::from).unwrap_or(xmtp_common::tmp_path());
        let db = xmtp_db::database::WasmDb::new(&StorageOption::Persistent(p.clone()))
            .await
            .unwrap();
        let store = EncryptedMessageStore::new(db).unwrap();
        let conn = store.conn();
        let r = f(DbConnection::new(conn));
        store.release_connection().unwrap();
        xmtp_db::delete_opfs_database(&p).await.unwrap();
        r
    }

    #[allow(unused)]
    pub async fn with_opfs_async<'a, R>(
        path: impl Into<Option<&'a str>>,
        f: impl AsyncFnOnce(xmtp_db::DefaultDbConnection) -> R,
    ) -> R {
        init_sqlite().await;
        let o: Option<&'a str> = path.into();
        let p = o.map(String::from).unwrap_or(xmtp_common::tmp_path());
        let db = xmtp_db::database::WasmDb::new(&StorageOption::Persistent(p.clone()))
            .await
            .unwrap();
        let store = EncryptedMessageStore::new(db).unwrap();
        let conn = store.conn();
        let r = f(DbConnection::new(conn)).await;
        store.release_connection().unwrap();
        xmtp_db::delete_opfs_database(&p).await.unwrap();
        r
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_opfs() {
        use xmtp_db::Store;

        let path = "test_db";
        with_opfs(path, |c1| {
            let intent = StoredIdentity::builder()
                .inbox_id("test")
                .installation_keys(vec![0, 1, 1, 1])
                .credential_bytes(vec![0, 0, 0, 0])
                .next_key_package_rotation_ns(1)
                .build()
                .unwrap();
            intent.store(&c1).unwrap();
        })
        .await;
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn opfs_dynamically_resizes() {
        use xmtp_common::tmp_path as path;
        init_sqlite().await;
        if let Some(Ok(util)) = get_sqlite() {
            xmtp_db::clear_opfs_databases().await.unwrap();
            let current_capacity = util.get_capacity();
            if current_capacity > 6 {
                util.reduce_capacity(current_capacity - 6).await.unwrap();
            }
        }
        with_opfs_async(&*path(), async move |_| {
            with_opfs_async(&*path(), async move |_| {
                with_opfs_async(&*path(), async move |_| {
                    with_opfs(&*path(), |_| {
                        // should have been resized here
                        if let Some(Ok(util)) = get_sqlite() {
                            let cap = util.get_capacity();
                            assert_eq!(cap, 12);
                        } else {
                            panic!("opfs failed to init")
                        }
                    })
                    .await
                })
                .await
            })
            .await
        })
        .await
    }

    /// Whole-database import requires a closed, absent target and fences old state.
    #[xmtp_common::test(unwrap_try = true)]
    async fn opfs_restore_rotates_identity_and_fences_old_handles() {
        use xmtp_db::{WasmDb, StorageError, PlatformStorageError};
        use xmtp_db::prelude::QueryDelivery;
        use xmtp_db::delivery::DeliveryScope;
        use xmtp_db::stream_storage::StreamStorageError;

        let path = xmtp_common::tmp_path();
        let database = WasmDb::new(&StorageOption::Persistent(path.clone())).await?;
        let old = EncryptedMessageStore::new(database)?;
        let cursor = old.db().current_delivery_cursor()?;
        let owner = old.db().acquire_delivery_owner(1, 1_000)?;
        let bytes = old.db().raw_query(|conn| Ok(conn.serialize_database_to_buffer().to_vec()))?;

        assert!(matches!(xmtp_db::import_opfs_database(&path, &bytes).await,
            Err(StorageError::Platform(PlatformStorageError::DatabaseInUse))));
        assert_eq!(old.db().stream_database_id()?, cursor.database_id);
        old.release_connection()?;
        let closed = old.db().current_delivery_cursor().unwrap_err();
        assert!(closed.db_needs_connection());
        assert!(matches!(xmtp_db::import_opfs_database(&path, &bytes).await,
            Err(StorageError::Platform(PlatformStorageError::RestoreDestinationExists))));
        old.reconnect()?;
        assert_eq!(old.db().stream_database_id()?, cursor.database_id);
        old.release_connection()?;

        // Isolate import fencing from the delete helper's separate fencing.
        get_sqlite().unwrap().as_ref().unwrap().delete_db(&path)?;
        xmtp_db::import_opfs_database(&path, &bytes).await?;
        let restored = EncryptedMessageStore::new(
            WasmDb::new(&StorageOption::Persistent(path.clone())).await?,
        )?;
        assert_ne!(restored.db().stream_database_id()?, cursor.database_id);
        assert!(matches!(old.reconnect(),
            Err(xmtp_db::ConnectionError::Platform(PlatformStorageError::Replaced))));
        let replaced = old.db().current_delivery_cursor().unwrap_err();
        assert!(!replaced.db_needs_connection());
        assert!(matches!(replaced,
            StorageError::Connection(xmtp_db::ConnectionError::Platform(PlatformStorageError::Replaced))));
        assert!(matches!(restored.db().replay_delivery_messages(cursor, &DeliveryScope::All, 2, 1),
            Err(StorageError::Stream(StreamStorageError::ForeignCursor))));
        assert!(matches!(restored.db().check_delivery_owner(owner, 2),
            Err(StorageError::Stream(StreamStorageError::NotCurrentOwner))));
        restored.release_connection()?;
        xmtp_db::delete_opfs_database(&path).await?;
    }

    /// Failed validation and an existing destination leave the original file unchanged.
    #[xmtp_common::test(unwrap_try = true)]
    async fn opfs_failed_restore_preserves_existing_data() {
        use diesel::connection::SimpleConnection;
        use xmtp_db::{WasmDb, StorageError, PlatformStorageError, XmtpDb};

        let path = xmtp_common::tmp_path();
        let existing = EncryptedMessageStore::new(
            WasmDb::new(&StorageOption::Persistent(path.clone())).await?,
        )?;
        let original = existing.db().raw_query(|conn| Ok(conn.serialize_database_to_buffer().to_vec()))?;
        existing.release_connection()?;
        let before = get_sqlite().unwrap().as_ref().unwrap().export_db(&path)?;

        assert!(matches!(xmtp_db::import_opfs_database(&path, b"invalid").await,
            Err(StorageError::Platform(PlatformStorageError::InvalidRestoreInput))));
        let unrelated = WasmDb::new(&StorageOption::Ephemeral).await?;
        unrelated.db().raw_query(|conn| conn.batch_execute("CREATE TABLE unrelated (id INTEGER);"))?;
        let unrelated_bytes = unrelated.db().raw_query(|conn| Ok(conn.serialize_database_to_buffer().to_vec()))?;
        let damaged = EncryptedMessageStore::new(WasmDb::new(&StorageOption::Ephemeral).await?)?;
        damaged.db().raw_query(|conn| conn.batch_execute(
            "PRAGMA writable_schema = ON; UPDATE sqlite_schema SET rootpage = 2147483647 WHERE name = 'group_messages'; PRAGMA writable_schema = OFF;",
        ))?;
        let damaged_bytes = damaged.db().raw_query(|conn| Ok(conn.serialize_database_to_buffer().to_vec()))?;
        let rejected_path = xmtp_common::tmp_path();
        for rejected in [&unrelated_bytes, &damaged_bytes] {
            assert!(xmtp_db::import_opfs_database(&rejected_path, rejected).await.is_err());
            assert!(!get_sqlite().unwrap().as_ref().unwrap().exists(&rejected_path)?);
        }
        assert!(matches!(xmtp_db::import_opfs_database(&path, &original).await,
            Err(StorageError::Platform(PlatformStorageError::RestoreDestinationExists))));
        assert_eq!(get_sqlite().unwrap().as_ref().unwrap().export_db(&path)?, before);
        existing.reconnect()?;
        existing.release_connection()?;
        xmtp_db::delete_opfs_database(&path).await?;
    }
}
