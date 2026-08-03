//! Spike: does "async everywhere, synchronous body on wasm" actually work?
//!
//! The proposal is to make every `Query*` method an unconditional `async fn` and
//! let the wasm implementation keep diesel — its bodies are synchronous and
//! never await, so the futures are always immediately `Ready`. That retires
//! `maybe-async/is_sync`, which is the *global* switch forcing the two-track
//! split.
//!
//! Three things could break on wasm specifically, and none of them show up on a
//! native test:
//!
//! 1. There is no async runtime. An always-`Ready` future has to complete when
//!    driven by `wasm-bindgen-futures` alone.
//! 2. The wasm connection is a single `Rc<RefCell<SqliteConnection>>`
//!    (`database/wasm.rs:172-175`) and `raw_query` takes `borrow_mut()`, which
//!    *panics* on re-entrancy. Wrapping calls in futures introduces the
//!    possibility of interleaving that the blocking API cannot express.
//! 3. `join!` polls several futures from one task. If a storage future could be
//!    left half-done holding the borrow, this is where it would surface.

xmtp_common::if_wasm! {
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    use wasm_bindgen_test::wasm_bindgen_test;
    use xmtp_db::group::{GroupMembershipState, GroupQueryArgs, QueryGroup, StoredGroup};
    use xmtp_db::{ConnectionExt, DbConnection, EncryptedMessageStore, StorageOption, Store};
    use xmtp_proto::types::GroupId;

    /// Stand-in for what a wasm `Query*` impl becomes under the proposal: an
    /// `async fn` whose body is entirely synchronous diesel work, with no await
    /// point anywhere inside it.
    trait AsyncStore {
        async fn group_count(&self) -> usize;
        async fn add_group(&self, id: GroupId);
        /// Nested: an async storage call awaiting another async storage call.
        async fn add_then_count(&self, id: GroupId) -> usize;
    }

    impl<C: ConnectionExt> AsyncStore for DbConnection<C> {
        async fn group_count(&self) -> usize {
            self.find_groups(GroupQueryArgs::default()).unwrap().len()
        }

        async fn add_group(&self, id: GroupId) {
            StoredGroup::builder()
                .id(id)
                .created_at_ns(0)
                .membership_state(GroupMembershipState::Allowed)
                .added_by_inbox_id("spike")
                .build()
                .unwrap()
                .store(self)
                .unwrap();
        }

        async fn add_then_count(&self, id: GroupId) -> usize {
            self.add_group(id).await;
            self.group_count().await
        }
    }

    async fn ephemeral() -> xmtp_db::DefaultDbConnection {
        let db = xmtp_db::database::WasmDb::new(&StorageOption::Ephemeral)
            .await
            .unwrap();
        let store = EncryptedMessageStore::new(db).unwrap();
        DbConnection::new(store.conn())
    }

    fn gid(n: u8) -> GroupId {
        GroupId::from([n; 16])
    }

    /// (1) An await-free future completes when driven by wasm-bindgen-futures,
    /// with no runtime in the picture.
    #[wasm_bindgen_test]
    async fn await_free_futures_complete_with_no_runtime() {
        let db = ephemeral().await;
        assert_eq!(db.group_count().await, 0);
        db.add_group(gid(1)).await;
        assert_eq!(db.group_count().await, 1);
    }

    /// (2) Nested async storage calls do not re-enter the `RefCell` borrow.
    #[wasm_bindgen_test]
    async fn nested_storage_futures_do_not_re_enter_the_borrow() {
        let db = ephemeral().await;
        assert_eq!(db.add_then_count(gid(1)).await, 1);
        assert_eq!(db.add_then_count(gid(2)).await, 2);
    }

    /// (3) `join!` drives several storage futures from one task. Because the
    /// bodies never suspend, each runs to completion within a single poll and
    /// the borrow is released before the next is polled.
    #[wasm_bindgen_test]
    async fn joined_storage_futures_do_not_trip_the_refcell() {
        let db = ephemeral().await;
        db.add_group(gid(1)).await;
        db.add_group(gid(2)).await;

        let (a, b, c) = futures::join!(db.group_count(), db.group_count(), db.group_count());
        assert_eq!((a, b, c), (2, 2, 2));

        // Interleave a write among the reads.
        let (_, n) = futures::join!(db.add_group(gid(3)), db.group_count());
        assert!(n == 2 || n == 3, "poll order decides, but neither panics: {n}");
        assert_eq!(db.group_count().await, 3);
    }

    /// A sequential burst, which is what the real call paths look like: the
    /// Stage 3 census puts an inbound message at ~38 storage round trips.
    #[wasm_bindgen_test]
    async fn a_burst_of_sequential_calls_is_stable() {
        let db = ephemeral().await;
        for i in 1..=40u8 {
            db.add_group(gid(i)).await;
        }
        assert_eq!(db.group_count().await, 40);
    }
}
