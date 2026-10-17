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

    /// A pause from an ending client must not take the pool from an open that
    /// has unpaused it and waits on a resize before its file is open.
    #[xmtp_common::test(unwrap_try = true)]
    async fn pause_waits_for_a_pending_open() {
        use futures::FutureExt;
        use xmtp_db::WasmDb;

        xmtp_db::try_init_sqlite().await.expect("OPFS install");
        let util = get_sqlite().expect("OPFS cell").expect("OPFS util");
        xmtp_db::clear_opfs_databases().await?;
        let capacity = util.get_capacity();
        // With one slot and no used files, the next open must grow the pool.
        util.reduce_capacity(capacity - 1).await?;
        let path = xmtp_common::tmp_path();
        let location = StorageOption::Persistent(path.clone());
        let mut open = std::pin::pin!(WasmDb::new(&location));
        assert!(open.as_mut().now_or_never().is_none(), "the open did not wait on the resize");

        xmtp_db::pause_sqlite_if_idle();
        let store = EncryptedMessageStore::new(open.await?)?;

        assert!(!util.is_paused(), "the pause took the pool from a pending open");
        assert!(util.exists(&path)?, "the open did not create its OPFS file");
        store.release_connection()?;
        xmtp_db::delete_opfs_database(&path).await?;
    }

    /// A caller can unpause the pool before `WasmDb::new`, and an ending client
    /// can pause it again before the open starts or while the open unpauses it.
    /// The open unpauses the pool itself, so it still opens its file.
    #[xmtp_common::test(unwrap_try = true)]
    async fn open_unpauses_a_pool_paused_before_it() {
        use futures::FutureExt;
        use xmtp_db::WasmDb;

        xmtp_db::try_init_sqlite().await.expect("OPFS install");
        let util = get_sqlite().expect("OPFS cell").expect("OPFS util");
        xmtp_db::pause_sqlite_if_idle();
        assert!(util.is_paused(), "an idle pool did not pause");
        let path = xmtp_common::tmp_path();
        let location = StorageOption::Persistent(path.clone());
        let mut open = std::pin::pin!(WasmDb::new(&location));
        assert!(open.as_mut().now_or_never().is_none(), "the open did not wait on the unpause");

        xmtp_db::pause_sqlite_if_idle();
        let store = EncryptedMessageStore::new(open.await?)?;

        assert!(!util.is_paused(), "the open left the pool paused");
        assert!(util.exists(&path)?, "the open did not create its OPFS file");
        store.release_connection()?;
        xmtp_db::delete_opfs_database(&path).await?;
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

    fn is_busy<T>(result: Result<T, xmtp_db::StorageError>) -> bool {
        matches!(result, Err(xmtp_db::StorageError::Platform(xmtp_db::PlatformStorageError::DatabaseInUse)))
    }

    /// The pinned VFS cannot count duplicate opens. Reject a second live
    /// connection before SQLite opens it, but allow a later closed-path reopen.
    #[xmtp_common::test(unwrap_try = true)]
    async fn opfs_same_path_requires_exclusive_live_connection() {
        use xmtp_db::{WasmDb, WasmDbConnection, PlatformStorageError, ConnectionError, XmtpDb};
        use xmtp_db::prelude::QueryDelivery;

        let path = xmtp_common::tmp_path();
        let location = StorageOption::Persistent(path.clone());
        let first = EncryptedMessageStore::new(WasmDb::new_strict(&location).await?)?;
        let identity = first.db().stream_database_id()?;
        let rejected = [
            matches!(WasmDb::new_strict(&location).await, Err(PlatformStorageError::DatabaseInUse)),
            matches!(WasmDbConnection::new(&path), Err(PlatformStorageError::DatabaseInUse)),
        ];
        assert_eq!(rejected, [true; 2], "both opens must reject a live path before touching the VFS");
        assert_eq!(first.db().stream_database_id()?, identity);
        first.release_connection()?;

        let database = WasmDb::new_strict(&location).await?;
        let shared = database.clone();
        let second = EncryptedMessageStore::new(database)?;
        assert!(matches!(first.reconnect(), Err(ConnectionError::Platform(PlatformStorageError::DatabaseInUse))));
        // Closing the old object again must not close the new connection.
        first.release_connection()?;
        xmtp_db::pause_sqlite_if_idle();
        assert!(!get_sqlite().unwrap().unwrap().is_paused());
        assert_eq!(second.db().stream_database_id()?, identity);
        assert_eq!(shared.db().stream_database_id()?, identity);
        let rejected_changes = [
            is_busy(xmtp_db::clear_opfs_databases().await),
            is_busy(xmtp_db::export_opfs_database(&path).await),
            is_busy(xmtp_db::delete_opfs_database(&path).await),
        ];
        assert_eq!(rejected_changes, [true; 3]);
        second.release_connection()?;
        xmtp_db::pause_sqlite_if_idle();
        assert!(get_sqlite().unwrap().unwrap().is_paused());
        xmtp_db::try_init_sqlite().await?;
        first.reconnect()?;
        assert_eq!(first.db().stream_database_id()?, identity);
        first.release_connection()?;
        xmtp_db::delete_opfs_database(&path).await?;
    }

    /// SQLite URIs can alias the same VFS path. Reject both URI forms before
    /// initializing OPFS or opening a persistent connection.
    #[xmtp_common::test(unwrap_try = true)]
    async fn opfs_persistent_paths_reject_sqlite_uris() {
        use xmtp_db::{WasmDb, WasmDbConnection, PlatformStorageError};

        assert!(get_sqlite().is_none());
        let rejected = [
            matches!(WasmDbConnection::new("file:uri-path"), Err(PlatformStorageError::InvalidDatabasePath)),
            matches!(WasmDbConnection::new("sqlite://uri-path"), Err(PlatformStorageError::InvalidDatabasePath)),
            matches!(WasmDb::new_strict(&StorageOption::Persistent("file:uri-path".into())).await, Err(PlatformStorageError::InvalidDatabasePath)),
            matches!(WasmDb::new_strict(&StorageOption::Persistent("sqlite://uri-path".into())).await, Err(PlatformStorageError::InvalidDatabasePath)),
        ];
        assert_eq!(rejected, [true; 4]);
        assert!(get_sqlite().is_none(), "invalid paths must not initialize OPFS");
    }

    /// Two reads must share the real VFS resume, including its access handles.
    #[xmtp_common::test(unwrap_try = true)]
    async fn opfs_reads_share_resume() {
        use futures::FutureExt;
        use xmtp_db::WasmDb;

        let path = xmtp_common::tmp_path();
        let store = EncryptedMessageStore::new(WasmDb::new_strict(&StorageOption::Persistent(path.clone())).await?)?;
        store.release_connection()?;
        let expected = xmtp_db::list_opfs_databases().await?;
        xmtp_db::pause_sqlite_if_idle();
        assert!(get_sqlite().unwrap().unwrap().is_paused());
        let mut list = std::pin::pin!(xmtp_db::list_opfs_databases());
        assert!(list.as_mut().now_or_never().is_none(), "read must wait on real VFS resume");
        let mut count = std::pin::pin!(xmtp_db::opfs_database_count());
        assert!(count.as_mut().now_or_never().is_none());
        let (list, count) = futures::join!(list, count);
        assert_eq!(list?, expected);
        assert_eq!(count?, expected.len() as u32);
        xmtp_db::delete_opfs_database(&path).await?;
    }

    /// A read and a persistent open must share resume before either uses the maps.
    #[xmtp_common::test(unwrap_try = true)]
    async fn opfs_read_and_open_share_resume() {
        use futures::FutureExt;
        use xmtp_db::WasmDb;

        xmtp_db::try_init_sqlite().await?;
        xmtp_db::clear_opfs_databases().await?;
        xmtp_db::pause_sqlite_if_idle();
        let mut list = std::pin::pin!(xmtp_db::list_opfs_databases());
        assert!(list.as_mut().now_or_never().is_none());
        let path = xmtp_common::tmp_path();
        let location = StorageOption::Persistent(path.clone());
        let mut open = std::pin::pin!(WasmDb::new_strict(&location));
        assert!(open.as_mut().now_or_never().is_none());
        let (list, open) = futures::join!(list, open);
        assert!(list?.is_empty());
        let store = EncryptedMessageStore::new(open?)?;
        assert!(xmtp_db::opfs_database_exists(&path).await?);
        store.release_connection()?;
        xmtp_db::delete_opfs_database(&path).await?;
    }

    /// Clear releases its maps before awaiting new handles. Every public route
    /// must reject admission during that interval, and idle pause must skip it.
    #[xmtp_common::test(unwrap_try = true)]
    async fn opfs_clear_excludes_reads_and_opens() {
        use futures::FutureExt;
        use xmtp_db::{WasmDb, WasmDbConnection, PlatformStorageError, ConnectionError};

        let path = xmtp_common::tmp_path();
        let store = EncryptedMessageStore::new(WasmDb::new_strict(&StorageOption::Persistent(path.clone())).await?)?;
        store.release_connection()?;
        let util = get_sqlite().unwrap().unwrap();
        let mut clear = std::pin::pin!(xmtp_db::clear_opfs_databases());
        assert!(clear.as_mut().now_or_never().is_none(), "clear must wait on real handle acquisition");
        assert_eq!(util.count(), 0, "test must stop after VFS clears the map");
        let rejected = [
            xmtp_db::list_opfs_databases().now_or_never().is_some_and(is_busy),
            xmtp_db::opfs_database_count().now_or_never().is_some_and(is_busy),
            xmtp_db::opfs_pool_capacity().now_or_never().is_some_and(is_busy),
            xmtp_db::opfs_database_exists(&path).now_or_never().is_some_and(is_busy),
            xmtp_db::export_opfs_database(&path).now_or_never().is_some_and(is_busy),
        ];
        assert_eq!(rejected, [true; 5], "list/count/capacity/exists/export must all return busy");
        let new_path = xmtp_common::tmp_path();
        let location = StorageOption::Persistent(new_path.clone());
        assert!(matches!(WasmDb::new_strict(&location).now_or_never(), Some(Err(PlatformStorageError::DatabaseInUse))));
        assert!(matches!(WasmDbConnection::new(&new_path), Err(PlatformStorageError::DatabaseInUse)));
        assert!(matches!(store.reconnect(), Err(ConnectionError::Platform(PlatformStorageError::DatabaseInUse))));
        xmtp_db::pause_sqlite_if_idle();
        assert!(!util.is_paused(), "pause must not interrupt clear");
        clear.await?;
        assert!(xmtp_db::list_opfs_databases().await?.is_empty());
        assert!(!xmtp_db::opfs_database_exists(&new_path).await?);
        assert!(matches!(store.reconnect(), Err(ConnectionError::Platform(PlatformStorageError::Replaced))));
    }

    /// A utility reserved before resume excludes mutations until its result is ready.
    #[xmtp_common::test(unwrap_try = true)]
    async fn opfs_resuming_read_excludes_mutations() {
        use futures::FutureExt;
        use xmtp_db::WasmDb;

        let path = xmtp_common::tmp_path();
        let store = EncryptedMessageStore::new(WasmDb::new_strict(&StorageOption::Persistent(path.clone())).await?)?;
        store.release_connection()?;
        let before = xmtp_db::export_opfs_database(&path).await?;
        xmtp_db::pause_sqlite_if_idle();
        let mut read = std::pin::pin!(xmtp_db::list_opfs_databases());
        assert!(read.as_mut().now_or_never().is_none());
        let rejected = [
            xmtp_db::delete_opfs_database(&path).now_or_never().is_some_and(is_busy),
            xmtp_db::clear_opfs_databases().now_or_never().is_some_and(is_busy),
            xmtp_db::import_opfs_database(&path, &before).now_or_never().is_some_and(is_busy),
        ];
        assert_eq!(rejected, [true; 3]);
        assert!(read.await?.contains(&path));
        assert_eq!(xmtp_db::export_opfs_database(&path).await?, before);
        xmtp_db::delete_opfs_database(&path).await?;
    }

    /// PendingOpen is reserved before resize and prevents each mutation route.
    #[xmtp_common::test(unwrap_try = true)]
    async fn opfs_resizing_open_excludes_mutations() {
        use futures::FutureExt;
        use xmtp_db::{WasmDb, WasmDbConnection, PlatformStorageError, ConnectionError};

        xmtp_db::try_init_sqlite().await?;
        xmtp_db::clear_opfs_databases().await?;
        let util = get_sqlite().unwrap().unwrap();
        let old_path = xmtp_common::tmp_path();
        let old = EncryptedMessageStore::new(WasmDb::new_strict(&StorageOption::Persistent(old_path.clone())).await?)?;
        old.release_connection()?;
        util.reduce_capacity(util.get_capacity() - 2).await?;
        let path = xmtp_common::tmp_path();
        let location = StorageOption::Persistent(path.clone());
        let mut open = std::pin::pin!(WasmDb::new_strict(&location));
        assert!(open.as_mut().now_or_never().is_none());
        let rejected = [
            xmtp_db::delete_opfs_database(&path).now_or_never().is_some_and(is_busy),
            xmtp_db::clear_opfs_databases().now_or_never().is_some_and(is_busy),
            xmtp_db::import_opfs_database(&path, b"invalid").now_or_never().is_some_and(is_busy),
        ];
        assert_eq!(rejected, [true; 3]);
        assert!(matches!(WasmDbConnection::new(&path), Err(PlatformStorageError::DatabaseInUse)));
        assert!(matches!(old.reconnect(), Err(ConnectionError::Platform(PlatformStorageError::DatabaseInUse))));
        let store = EncryptedMessageStore::new(open.await?)?;
        assert!(xmtp_db::opfs_database_exists(&path).await?);
        store.release_connection()?;
        xmtp_db::delete_opfs_database(&path).await?;
        old.reconnect()?;
        old.release_connection()?;
        xmtp_db::delete_opfs_database(&old_path).await?;
    }

    /// Export rejects open files, preserves a closed file and identity, and does
    /// not fence reconnect or create a missing destination.
    #[xmtp_common::test(unwrap_try = true)]
    async fn opfs_export_requires_closed_target_and_preserves_identity() {
        use xmtp_db::WasmDb;
        use xmtp_db::prelude::QueryDelivery;

        let path = xmtp_common::tmp_path();
        let store = EncryptedMessageStore::new(WasmDb::new_strict(&StorageOption::Persistent(path.clone())).await?)?;
        let database_id = store.db().stream_database_id()?;
        assert!(is_busy(xmtp_db::export_opfs_database(&path).await));
        store.release_connection()?;
        let util = get_sqlite().unwrap().unwrap();
        let before = util.export_db(&path)?;
        assert_eq!(xmtp_db::export_opfs_database(&path).await?, before);
        assert_eq!(util.export_db(&path)?, before);
        store.reconnect()?;
        assert_eq!(store.db().stream_database_id()?, database_id);
        store.release_connection()?;
        let absent = xmtp_common::tmp_path();
        assert!(xmtp_db::export_opfs_database(&absent).await.is_err());
        assert!(!xmtp_db::opfs_database_exists(&absent).await?);
        xmtp_db::delete_opfs_database(&path).await?;
    }

    /// Clear checks all handles before it changes any closed file.
    #[xmtp_common::test(unwrap_try = true)]
    async fn opfs_failed_clear_preserves_all_files() {
        use xmtp_db::WasmDb;

        let mut stores = Vec::new();
        for _ in 0..3 {
            let path = xmtp_common::tmp_path();
            let store = EncryptedMessageStore::new(WasmDb::new_strict(&StorageOption::Persistent(path.clone())).await?)?;
            stores.push((path, store));
        }
        stores[0].1.release_connection()?;
        stores[1].1.release_connection()?;
        let util = get_sqlite().unwrap().unwrap();
        let mut before_names = util.list();
        before_names.sort();
        let before_bytes: Vec<_> = stores.iter().map(|(path, _)| util.export_db(path).unwrap()).collect();
        assert!(is_busy(xmtp_db::clear_opfs_databases().await));
        let mut after_names = util.list();
        after_names.sort();
        assert_eq!(after_names, before_names);
        for ((path, _), before) in stores.iter().zip(before_bytes) {
            assert_eq!(util.export_db(path)?, before);
        }
        for (path, store) in stores {
            store.reconnect()?;
            store.release_connection()?;
            xmtp_db::delete_opfs_database(&path).await?;
        }
    }

    /// Cancelling an install cannot make the same worker safe to retry.
    #[xmtp_common::test(unwrap_try = true)]
    async fn opfs_cancelled_install_requires_worker_restart() {
        use futures::FutureExt;

        assert!(get_sqlite().is_none());
        let mut install = Box::pin(xmtp_db::try_init_sqlite());
        assert!(install.as_mut().now_or_never().is_none());
        drop(install);
        assert!(xmtp_db::opfs_requires_worker_restart());
        assert!(matches!(xmtp_db::try_init_sqlite().await, Err(xmtp_db::PlatformStorageError::PoolUnusable)));
    }

    /// Cancelling resume must fence both async utilities and synchronous opens.
    #[xmtp_common::test(unwrap_try = true)]
    async fn opfs_cancelled_resume_requires_worker_restart() {
        use futures::FutureExt;
        use xmtp_db::{PlatformStorageError, StorageError, WasmDbConnection};

        xmtp_db::try_init_sqlite().await?;
        xmtp_db::pause_sqlite_if_idle();
        let mut read = Box::pin(xmtp_db::list_opfs_databases());
        assert!(read.as_mut().now_or_never().is_none());
        drop(read);
        assert!(xmtp_db::opfs_requires_worker_restart());
        assert!(matches!(xmtp_db::list_opfs_databases().await, Err(StorageError::Platform(PlatformStorageError::PoolUnusable))));
        assert!(matches!(WasmDbConnection::new("after-cancel"), Err(PlatformStorageError::PoolUnusable)));
    }

    async fn occupy_access_handles() -> sqlite_wasm_vfs::sahpool::OpfsSAHPoolUtil {
        let cfg = sqlite_wasm_vfs::sahpool::OpfsSAHPoolCfg {
            vfs_name: "test-conflicting-pool".into(),
            directory: xmtp_configuration::WASM_VFS_DIRECTORY.into(),
            clear_on_init: false,
            initial_capacity: 6,
        };
        sqlite_wasm_vfs::sahpool::install::<sqlite_wasm_rs::WasmOsCallback>(&cfg, false).await.unwrap()
    }

    /// Existing callers keep the old fallback. When another owner holds the
    /// OPFS pool, the default open logs the failure and opens on SQLite's
    /// default VFS. A strict open in that worker returns a typed error.
    #[xmtp_common::test(unwrap_try = true)]
    async fn default_open_falls_back_when_another_owner_holds_the_pool() {
        use xmtp_db::{PlatformStorageError, WasmDb};
        use xmtp_db::prelude::QueryDelivery;

        let blocker = occupy_access_handles().await;
        let path = xmtp_common::tmp_path();
        let location = StorageOption::Persistent(path.clone());
        let store = EncryptedMessageStore::new(WasmDb::new(&location).await?)?;
        let identity = store.db().stream_database_id()?;
        store.release_connection()?;
        store.reconnect()?;
        assert_eq!(store.db().stream_database_id()?, identity);
        assert!(!blocker.exists(&path)?, "the fallback database used the held pool");
        assert!(matches!(
            WasmDb::new_strict(&StorageOption::Persistent(xmtp_common::tmp_path())).await,
            Err(PlatformStorageError::SAH(_))
        ));
        store.release_connection()?;
        blocker.pause_vfs()?;
    }

    /// A legacy install that fails while another owner holds the pool stays
    /// retryable in this worker. It uses the install that the legacy browser
    /// OPFS functions call. After the owner releases the pool, the next
    /// legacy install and open use it.
    #[xmtp_common::test(unwrap_try = true)]
    async fn legacy_install_retries_after_the_owner_releases_the_pool() {
        use xmtp_db::WasmDb;

        let blocker = occupy_access_handles().await;
        xmtp_db::database::init_sqlite().await;
        assert!(!matches!(get_sqlite(), Some(Ok(_))));
        let fallback = xmtp_common::tmp_path();
        let store = EncryptedMessageStore::new(WasmDb::new(&StorageOption::Persistent(fallback)).await?)?;
        store.release_connection()?;
        assert!(!xmtp_db::opfs_requires_worker_restart());
        blocker.pause_vfs()?;

        xmtp_db::database::init_sqlite().await;
        let Some(Ok(util)) = get_sqlite() else {
            panic!("legacy install did not retry");
        };
        let path = xmtp_common::tmp_path();
        let store = EncryptedMessageStore::new(WasmDb::new(&StorageOption::Persistent(path.clone())).await?)?;
        assert!(util.exists(&path)?, "the legacy open did not use the pool");
        store.release_connection()?;
        xmtp_db::delete_opfs_database(&path).await?;
    }

    /// A real SAH conflict returns its typed cause. Even after that conflict
    /// clears, partial installation requires a fresh worker.
    #[xmtp_common::test(unwrap_try = true)]
    async fn opfs_failed_install_requires_worker_restart() {
        use xmtp_db::{PlatformStorageError, OpfsSAHError, WasmDb};

        let blocker = occupy_access_handles().await;
        let result = WasmDb::new_strict(&StorageOption::Persistent("blocked-install".into())).await;
        assert!(matches!(result, Err(PlatformStorageError::SAH(OpfsSAHError::CreateSyncAccessHandle(_)))));
        blocker.pause_vfs()?;
        assert!(xmtp_db::opfs_requires_worker_restart());
        assert!(matches!(xmtp_db::try_init_sqlite().await, Err(PlatformStorageError::PoolUnusable)));
    }

    /// An installed OnceCell cannot make a failed resume reusable.
    #[xmtp_common::test(unwrap_try = true)]
    async fn opfs_failed_resume_requires_worker_restart() {
        use xmtp_db::{PlatformStorageError, StorageError, OpfsSAHError};

        xmtp_db::try_init_sqlite().await?;
        xmtp_db::pause_sqlite_if_idle();
        let blocker = occupy_access_handles().await;
        assert!(matches!(xmtp_db::list_opfs_databases().await, Err(StorageError::Platform(PlatformStorageError::SAH(OpfsSAHError::CreateSyncAccessHandle(_))))));
        blocker.pause_vfs()?;
        assert!(xmtp_db::opfs_requires_worker_restart());
        assert!(matches!(xmtp_db::try_init_sqlite().await, Err(PlatformStorageError::PoolUnusable)));
    }
}
