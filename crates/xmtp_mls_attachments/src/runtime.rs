//! The attachment runtime: the local store, lease timing, shared per-attachment state,
//! and the reconcile and sweep of stored attachments.

use super::*;

const DEFAULT_MAX_PENDING_AGE: Duration = Duration::from_secs(86_400);
const RECONCILE_AGE: Duration = Duration::from_secs(3_600);
pub(super) const LEASE_DURATION: Duration = Duration::from_secs(120);
pub(super) const LEASE_RENEW: Duration = Duration::from_secs(30);
const LEASE_POLL: Duration = Duration::from_secs(1);
#[cfg(any(test, feature = "test-hooks"))]
pub(super) static FAIL_NEXT_RECONCILES: AtomicUsize = AtomicUsize::new(0);
static PUBLICATION_LOCKS: LazyLock<Mutex<HashMap<PathBuf, Weak<AsyncMutex<()>>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
fn shared_publication_lock(dir: Option<&PathBuf>) -> Arc<AsyncMutex<()>> {
    let Some(dir) = dir else {
        return Arc::new(AsyncMutex::new(()));
    };
    let path = dir.clone();
    let mut locks = PUBLICATION_LOCKS.lock();
    if let Some(lock) = locks.get(&path).and_then(Weak::upgrade) {
        return lock;
    }
    locks.retain(|_, lock| lock.strong_count() != 0);
    let lock = Arc::new(AsyncMutex::new(()));
    locks.insert(path, Arc::downgrade(&lock));
    lock
}
#[derive(Clone, Copy)]
pub(super) struct LeaseTiming {
    duration: Duration,
    pub(super) renew: Duration,
    pub(super) poll: Duration,
}
impl Default for LeaseTiming {
    fn default() -> Self {
        Self {
            duration: LEASE_DURATION,
            renew: LEASE_RENEW,
            poll: LEASE_POLL,
        }
    }
}
impl LeaseTiming {
    pub(super) fn duration_ns(self) -> i64 {
        self.duration.as_nanos().min(i64::MAX as u128) as i64
    }

