//! Client builds: admitting the configuration, building offline or online, and the
//! backfill a build runs.

use super::*;

// Every identity request uses an admitted snapshot, including the lookup
// needed to find a data directory's database.
fn admit_configuration<ApiClient: XmtpBackendClient>(
    api_client: &mut ApiClientWrapper<ApiClient>,
    configuration: &xmtp_configuration::ServerConfiguration,
    client_version: &semver::Version,
) -> Result<(), ClientError> {
    crate::server_configuration::check_minimum_version(configuration, client_version)?;
    // implements: CONF-051
    if configuration.auth.enabled && !api_client.has_credential_source() {
        return Err(ClientError::AuthRequired {
            required_scopes: configuration.auth.required_scopes.clone(),
        });
    }
    let snapshot = Arc::new(configuration.clone());
    api_client
        .api_client
        .set_limits(Arc::new(snapshot.limits.clone()));
    api_client.set_configuration(snapshot);
    Ok(())
}

/// One-time backfill of `ProcessPendingSelfRemove` tasks for groups that already
/// had pending leave requests before self-removal became event-driven. Such rows
/// have no incoming LeaveRequest to re-fire, so without this they'd only be
/// processed if a new one arrived. Best-effort and idempotent (deduped per group).
///
/// TODO(#3748): removable once every client has shipped a release that enqueues
/// these tasks inline — safe to delete by the next-next stable release.
fn backfill_pending_self_remove_tasks<C>(context: &C) -> Result<(), StorageError>
where
    C: XmtpSharedContext,
{
    let db = context.db();
    for raw_id in db.get_groups_have_pending_leave_request()? {
        let Ok(group_id) = xmtp_proto::types::GroupId::try_from(raw_id.as_slice()) else {
            continue;
        };
        let now = xmtp_common::time::now_ns();
        let proto = TaskProto {
            task: Some(TaskKind::ProcessPendingSelfRemove(
                ProcessPendingSelfRemove {
                    group_id: group_id.to_vec(),
                },
            )),
        };
        let task = xmtp_db::tasks::NewTask::builder()
            .originating_message_sequence_id(0)
            .created_at_ns(now)
            .next_attempt_at_ns(now)
            .build(proto)?;
        // Insert-if-absent per group: leaves any live retrying task (and its
        // backoff) untouched, only replacing dead rows. Safe to call on every
        // startup without resurrecting exhausted tasks.
        db.upsert_pending_self_remove_task(&group_id, task)?;
    }
    Ok(())
}

impl<ApiClient, S, Db> ClientBuilder<ApiClient, S, Db> {
    /// build a client in offline mode.
    /// returns an error if the client failed to build as offline
    pub fn build_offline(self) -> Result<Client<ContextParts<ApiClient, S, Db>>, ClientBuilderError>
    where
        ApiClient: XmtpApi + 'static,
        Db: xmtp_db::XmtpDb + 'static,
        S: XmtpMlsStorageProvider + 'static,
    {
        self.build()
            .now_or_never()
            .ok_or(ClientBuilderError::OfflineBuildFailed)
            .flatten()
    }

