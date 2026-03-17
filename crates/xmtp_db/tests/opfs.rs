xmtp_common::if_wasm! {
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    use xmtp_db::DbConnection;
    use xmtp_db::EncryptedMessageStore;
    use xmtp_db::identity::StoredIdentity;
    use xmtp_db::{StorageOption};
    use xmtp_db::{init_sqlite, get_sqlite, SyncOpfsUtil, init_opfs};

    pub async fn with_opfs<'a, F, R>(path: impl Into<Option<&'a str>>, f: F) -> R
    where
        F: FnOnce(xmtp_db::DefaultDbConnection) -> R,
    {
        let util = init_opfs().await;
        let o: Option<&'a str> = path.into();
        let p = o.map(String::from).unwrap_or(xmtp_common::tmp_path());
        let db = xmtp_db::database::WasmDb::new(&StorageOption::Persistent(p))
            .await
            .unwrap();
        let store = EncryptedMessageStore::new(db).unwrap();
        let conn = store.conn();
        let r = f(DbConnection::new(conn));
        if let SyncOpfsUtil(Ok(u)) = util {
            u.clear_all().await.unwrap();
        }
        r
    }

    #[allow(unused)]
    pub async fn with_opfs_async<'a, R>(
        path: impl Into<Option<&'a str>>,
        f: impl AsyncFnOnce(xmtp_db::DefaultDbConnection) -> R,
    ) -> R {
        let util = init_opfs().await;
        let o: Option<&'a str> = path.into();
        let p = o.map(String::from).unwrap_or(xmtp_common::tmp_path());
        let db = xmtp_db::database::WasmDb::new(&StorageOption::Persistent(p))
            .await
            .unwrap();
        let store = EncryptedMessageStore::new(db).unwrap();
        let conn = store.conn();
        let r = f(DbConnection::new(conn)).await;
        if let SyncOpfsUtil(Ok(u)) = util {
            u.clear_all().await.unwrap();
        }
        r
    }

    #[xmtp_common::test]
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

    #[xmtp_common::test]
    async fn opfs_dynamically_resizes() {
        use xmtp_common::tmp_path as path;
        init_sqlite().await;
        if let Some(Ok(util)) = get_sqlite() {
            util.clear_all().await.unwrap();
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
}