    #[cfg(all(any(test, feature = "test-hooks"), not(target_arch = "wasm32")))]
    pub(super) fn for_test(duration: Duration, renew: Duration, poll: Duration) -> Self {
        Self {
            duration,
            renew,
            poll,
        }
    }
}
#[doc(hidden)]
pub struct AttachmentRuntime {
    pub(crate) store: Option<Arc<dyn LocalStore>>,
    pub(crate) dir: Option<PathBuf>,
    pub(crate) options: AttachmentOptions,
    pub(super) pending: Mutex<HashMap<String, Weak<PendingShared>>>,
    pub(super) downloads: Mutex<HashMap<String, Arc<DownloadShared>>>,
    pub(super) deleting: Mutex<HashMap<String, usize>>,
    pub(super) event_locks: Mutex<HashMap<String, Weak<AsyncMutex<()>>>>,
    pub(super) publication_lock: Arc<AsyncMutex<()>>,
    reconciled: OnceCell<()>,
    pub(super) lease_timing: Mutex<LeaseTiming>,
    #[cfg(any(test, feature = "test-hooks"))]
    pub(super) sweep_pause: Mutex<Option<(Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)>>,
    #[cfg(any(test, feature = "test-hooks"))]
    pub(super) delete_pause: Mutex<Option<(Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)>>,
    #[cfg(all(any(test, feature = "test-hooks"), not(target_arch = "wasm32")))]
    pub(super) outcome_lock_pause:
        Mutex<Option<(Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)>>,
    #[cfg(all(any(test, feature = "test-hooks"), not(target_arch = "wasm32")))]
    pub(super) fail_next_staged_removal: AtomicBool,
    #[cfg(all(any(test, feature = "test-hooks"), not(target_arch = "wasm32")))]
    pub(super) fail_next_download_cleanup: AtomicBool,
    #[cfg(all(any(test, feature = "test-hooks"), not(target_arch = "wasm32")))]
    pub(super) create_move_pause: Mutex<Option<Arc<test_hooks::CreateMovePause>>>,
    #[cfg(all(any(test, feature = "test-hooks"), not(target_arch = "wasm32")))]
    pub(super) create_publish_pause: Mutex<Option<Arc<test_hooks::CreatePublishPause>>>,
    #[cfg(all(any(test, feature = "test-hooks"), not(target_arch = "wasm32")))]
    pub(super) attempt_panic_pause:
        Mutex<Option<(Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)>>,
    #[cfg(all(any(test, feature = "test-hooks"), not(target_arch = "wasm32")))]
    pub(super) download_panic_pause:
        Mutex<Option<(Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)>>,
    #[cfg(all(any(test, feature = "test-hooks"), not(target_arch = "wasm32")))]
    pub(super) download_move_pause:
        Mutex<Option<(Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)>>,
    #[cfg(all(any(test, feature = "test-hooks"), not(target_arch = "wasm32")))]
    pub(super) download_publish_pause:
        Mutex<Option<(Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)>>,
    #[cfg(all(any(test, feature = "test-hooks"), not(target_arch = "wasm32")))]
    pub(super) download_completion_pause:
        Mutex<Option<(Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)>>,
    #[cfg(all(any(test, feature = "test-hooks"), not(target_arch = "wasm32")))]
    pub(super) download_failure_pause:
        Mutex<Option<(Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)>>,
    #[cfg(all(any(test, feature = "test-hooks"), not(target_arch = "wasm32")))]
    pub(super) download_existing_pause:
        Mutex<Option<(Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)>>,
    #[cfg(all(any(test, feature = "test-hooks"), not(target_arch = "wasm32")))]
    pub(super) reconcile_snapshot_pause:
        Mutex<Option<(Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)>>,
    #[cfg(any(test, feature = "test-hooks"))]
    pub(super) outcome_write_errors: AtomicUsize,
    #[cfg(any(test, feature = "test-hooks"))]
    pub(super) lease_extension_errors: AtomicUsize,
}
impl Default for AttachmentRuntime {
    fn default() -> Self {
        Self {
            store: None,
            dir: None,
            options: AttachmentOptions::default(),
            pending: Mutex::new(HashMap::new()),
            downloads: Mutex::new(HashMap::new()),
            deleting: Mutex::new(HashMap::new()),
            event_locks: Mutex::new(HashMap::new()),
            publication_lock: Arc::new(AsyncMutex::new(())),
            reconciled: OnceCell::new(),
            lease_timing: Mutex::new(LeaseTiming::default()),
            #[cfg(any(test, feature = "test-hooks"))]
            sweep_pause: Mutex::new(None),
            #[cfg(any(test, feature = "test-hooks"))]
            delete_pause: Mutex::new(None),
            #[cfg(all(any(test, feature = "test-hooks"), not(target_arch = "wasm32")))]
            outcome_lock_pause: Mutex::new(None),
            #[cfg(all(any(test, feature = "test-hooks"), not(target_arch = "wasm32")))]
            fail_next_staged_removal: AtomicBool::new(false),
            #[cfg(all(any(test, feature = "test-hooks"), not(target_arch = "wasm32")))]
            fail_next_download_cleanup: AtomicBool::new(false),
            #[cfg(all(any(test, feature = "test-hooks"), not(target_arch = "wasm32")))]
            create_move_pause: Mutex::new(None),
            #[cfg(all(any(test, feature = "test-hooks"), not(target_arch = "wasm32")))]
            create_publish_pause: Mutex::new(None),
            #[cfg(all(any(test, feature = "test-hooks"), not(target_arch = "wasm32")))]
            attempt_panic_pause: Mutex::new(None),
            #[cfg(all(any(test, feature = "test-hooks"), not(target_arch = "wasm32")))]
            download_panic_pause: Mutex::new(None),
            #[cfg(all(any(test, feature = "test-hooks"), not(target_arch = "wasm32")))]
            download_move_pause: Mutex::new(None),
            #[cfg(all(any(test, feature = "test-hooks"), not(target_arch = "wasm32")))]
            download_publish_pause: Mutex::new(None),
            #[cfg(all(any(test, feature = "test-hooks"), not(target_arch = "wasm32")))]
            download_completion_pause: Mutex::new(None),
            #[cfg(all(any(test, feature = "test-hooks"), not(target_arch = "wasm32")))]
            download_failure_pause: Mutex::new(None),
            #[cfg(all(any(test, feature = "test-hooks"), not(target_arch = "wasm32")))]
            download_existing_pause: Mutex::new(None),
            #[cfg(all(any(test, feature = "test-hooks"), not(target_arch = "wasm32")))]
            reconcile_snapshot_pause: Mutex::new(None),
            #[cfg(any(test, feature = "test-hooks"))]
            outcome_write_errors: AtomicUsize::new(0),
            #[cfg(any(test, feature = "test-hooks"))]
            lease_extension_errors: AtomicUsize::new(0),
        }
    }
}
impl AttachmentRuntime {
    /// Whether this runtime keeps attachment files in a local store.
    pub fn has_store(&self) -> bool {
        self.store.is_some()
    }