    /// Build the client. When the identity strategy carries an identifier,
    /// the build requires it to belong to the inbox of the identity it loads,
    /// including a stored one: it checks the newest association state after
    /// it loads the inbox's identity updates and before it starts any worker.
    /// An offline build loads none and checks the newest state the database
    /// stores, which can predate a revocation. Before the inbox's first
    /// identity update, only the identifier whose inbox at the strategy's
    /// nonce is that inbox belongs to it. A failure is
    /// [`IdentityError::IdentifierNotInInbox`](crate::identity::IdentityError::IdentifierNotInInbox).
    #[tracing::instrument(err, skip_all, fields(operation = "mls.build_client"))]
    pub async fn build(self) -> Result<Client<ContextParts<ApiClient, S, Db>>, ClientBuilderError>
    where
        ApiClient: XmtpApi + 'static,
        Db: xmtp_db::XmtpDb + 'static,
        S: XmtpMlsStorageProvider + 'static,
    {
        let ClientBuilder {
            mut deployment_recorder,
            data_location,
            location_store_opener,
            mls_storage_factory,
            storage_location_conflict,
            mut attachments_dir,
            attachment_options,
            mut api_client,
            identity,
            mut store,
            mut identity_strategy,
            mut scw_verifier,
            custom_scw_verifier,

            device_sync_worker_mode,
            fork_recovery_opts,
            change_callbacks,
            stream_policy,
            incoming_factory,
            version_info,
            allow_offline,
            disable_commit_log_worker,
            mut mls_storage,
            disable_workers,
            worker_config,
            config_provider,
            #[cfg(any(test, feature = "test-utils"))]
            application_components_for_test,
            ..
        } = self;

        if storage_location_conflict {
            return Err(crate::storage_location::StorageLocationError::ConflictingStore.into());
        }

        let api_client = api_client
            .take()
            .ok_or(ClientBuilderError::MissingParameter {
                parameter: "api_client",
            })?;

        let mut scw_verifier = scw_verifier
            .take()
            .ok_or(ClientBuilderError::MissingParameter {
                parameter: "scw_verifier",
            })?;

        let mut api_client = ApiClientWrapper::new(api_client, Retry::default());
        let config_provider = config_provider.map(ServerConfigurationHandle::new);
        let mut data_dir_opened_identifier = None;
        if let Some(location) = data_location {
            use crate::storage_location::StorageLocationError;
            let data_dir = matches!(
                location,
                crate::storage_location::StorageLocation::DataDir(_)
            );
            // A data directory's path holds the inbox. Only an online build
            // can look up an unknown one.
            let look_up_inbox = data_dir && identity_strategy.inbox_id().is_none();
            if look_up_inbox
                && (allow_offline
                    || !matches!(
                        identity_strategy,
                        IdentityStrategy::CreateForIdentifier { .. }
                    ))
            {
                return Err(StorageLocationError::InboxId.into());
            }
            let backend_url = api_client.backend_url().unwrap_or_default().to_owned();
            if data_dir && backend_url.trim_end_matches('/').is_empty() {
                return Err(StorageLocationError::BackendUrl.into());
            }
            let recorder = location.recorder(&backend_url);
            let recorded = match &recorder {
                Some(recorder) => recorder.lookup().await?,
                None => None,
            };
            let mut fetched = None;
            // The request that looks up an inbox carries the identifier, and
            // the database it finds may hold no configuration. So that build
            // asks the deployment for its configuration first, even for a
            // recorded URL.
            let opened_identifier = match recorded {
                Some(identifier) if !look_up_inbox => Some(identifier),
                _ if !data_dir => None,
                recorded => {
                    if allow_offline {
                        return Err(StorageLocationError::OfflineMissingDeployment.into());
                    }
                    let response = api_client.get_configuration().await.map_err(|error| {
                        ClientBuilderError::ClientError(ClientError::ConfigurationUnavailable(
                            Box::new(crate::server_configuration::ConfigurationFetchError::Api(
                                error,
                            )),
                        ))
                    })?;
                    let configuration = crate::server_configuration::validated(&response)
                        .map_err(ClientError::from)?;
                    let identifier = configuration.identifier.clone();
                    if let Some(recorded) = recorded
                        && recorded != identifier
                    {
                        return Err(ClientError::BackendMismatch {
                            stored: recorded,
                            received: identifier,
                        }
                        .into());
                    }
                    if look_up_inbox {
                        let admission = config_provider
                            .as_ref()
                            .map(ServerConfigurationHandle::configuration)
                            .unwrap_or(&configuration);
                        if admission.identifier != identifier {
                            return Err(StorageLocationError::DeploymentMismatch.into());
                        }
                        admit_configuration(
                            &mut api_client,
                            admission,
                            version_info.pkg_semver().semver(),
                        )?;
                    }
                    fetched = Some((identifier.clone(), response));
                    Some(identifier)
                }
            };
            // An explicit database is opened first, and its build looks up
            // an unknown inbox after the configuration checks below.
            if look_up_inbox {
                identity_strategy = identity_strategy.with_inbox(&api_client).await?;
            }
            let paths = location.resolve_identifier(
                identity_strategy.inbox_id().unwrap_or_default(),
                opened_identifier.as_deref().unwrap_or_default(),
            )?;
            let opener = location_store_opener.ok_or(StorageLocationError::ConflictingStore)?;
            let opened = opener(paths.clone()).await?;
            if let Some((identifier, response)) = fetched {
                // A stored configuration stays, and the checks below compare
                // it with this backend.
                if opened.db().server_configuration()?.is_none() {
                    opened.db().store_server_configuration(
                        &identifier,
                        backend_url.trim_end_matches('/'),
                        &response.encode_to_vec(),
                        xmtp_common::time::now_ns(),
                    )?;
                }
                if let Some(recorder) = &recorder {
                    recorder.record(&identifier).await?;
                }
            }
            store = Some(opened);
            attachments_dir = Some(paths.attachments_dir);
            if data_dir {
                data_dir_opened_identifier = opened_identifier.clone();
            }
            deployment_recorder = recorder.map(|recorder| match opened_identifier {
                Some(identifier) => recorder.for_opened_identifier(identifier),
                None => recorder,
            });
        }
        let store = store
            .take()
            .ok_or(ClientBuilderError::MissingParameter { parameter: "store" })?;
        let mls_storage = mls_storage
            .take()
            .or_else(|| mls_storage_factory.map(|factory| factory(&store)))
            .ok_or(ClientBuilderError::MissingParameter {
                parameter: "mls_storage",
            })?;

        let conn = store.db();

        // The configuration is resolved before any identity
        // work, so a deployment that refuses this client refuses it before the
        // database gains an identity. A caller-supplied provider
        // short-circuits every network and database path here, which is what
        // keeps `build_offline` free of a pending future.
        let has_config_provider = config_provider.is_some();
        let server_configuration = match config_provider {
            Some(provider) => provider,
            None => crate::server_configuration::resolve(&api_client, &conn, allow_offline)
                .await
                .map_err(ClientError::from)?,
        };

        #[cfg(any(test, feature = "test-utils"))]
        let server_configuration = match application_components_for_test {
            Some(components) => server_configuration.with_application_components(components),
            None => server_configuration,
        };

        if has_config_provider && let Some(opened_identifier) = data_dir_opened_identifier {
            let provider_identifier = &server_configuration.configuration().identifier;
            let stored_identifier = conn.server_configuration()?.map(|row| row.identifier);
            if provider_identifier != &opened_identifier
                || stored_identifier.as_deref().is_some_and(|identifier| {
                    !identifier.is_empty() && identifier != provider_identifier
                })
            {
                return Err(
                    crate::storage_location::StorageLocationError::DeploymentMismatch.into(),
                );
            }
        }

        let server_configuration = server_configuration.with_chain_restriction(custom_scw_verifier);
        if server_configuration.requires_preflight() {
            api_client.api_client.require_preflight();
        }
        if let Some(recorder) = deployment_recorder {
            if !has_config_provider && !server_configuration.configuration().identifier.is_empty() {
                recorder
                    .record(&server_configuration.configuration().identifier)
                    .await?;
            }
            server_configuration.set_deployment_recorder(recorder);
        }

        admit_configuration(
            &mut api_client,
            server_configuration.configuration(),
            version_info.pkg_semver().semver(),
        )?;
        // A shared backend can admit a different snapshot for another client.
        // Keep this client's receiving limits before its first lazy stream opens.
        let incoming_factory = incoming_factory
            .map(|factory| factory.bind_limits(api_client.limits()).unwrap_or(factory));
        // Clone after admission, so the default verifier sizes its verify
        // requests by the published limits.
        if !custom_scw_verifier {
            scw_verifier = Box::new(api_client.clone());
        }

        let identifier_check = match &identity_strategy {
            IdentityStrategy::CreateIfNotFound {
                identifier, nonce, ..
            }
            | IdentityStrategy::CreateForIdentifier { identifier, nonce } => {
                Some((identifier.clone(), *nonce))
            }
            _ => None,
        };
        let mut identity = if let Some(identity) = identity {
            identity
        } else {
            identity_strategy
                .initialize_identity(&api_client, &mls_storage, &scw_verifier)
                .await?
        };

        // The registration request is handed to the app to
        // sign, so bind it to the chains the deployment accepts before it
        // leaves the client.
        if let Some(request) = identity.signature_request.as_mut() {
            server_configuration.restrict(request);
        }

        debug!(
            inbox_id = identity.inbox_id(),
            installation_id = identity.installation_keys.public_bytes().snippet(),
            "Initialized identity"
        );
        if !allow_offline {
            // get sequence_id from identity updates and loaded into the DB
            load_identity_updates(
                &api_client,
                &conn,
                vec![identity.inbox_id.as_str()].as_slice(),
            )
            .await
            .map_err(ClientError::from)?;
        }
        // The identity store trusts a stored identity of the same inbox. The
        // check runs on the identity updates loaded above and before any
        // worker, so a rejected identifier makes no request as the stored
        // installation. Offline, it reads only the database.
        // implements: IDENT-096
        if let Some((identifier, nonce)) = identifier_check {
            let inbox_id = identity.inbox_id.as_str();
            let membership = if allow_offline {
                crate::identity_updates::stored_identifier_membership(&conn, inbox_id, &identifier)
                    .await
                    .map_err(ClientError::from)?
            } else {
                crate::identity_updates::identifier_membership(
                    &conn,
                    inbox_id,
                    &identifier,
                    &scw_verifier,
                )
                .await
                .map_err(ClientError::from)?
            };
            if !crate::identity_updates::identifier_opens_inbox(
                membership,
                &identifier,
                nonce,
                &identity.inbox_id,
            ) {
                return Err(crate::identity::IdentityError::IdentifierNotInInbox {
                    inbox_id: identity.inbox_id.clone(),
                }
                .into());
            }
        }

        // Own-installation events start after the own-inbox updates stored
        // now, including any the load above stored.
        let own_inbox_stored_through = conn
            .get_latest_sequence_id(&[identity.inbox_id.as_str()])
            .map_err(StorageError::from)?
            .get(identity.inbox_id.as_str())
            .copied();

        // Fold the legacy single-worker toggles into the unified enable map so
        // there is one source of truth for "is worker X enabled" that code paths
        // when it registers worker subscriptions. The old fields keep working;
        // they just write here.
        let mut worker_config = worker_config;
        if disable_workers {
            // Disable optional workers. The HMAC epoch observer still runs for
            // every open client.
            for kind in [
                crate::worker::WorkerKind::DeviceSync,
                crate::worker::WorkerKind::DisappearingMessages,
                crate::worker::WorkerKind::CommitLog,
                crate::worker::WorkerKind::TaskRunner,
                crate::worker::WorkerKind::ConfigurationRefresh,
                crate::worker::WorkerKind::AttachmentCleanup,
            ] {
                worker_config.enabled.insert(kind, false);
            }
        }
        if matches!(device_sync_worker_mode, DeviceSyncMode::Disabled) {
            worker_config
                .enabled
                .entry(crate::worker::WorkerKind::DeviceSync)
                .or_insert(false);
        }
        if disable_commit_log_worker {
            worker_config
                .enabled
                .entry(crate::worker::WorkerKind::CommitLog)
                .or_insert(false);
        }

        let events = xmtp_events::EventBus::new();
        let events_for_lifetime = events.clone();
        let public_event_writer: Arc<dyn xmtp_events::EventWriter<()>> =
            Arc::new(xmtp_events::PublicBusWriter::new(&events));
        server_configuration.set_event_writer(public_event_writer.clone());
        api_client
            .api_client
            .register_client_event_writer(&public_event_writer);
        let mut workers = WorkerRunner::new();
        let incoming_runtime = crate::subscriptions::incoming::IncomingRuntime::new(
            stream_policy.clone(),
            incoming_factory,
        )
        .with_preflight(api_client.api_client.clone());
        let attachments =
            Arc::new(AttachmentRuntime::new(attachments_dir, attachment_options).await?);
        let context = Arc::new(XmtpMlsLocalContext {
            identity,
            mls_storage,
            store,
            api_client,
            version_info,
            server_configuration,
            scw_verifier: Arc::new(scw_verifier),
            mutexes: MutexRegistry::new(),
            hooks: Default::default(),
            events,
            public_event_writer,
            registration_event_pending: Arc::new(AtomicBool::new(false)),
            device_sync: DeviceSync {
                mode: device_sync_worker_mode,
            },
            fork_recovery_opts: fork_recovery_opts.unwrap_or_default(),
            change_callbacks,
            stream_policy,
            worker_config,

            worker_metrics: workers.sync_metrics_slot().clone(),
            task_channels: workers.task_channels().clone(),
            cancellation_token: CancellationToken::new(),
            shutdown_complete: Arc::new(AtomicBool::new(false)),
            foreground_calls: Default::default(),
            delivery_owner: Default::default(),
            retired_delivery_owner: Default::default(),
            features: crate::features::MlsFeatures {
                attachments,
                incoming: Arc::new(incoming_runtime),
                identity_resolutions: Arc::new(
                    crate::identity_updates::IdentityResolutionRegistry::starting_after(
                        own_inbox_stored_through,
                    ),
                ),
            },
        });

        crate::core::preflight::bind_preflight(&context);

        // register workers
        if let Err(error) = context.features.attachments.sweep(&context).await {
            tracing::warn!(%error, "attachment cleanup failed during client build");
        }
        if let Err(error) = context
            .features
            .attachments
            .ensure_reconciled(&context)
            .await
        {
            tracing::warn!(%error, "attachment reconciliation failed during client build");
        }
        if !disable_workers {
            use crate::worker::WorkerKind;
            // One source of truth for enablement: the folded WorkerConfig map.
            let enabled = |k| context.worker_config().worker_enabled(k);

            // Keep the original `device_sync_worker_enabled()` AND-check to
            // preserve the exact legacy semantics alongside the map gate.
            if enabled(WorkerKind::DeviceSync) && context.device_sync_worker_enabled() {
                workers.register_new_worker::<SyncWorker<ContextParts<ApiClient, S, Db>>, _>(
                    context.clone(),
                );
            }
            if enabled(WorkerKind::DisappearingMessages) {
                workers
                    .register_new_worker::<DisappearingMessagesWorker<ContextParts<ApiClient, S, Db>>, _>(
                        context.clone(),
                    );
            }
            // Enable CommitLogWorker based on configuration
            // The deployment decides whether the commit log runs,
            // falling back to the compiled default when it says nothing.
            if enabled(WorkerKind::CommitLog)
                && context
                    .server_configuration()
                    .configuration()
                    .mls
                    .commit_log_enabled()
                && !disable_commit_log_worker
            {
                workers.register_new_worker::<
                crate::groups::commit_log::CommitLogWorker<ContextParts<ApiClient, S, Db>>,
                _,
                >(context.clone());
            }
            // Only a client that reads its configuration refreshes it.
            // A caller-supplied provider owns its own values.
            if enabled(WorkerKind::ConfigurationRefresh) && !has_config_provider {
                workers.register_new_worker::<crate::core::config_refresh::ConfigurationWorker<
                    ContextParts<ApiClient, S, Db>,
                >, _>(context.clone());
            }
            if enabled(WorkerKind::TaskRunner) {
                workers.register_new_worker::<TaskWorker<
                    ContextParts<ApiClient, S, Db>,
                    crate::task_handlers::MlsTaskHandlers,
                >, _>(context.clone());
                // KP maintenance is deliberately coupled to the TaskRunner (no
                // standalone fallback): disabling the TaskRunner — per-kind via
                // WorkerConfig or globally via disable_workers — disables KP
                // rotation/deletion with it. Seeding failure is fatal to the
                // build: the DB was already opened/migrated above, so an error
                // here means it is broken; building a client whose critical
                // maintenance silently never got seeded would be worse.
                crate::worker::key_package_maintenance::seed_and_reconcile_kp_tasks(&context)?;
                // One-time backfill: pending self-removes recorded before the
                // worker became event-driven have no LeaveRequest to re-fire, so
                // seed a ProcessPendingSelfRemove task for each already-flagged
                // group. Best-effort (logged, never fails the build).
                //
                // TODO: remove this migration once all clients have shipped a
                // release that enqueues these tasks inline — safe to delete by the
                // next-next stable release.
                if let Err(e) = backfill_pending_self_remove_tasks(&context) {
                    tracing::warn!(
                        "pending-self-remove backfill failed (will rely on next sync): {e}"
                    );
                }
            }
            if enabled(WorkerKind::AttachmentCleanup) && context.features.attachments.has_store() {
                workers.register_new_worker::<crate::attachments::cleanup::AttachmentCleanup<
                    ContextParts<ApiClient, S, Db>,
                >, _>(context.clone());
            }
        }

        // Every open client observes HMAC epoch changes, including clients
        // that disable the other background workers.
        workers.register_new_worker::<
            crate::worker::hmac_epoch::HmacEpochWorker<ContextParts<ApiClient, S, Db>>,
            _,
        >(context.clone());

        let workers = Arc::new(workers);

        workers.spawn(context.clone());

        log_event!(
            Event::ClientCreated,
            context.installation_id(),
            inbox_id = context.inbox_id(),
            device_sync_enabled = context.device_sync_worker_enabled(),
            disabled_workers = disable_workers,
        );

        let installation_id = context.installation_id();
        let client = Client {
            context,
            installation_id,
            workers,
            app_lifetime: Arc::new(crate::client::AppLifetime {
                events: events_for_lifetime,
            }),
        };

        Ok(client)
    }
}
