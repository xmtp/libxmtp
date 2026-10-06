#[xmtp_macro::sdk_export]
impl Client {
    #[uniffi::constructor]
    pub async fn create(
        signer: Arc<dyn Signer>,
        options: ClientOptions,
    ) -> Result<Self, XmtpError> {
        // Check ID arguments before the signer callback runs.
        options.fork_recovery_opts()?;
        let guard = OpenStoreGuard::default();
        let mut task_guard = guard.share();
        let (created, task_guard) = on_build_task(Box::pin(async move {
            let created = async {
                let identity = signer::identity(signer.clone()).await?;
                Self::create_with_guard(signer, identity, options, &mut task_guard).await
            }
            .await;
            if created.is_err() {
                task_guard.disarm();
            }
            (created, task_guard)
        }))
        .await?;
        task_guard.disarm();
        drop(guard);
        created
    }

    /// Build requires a stored identity. It fetches server configuration by default.
    /// Set `allowOffline` to true to use stored state offline; it needs a known
    /// inbox ID unless the storage location is `Explicit`.
    /// Build checks that the identity belongs to the opened inbox, including
    /// when the caller supplies an `inboxId`.
    #[uniffi::constructor]
    pub async fn build(
        identity: PublicIdentity,
        options: ClientOptions,
        inbox_id: Option<InboxId>,
    ) -> Result<Self, XmtpError> {
        let guard = OpenStoreGuard::default();
        let mut task_guard = guard.share();
        let (built, task_guard) = on_build_task(Box::pin(async move {
            let built = Self::build_inner(identity, options, inbox_id, true, &mut task_guard).await;
            if built.is_err() {
                task_guard.disarm();
            }
            (built, task_guard)
        }))
        .await?;
        task_guard.disarm();
        drop(guard);
        built
    }

    #[sdk(immutable)]
    pub fn inbox_id(&self) -> InboxId {
        InboxId::unchecked(self.inner.inbox_id().to_owned())
    }

    #[sdk(immutable)]
    pub fn installation_id(&self) -> InstallationId {
        InstallationId::unchecked(self.inner.installation_public_key().to_string())
    }

    /// Host runtimes use this key to find the owner of a lifted message.
    #[sdk(immutable)]
    pub fn client_key(&self) -> u64 {
        self.key
    }

    #[sdk(immutable)]
    pub fn conversations(&self) -> Arc<Conversations> {
        Arc::new(Conversations {
            client: self.inner.clone(),
            client_key: self.key,
        })
    }

    #[sdk(immutable)]
    pub fn preferences(&self) -> Arc<Preferences> {
        Arc::new(Preferences {
            client: self.inner.clone(),
        })
    }

    #[sdk(immutable)]
    pub fn diagnostics(&self) -> Arc<Diagnostics> {
        Arc::new(Diagnostics {
            client: self.inner.clone(),
        })
    }

    #[sdk(immutable)]
    pub fn storage(&self) -> Arc<Storage> {
        Arc::new(Storage {
            #[cfg(not(target_arch = "wasm32"))]
            client: self.inner.clone(),
            path: self.storage_path.clone(),
            #[cfg(not(target_arch = "wasm32"))]
            listeners: self.listeners.clone(),
            #[cfg(not(target_arch = "wasm32"))]
            event_readers: self.event_readers.clone(),
        })
    }

    #[sdk(immutable)]
    pub fn attachments(&self) -> Arc<Attachments> {
        Arc::new(Attachments {
            client: self.inner.clone(),
        })
    }

    #[sdk(immutable)]
    pub fn archives(&self) -> Arc<Archives> {
        Arc::new(Archives {
            client: self.inner.clone(),
        })
    }

    pub async fn end(&self) -> Result<(), XmtpError> {
        end_client(&self.inner, &self.listeners, &self.event_readers).await?;
        #[cfg(target_arch = "wasm32")]
        xmtp_db::pause_sqlite_if_idle();
        Ok(())
    }

    // implements: EVENT-014
    // implements: EVENT-015
    // implements: EVENT-016
    pub async fn events(
        &self,
        filter: crate::EventFilter,
    ) -> Result<Arc<crate::EventReader>, XmtpError> {
        let filter = filter.checked()?;
        // The filter reads stored conversations. Leave the gate before the
        // subscription starts: that step does not use the database, and end()
        // must not wait for it.
        let filter = {
            let _call = self.ensure_open()?;
            filter.to_core(&self.inner)?
        };
        let subscription = self
            .inner
            .context
            .events()
            .subscribe_app(filter)
            .ok_or_else(XmtpError::closed)?;
        let reader = crate::EventReader::new(subscription);
        {
            let mut registry = self.event_readers.lock();
            if registry.closing {
                reader.close();
                return Err(XmtpError::closed());
            }
            registry.readers.retain(|weak| weak.strong_count() > 0);
            registry.readers.push(Arc::downgrade(&reader));
        }
        Ok(reader)
    }

    // implements: EVENT-050
    // implements: EVENT-051
    // implements: EVENT-052
    pub async fn start_listener(
        &self,
        filter: crate::EventFilter,
        listener: Arc<dyn crate::EventListener>,
    ) -> Result<crate::ListenerId, XmtpError> {
        let filter = filter.checked()?;
        // The filter reads stored conversations. Leave the gate before the
        // subscription starts: that step does not use the database, and end()
        // must not wait for it.
        let filter = {
            let _call = self.ensure_open()?;
            filter.to_core(&self.inner)?
        };
        let subscription = self
            .inner
            .context
            .events()
            .subscribe_app(filter)
            .ok_or_else(XmtpError::closed)?;
        self.listeners.start(subscription, listener)
    }

    // implements: EVENT-053
    // implements: EVENT-054
    pub async fn stop_listener(&self, id: crate::ListenerId) {
        self.listeners.stop(id);
    }
}