    /// The directory attachment files are stored under, if any.
    #[cfg(any(test, feature = "test-utils"))]
    #[doc(hidden)]
    pub fn dir(&self) -> Option<&PathBuf> {
        self.dir.as_ref()
    }

    /// The options uploads and downloads run with.
    #[cfg(any(test, feature = "test-utils"))]
    #[doc(hidden)]
    pub fn options(&self) -> &AttachmentOptions {
        &self.options
    }

    /// Reconcile stored attachments with their local files once per runtime.
    /// Later calls return the first run's outcome.
    pub async fn ensure_reconciled<Context: AttachmentContext>(
        &self,
        context: &Context,
    ) -> Result<(), AttachmentClientError> {
        self.reconciled
            .get_or_try_init(|| self.reconcile(context))
            .await
            .map(|_| ())
    }

    /// A runtime that stores attachment files under `dir`, or keeps no
    /// local files when it is `None`.
    pub async fn new(
        dir: Option<PathBuf>,
        options: AttachmentOptions,
    ) -> Result<Self, AttachmentClientError> {
        let store: Option<Arc<dyn LocalStore>> = match dir.as_ref() {
            None => None,
            #[cfg(not(target_arch = "wasm32"))]
            Some(path) => Some(Arc::new(xmtp_attachments::NativeStore::new(path).await?)),
            #[cfg(target_arch = "wasm32")]
            Some(path) => Some(Arc::new(
                xmtp_attachments::OpfsStore::new(&path.to_string_lossy()).await?,
            )),
        };
        #[cfg(not(target_arch = "wasm32"))]
        let lock_path = match dir.as_ref() {
            Some(path) => Some(
                tokio::fs::canonicalize(path)
                    .await
                    .map_err(|_| AttachmentClientError::new(Cause::LocalStorage))?,
            ),
            None => None,
        };
        #[cfg(target_arch = "wasm32")]
        let lock_path = dir.clone();
        let publication_lock = shared_publication_lock(lock_path.as_ref());
        Ok(Self {
            store,
            dir,
            options,
            pending: Mutex::new(HashMap::new()),
            downloads: Mutex::new(HashMap::new()),
            deleting: Mutex::new(HashMap::new()),
            event_locks: Mutex::new(HashMap::new()),
            publication_lock,
            reconciled: OnceCell::new(),
            lease_timing: Mutex::new(LeaseTiming::default()),
            #[cfg(any(test, feature = "test-hooks"))]
            sweep_pause: Mutex::new(None),
            #[cfg(any(test, feature = "test-hooks"))]
            delete_pause: Mutex::new(None),
            #[cfg(all(any(test, feature = "test-hooks"), not(target_arch = "wasm32")))]
            outcome_lock_pause: Mutex::new(None),
            #[cfg(all(any(test, feature = "test-hooks"), not(target_arch = "wasm32")))]
            fail_next_staged_removal: AtomicBool::new(false),
            #[cfg(all(any(test, feature = "test-hooks"), not(target_arch = "wasm32")))]
            fail_next_download_cleanup: AtomicBool::new(false),
            #[cfg(all(any(test, feature = "test-hooks"), not(target_arch = "wasm32")))]
            create_move_pause: Mutex::new(None),
            #[cfg(all(any(test, feature = "test-hooks"), not(target_arch = "wasm32")))]
            create_publish_pause: Mutex::new(None),
            #[cfg(all(any(test, feature = "test-hooks"), not(target_arch = "wasm32")))]
            attempt_panic_pause: Mutex::new(None),
            #[cfg(all(any(test, feature = "test-hooks"), not(target_arch = "wasm32")))]
            download_panic_pause: Mutex::new(None),
            #[cfg(all(any(test, feature = "test-hooks"), not(target_arch = "wasm32")))]
            download_move_pause: Mutex::new(None),
            #[cfg(all(any(test, feature = "test-hooks"), not(target_arch = "wasm32")))]
            download_publish_pause: Mutex::new(None),
            #[cfg(all(any(test, feature = "test-hooks"), not(target_arch = "wasm32")))]
            download_completion_pause: Mutex::new(None),
            #[cfg(all(any(test, feature = "test-hooks"), not(target_arch = "wasm32")))]
            download_failure_pause: Mutex::new(None),
            #[cfg(all(any(test, feature = "test-hooks"), not(target_arch = "wasm32")))]
            download_existing_pause: Mutex::new(None),
            #[cfg(all(any(test, feature = "test-hooks"), not(target_arch = "wasm32")))]
            reconcile_snapshot_pause: Mutex::new(None),
            #[cfg(any(test, feature = "test-hooks"))]
            outcome_write_errors: AtomicUsize::new(0),
            #[cfg(any(test, feature = "test-hooks"))]
            lease_extension_errors: AtomicUsize::new(0),
        })
    }

    /// Delete local files through the retained store after the client shuts down.
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn delete_local_files(&self) -> Result<(), AttachmentClientError> {
        self.store()?.delete_local_files().await.map_err(Into::into)
    }

    pub(super) fn store(&self) -> Result<&Arc<dyn LocalStore>, AttachmentClientError> {
        self.store
            .as_ref()
            .ok_or_else(|| AttachmentClientError::new(Cause::LocalStorage))
    }

    pub(super) fn shared(&self, digest: &str) -> Arc<PendingShared> {
        let mut pending = self.pending.lock();
        if let Some(shared) = pending.get(digest).and_then(Weak::upgrade) {
            return shared;
        }
        pending.retain(|_, shared| shared.strong_count() != 0);
        let shared = Arc::new(PendingShared::new());
        pending.insert(digest.to_owned(), Arc::downgrade(&shared));
        shared
    }

    pub(super) fn event_lock(&self, key: &str) -> Arc<AsyncMutex<()>> {
        let mut locks = self.event_locks.lock();
        if let Some(lock) = locks.get(key).and_then(Weak::upgrade) {
            return lock;
        }
        locks.retain(|_, lock| lock.strong_count() != 0);
        let lock = Arc::new(AsyncMutex::new(()));
        locks.insert(key.to_owned(), Arc::downgrade(&lock));
        lock
    }

    pub(super) fn cutoff(&self) -> i64 {
        let age = self
            .options
            .max_pending_age
            .unwrap_or(DEFAULT_MAX_PENDING_AGE);
        now_ns().saturating_sub(age.as_nanos().min(i64::MAX as u128) as i64)
    }

    /// Delete expired rows before their staged files. An active upload stays valid.
    pub async fn sweep<Context: AttachmentContext>(
        &self,
        context: &Context,
    ) -> Result<(), AttachmentClientError> {
        let Some(store) = &self.store else {
            return Ok(());
        };
        let rows = context
            .db()
            .pending_attachment_sweep_candidates(self.cutoff())
            .map_err(|_| AttachmentClientError::new(Cause::LocalStorage))?;
        for digest in rows {
            let shared = self.shared(&digest);
            let _state = shared.state.lock().await;
            #[cfg(any(test, feature = "test-hooks"))]
            {
                let pause = self.sweep_pause.lock().clone();
                if let Some((entered, resume)) = pause {
                    entered.notify_one();
                    resume.notified().await;
                }
            }
            let deleted = context
                .db()
                .sweep_pending_attachment(&digest, self.cutoff(), now_ns())
                .map_err(|_| AttachmentClientError::new(Cause::LocalStorage))?;
            if deleted == 0 {
                continue;
            }
            let path = staged_path(&digest)?;
            if store.exists(&path).await? {
                store.remove_file(&path).await?;
            }
            self.pending.lock().remove(&digest);
        }
        Ok(())
    }

    pub(crate) async fn reconcile<Context: AttachmentContext>(
        &self,
        context: &Context,
    ) -> Result<(), AttachmentClientError> {
        let _publication = self.publication_lock.lock().await;
        #[cfg(any(test, feature = "test-hooks"))]
        if FAIL_NEXT_RECONCILES
            .fetch_update(AtomicOrdering::SeqCst, AtomicOrdering::SeqCst, |count| {
                count.checked_sub(1)
            })
            .is_ok()
        {
            return Err(AttachmentClientError::new(Cause::LocalStorage));
        }
        let Some(store) = &self.store else {
            return Ok(());
        };
        let pending: HashMap<_, _> = context
            .db()
            .list_pending_attachments_since(0)
            .map_err(|_| AttachmentClientError::new(Cause::LocalStorage))?
            .into_iter()
            .map(|row| (row.content_digest, row.status))
            .collect();
        let records = context
            .db()
            .list_local_attachments()
            .map_err(|_| AttachmentClientError::new(Cause::LocalStorage))?;
        let recorded: HashSet<_> = records.iter().map(|row| row.path.as_str()).collect();
        let files = store.list_files().await?;
        let present: HashSet<_> = files.iter().map(|file| file.path.as_str()).collect();
        #[cfg(all(any(test, feature = "test-hooks"), not(target_arch = "wasm32")))]
        {
            let pause = self.reconcile_snapshot_pause.lock().clone();
            if let Some((entered, resume)) = pause {
                entered.notify_one();
                resume.notified().await;
            }
        }
        let now = now_ns();
        let cutoff = now.saturating_sub(RECONCILE_AGE.as_nanos() as i64);
        let staged_age = self
            .options
            .max_pending_age
            .unwrap_or(DEFAULT_MAX_PENDING_AGE)
            .min(RECONCILE_AGE);
        let staged_cutoff = now.saturating_sub(staged_age.as_nanos() as i64);
        for file in &files {
            if file.path.starts_with(".tmp/") {
                if file.modified_at_ns < cutoff {
                    store.remove_file(&file.path).await?;
                }
            } else if let Some(digest) = file.path.strip_prefix(".staged/") {
                if pending
                    .get(digest)
                    .is_some_and(|status| status == "complete")
                    || (!pending.contains_key(digest) && file.modified_at_ns < staged_cutoff)
                {
                    store.remove_file(&file.path).await?;
                }
            } else if file.path.split_once('/').is_some_and(|(key, name)| {
                key.len() == 64
                    && key
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                    && !name.is_empty()
                    && !name.contains('/')
            }) && !recorded.contains(file.path.as_str())
            {
                context
                    .db()
                    .insert_or_ignore_local_attachment(&file.path, file.modified_at_ns, None, None)
                    .map_err(|_| AttachmentClientError::new(Cause::LocalStorage))?;
            }
        }
        for record in records {
            if !present.contains(record.path.as_str()) {
                context
                    .db()
                    .delete_local_attachment(&record.path)
                    .map_err(|_| AttachmentClientError::new(Cause::LocalStorage))?;
            }
        }
        Ok(())
    }
}
