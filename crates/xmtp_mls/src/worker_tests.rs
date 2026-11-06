//! Characterization tests of the worker runner over every worker the client
//! registers: which events wake each worker, how each worker's error
//! wrapper forwards a database disconnect, and how the runner restarts a
//! rate-limited worker. They build workers from every feature, so they live
//! with the client rather than with the runner.

use crate::core::events::InternalEvent;
use crate::core::hmac_epoch;
#[cfg(not(target_arch = "wasm32"))]
use crate::core::reconnect::NeedsDbReconnect;
use crate::worker::*;
use std::sync::Arc;
#[cfg(not(target_arch = "wasm32"))]
use tokio_util::sync::CancellationToken;
#[cfg(not(target_arch = "wasm32"))]
use xmtp_common::time::Duration;
use xmtp_events::EventFilter;

#[cfg(test)]
pub(crate) mod event_subscription_tests {
    use super::*;
    use crate::subscriptions::internal::DeletedMessages;
    use crate::worker::device_sync::preference_sync::PreferenceUpdate;
    use xmtp_events::{EventBus, EventKind, EventWriter};

    /// The factory of every worker the builder can register, on a mock context.
    fn factories() -> Vec<DynFactory> {
        use crate::test::mock::{NewMockContext, context};
        fn factory<W: NewWorker<NewMockContext>>() -> DynFactory {
            Arc::new(W::factory(context()))
        }
        vec![
            factory::<crate::attachments::cleanup::AttachmentCleanup<NewMockContext>>(),
            factory::<crate::worker::device_sync::worker::SyncWorker<NewMockContext>>(),
            factory::<
                crate::worker::disappearing_messages::DisappearingMessagesWorker<NewMockContext>,
            >(),
            factory::<crate::groups::commit_log::CommitLogWorker<NewMockContext>>(),
            factory::<
                crate::worker::tasks::TaskWorker<
                    NewMockContext,
                    crate::task_handlers::MlsTaskHandlers,
                >,
            >(),
            factory::<crate::core::config_refresh::ConfigurationWorker<NewMockContext>>(),
            factory::<hmac_epoch::HmacEpochWorker<NewMockContext>>(),
        ]
    }

    /// The event interest of the worker of `kind`; `None` when no worker of
    /// that kind exists or no event wakes it.
    pub(crate) fn worker_event_filter(
        kind: WorkerKind,
    ) -> Option<(EventFilter<InternalEvent>, Option<usize>)> {
        factories()
            .into_iter()
            .find(|factory| factory.kind() == kind)?
            .event_interest()
    }

    /// What a worker of `kind` subscribes to: its public kinds, its buffer
    /// depth, and the labels of the `events` its internal predicate admits.
    fn event_interest_for(
        kind: WorkerKind,
        events: &[(&'static str, InternalEvent)],
    ) -> Option<(Vec<EventKind>, Option<usize>, Vec<&'static str>)> {
        let (filter, depth) = worker_event_filter(kind)?;
        let kinds = filter.kinds.clone();
        let bus = EventBus::new();
        let subscription = bus.subscribe(filter, depth);
        let mut admitted = vec![];
        for (label, event) in events {
            bus.emit(None, Some(event.clone()));
            if !subscription.drain().is_empty() {
                admitted.push(*label);
            }
        }
        Some((kinds, depth, admitted))
    }

    /// One labelled sample of every internal event, with a matching and a
    /// non-matching value for each field a worker predicate reads. The match
    /// has no wildcard, so a new variant fails to compile here until it has a
    /// sample.
    fn internal_event_samples() -> Vec<(&'static str, InternalEvent)> {
        let group_id = xmtp_proto::types::GroupId::from([7; 16]);
        let stored = |expires_at_ns, is_sync| InternalEvent::MessageStored {
            group_id,
            message_id: vec![1],
            expires_at_ns,
            is_sync,
        };
        let deleted = || DeletedMessages {
            messages: vec![],
            deleted_by_inbox_id: None,
        };
        let samples = vec![
            (
                "GroupJoined{is_sync:false}",
                InternalEvent::GroupJoined {
                    group_id,
                    is_sync: false,
                },
            ),
            (
                "GroupJoined{is_sync:true}",
                InternalEvent::GroupJoined {
                    group_id,
                    is_sync: true,
                },
            ),
            (
                "MessageStored{expires:None,is_sync:false}",
                stored(None, false),
            ),
            (
                "MessageStored{expires:None,is_sync:true}",
                stored(None, true),
            ),
            (
                "MessageStored{expires:Some,is_sync:false}",
                stored(Some(1), false),
            ),
            (
                "MessageStored{expires:Some,is_sync:true}",
                stored(Some(1), true),
            ),
            ("OutboxChanged", InternalEvent::OutboxChanged),
            ("TaskScheduled", InternalEvent::TaskScheduled),
            (
                "NotificationSettingsChanged",
                InternalEvent::NotificationSettingsChanged,
            ),
            (
                "PreferencesChanged{empty}",
                InternalEvent::PreferencesChanged { updates: vec![] },
            ),
            (
                "PreferencesChanged{hmac}",
                InternalEvent::PreferencesChanged {
                    updates: vec![PreferenceUpdate::Hmac {
                        key: vec![3; 32].into(),
                        cycled_at_ns: 1,
                    }],
                },
            ),
            ("MessagesDeleted", InternalEvent::MessagesDeleted(deleted())),
            ("MessagesExpired", InternalEvent::MessagesExpired(deleted())),
        ];
        for (_, event) in &samples {
            match event {
                InternalEvent::GroupJoined { .. }
                | InternalEvent::MessageStored { .. }
                | InternalEvent::OutboxChanged
                | InternalEvent::TaskScheduled
                | InternalEvent::NotificationSettingsChanged
                | InternalEvent::PreferencesChanged { .. }
                | InternalEvent::MessagesDeleted(_)
                | InternalEvent::MessagesExpired(_) => {}
            }
        }
        samples
    }

    /// Which events wake each worker, frozen before the crate split. The
    /// match has no wildcard, so a new worker kind needs a row.
    fn frozen_interest(
        kind: WorkerKind,
    ) -> Option<(Vec<EventKind>, Option<usize>, Vec<&'static str>)> {
        match kind {
            WorkerKind::DeviceSync => Some((
                vec![],
                None,
                vec![
                    "MessageStored{expires:None,is_sync:true}",
                    "MessageStored{expires:Some,is_sync:true}",
                    "OutboxChanged",
                ],
            )),
            WorkerKind::DisappearingMessages => Some((
                vec![EventKind::ConversationMetadataChanged],
                Some(10),
                vec![
                    "MessageStored{expires:Some,is_sync:false}",
                    "MessageStored{expires:Some,is_sync:true}",
                ],
            )),
            WorkerKind::TaskRunner => Some((
                vec![
                    EventKind::ConsentChanged,
                    EventKind::ConversationJoined,
                    EventKind::ConversationRemoved,
                    EventKind::HmacKeysUpdated,
                    EventKind::ArchiveRestored,
                ],
                Some(10),
                vec![
                    "GroupJoined{is_sync:false}",
                    "GroupJoined{is_sync:true}",
                    "TaskScheduled",
                    "NotificationSettingsChanged",
                    "PreferencesChanged{empty}",
                    "PreferencesChanged{hmac}",
                ],
            )),
            WorkerKind::AttachmentCleanup
            | WorkerKind::KeyPackageCleaner
            | WorkerKind::CommitLog
            | WorkerKind::ConfigurationRefresh
            | WorkerKind::HmacEpoch => None,
        }
    }

    #[xmtp_common::test]
    fn worker_event_interest_matches_frozen_table() {
        let samples = internal_event_samples();
        for kind in [
            WorkerKind::AttachmentCleanup,
            WorkerKind::DeviceSync,
            WorkerKind::DisappearingMessages,
            WorkerKind::KeyPackageCleaner,
            WorkerKind::CommitLog,
            WorkerKind::TaskRunner,
            WorkerKind::ConfigurationRefresh,
            WorkerKind::HmacEpoch,
        ] {
            assert_eq!(
                event_interest_for(kind, &samples),
                frozen_interest(kind),
                "{kind:?}"
            );
        }
    }
}

// These fixtures use native pool errors. Browser connection lifecycle tests
// cover closed persistent handles in the WASM database module.
#[cfg(all(test, not(target_arch = "wasm32")))]
mod disconnect_propagation_tests {
    //! Pins that a dropped-pool signal survives the wrapper error types each
    //! worker surfaces from `run_tasks`, so `needs_db_reconnect()` stays `true`.
    use super::NeedsDbReconnect;
    use crate::groups::GroupError;
    use crate::mls_store::MlsStoreError;
    use crate::subscriptions::SubscribeError;
    use crate::worker::device_sync::DeviceSyncError;
    use crate::worker::key_package_maintenance::KeyPackageMaintenanceError;
    use xmtp_db::{ConnectionError, PlatformStorageError, StorageError};

    /// A `StorageError` that signals the connection pool was dropped.
    fn disconnect_storage() -> StorageError {
        StorageError::Platform(PlatformStorageError::PoolNeedsConnection)
    }

    /// A `ConnectionError` that signals the connection pool was dropped.
    fn disconnect_connection() -> ConnectionError {
        ConnectionError::Platform(PlatformStorageError::PoolNeedsConnection)
    }

    // Per-worker `run_tasks` error types — what the supervisor actually inspects.

    #[xmtp_common::test]
    fn task_worker_load_group_forwards_disconnect() {
        use crate::task_handlers::TaskWorkerError;
        // Self-remove tasks load the MLS group (MlsStoreError) and remove members
        // (GroupError); a dropped pool must bubble through both paths.
        assert!(
            TaskWorkerError::LoadGroup(MlsStoreError::Connection(disconnect_connection()))
                .needs_db_reconnect()
        );
        assert!(
            TaskWorkerError::Group(GroupError::Storage(disconnect_storage())).needs_db_reconnect()
        );
        assert!(!TaskWorkerError::Group(GroupError::InvalidGroupMembership).needs_db_reconnect());
    }

    /// Every wrapper variant that holds an error with a reconnect signal
    /// forwards a dropped pool, at each level a worker or a client caller
    /// inspects.
    #[xmtp_common::test]
    fn every_wrapper_forwards_db_disconnect() {
        use crate::client::ClientError;
        use crate::client::notifications::NotificationError;
        use crate::groups::ProcessIntentError;
        use crate::groups::{
            GroupMessageProcessingError, summary::SyncSummary,
            validated_commit::CommitValidationError,
        };
        use crate::identity::IdentityError;
        use crate::identity_updates::{
            IdentityDependencyError, IdentityUpdateError, InstallationDiffError,
        };
        use crate::messages::enrichment::EnrichMessageError;
        use crate::server_configuration::ConfigurationFetchError;
        use crate::subscriptions::{incoming::IncomingError, local_delivery::LocalDeliveryError};
        use crate::task_handlers::TaskWorkerError;
        use std::sync::Arc;
        use xmtp_db::{consent_record::ConsentActError, sql_key_store::SqlKeyStoreError};

        let group = || GroupError::Storage(disconnect_storage());
        let processing = || GroupMessageProcessingError::Storage(disconnect_storage());
        let identity_update = || IdentityUpdateError::Storage(disconnect_storage());
        let identity = || IdentityError::StorageError(disconnect_storage());
        let dependency = || IdentityDependencyError::Storage(disconnect_storage());
        let diff = || InstallationDiffError::Storage(disconnect_storage());
        let commit = || CommitValidationError::StorageError(disconnect_storage());
        let summary = || Box::new(SyncSummary::other(group()));
        let incoming = || Arc::new(IncomingError::Storage(disconnect_storage()));
        let local_delivery = || LocalDeliveryError::Storage(disconnect_storage());
        let device_sync = || DeviceSyncError::Storage(disconnect_storage());

        let cases = [
            (
                "ClientError::Storage",
                ClientError::Storage(disconnect_storage()).db_needs_connection(),
            ),
            (
                "ClientError::ConsentAct",
                ClientError::ConsentAct(ConsentActError::Storage(disconnect_storage()))
                    .db_needs_connection(),
            ),
            (
                "ClientError::Db",
                ClientError::Db(disconnect_connection()).db_needs_connection(),
            ),
            (
                "ClientError::Group",
                ClientError::Group(Box::new(group())).db_needs_connection(),
            ),
            (
                "ClientError::Identity",
                ClientError::Identity(identity()).db_needs_connection(),
            ),
            (
                "ClientError::IdentityUpdate",
                ClientError::IdentityUpdate(identity_update()).db_needs_connection(),
            ),
            (
                "ClientError::MlsStore",
                ClientError::MlsStore(MlsStoreError::Connection(disconnect_connection()))
                    .db_needs_connection(),
            ),
            (
                "ClientError::ConfigurationUnavailable",
                ClientError::ConfigurationUnavailable(Box::new(ConfigurationFetchError::Storage(
                    disconnect_storage(),
                )))
                .db_needs_connection(),
            ),
            ("GroupError::Storage", group().needs_db_reconnect()),
            (
                "GroupError::IdentityUpdate",
                GroupError::IdentityUpdate(identity_update()).needs_db_reconnect(),
            ),
            (
                "GroupError::Db",
                GroupError::Db(disconnect_connection()).needs_db_reconnect(),
            ),
            (
                "GroupError::MlsStore",
                GroupError::MlsStore(MlsStoreError::Storage(disconnect_storage()))
                    .needs_db_reconnect(),
            ),
            (
                "GroupError::Identity",
                GroupError::Identity(identity()).needs_db_reconnect(),
            ),
            (
                "GroupError::ReceiveError",
                GroupError::ReceiveError(processing()).needs_db_reconnect(),
            ),
            (
                "GroupError::Sync",
                GroupError::Sync(summary()).needs_db_reconnect(),
            ),
            (
                "GroupError::SyncFailedToWait",
                GroupError::SyncFailedToWait(summary()).needs_db_reconnect(),
            ),
            (
                "GroupError::CommitValidation",
                GroupError::CommitValidation(commit()).needs_db_reconnect(),
            ),
            (
                "GroupError::InstallationDiff",
                GroupError::InstallationDiff(diff()).needs_db_reconnect(),
            ),
            (
                "GroupError::ProcessIntent",
                GroupError::ProcessIntent(ProcessIntentError::Storage(disconnect_storage()))
                    .needs_db_reconnect(),
            ),
            (
                "GroupError::SqlKeyStore",
                GroupError::SqlKeyStore(SqlKeyStoreError::Connection(disconnect_connection()))
                    .needs_db_reconnect(),
            ),
            (
                "GroupMessageProcessingError::Storage",
                processing().needs_db_reconnect(),
            ),
            (
                "GroupMessageProcessingError::Db",
                GroupMessageProcessingError::Db(disconnect_connection()).needs_db_reconnect(),
            ),
            (
                "GroupMessageProcessingError::IdentityUpdate",
                GroupMessageProcessingError::IdentityUpdate(identity_update()).needs_db_reconnect(),
            ),
            (
                "GroupMessageProcessingError::Identity",
                GroupMessageProcessingError::Identity(identity()).needs_db_reconnect(),
            ),
            (
                "GroupMessageProcessingError::PreparedAttempt",
                GroupMessageProcessingError::PreparedAttempt(Box::new(group()))
                    .needs_db_reconnect(),
            ),
            (
                "GroupMessageProcessingError::CommitValidation",
                GroupMessageProcessingError::CommitValidation(commit()).needs_db_reconnect(),
            ),
            (
                "CommitValidationError::StorageError",
                commit().needs_db_reconnect(),
            ),
            (
                "CommitValidationError::InstalledState",
                CommitValidationError::InstalledState(Box::new(commit())).needs_db_reconnect(),
            ),
            (
                "CommitValidationError::IdentityDependency",
                CommitValidationError::IdentityDependency(dependency()).needs_db_reconnect(),
            ),
            (
                "CommitValidationError::InstallationDiff",
                CommitValidationError::InstallationDiff(diff()).needs_db_reconnect(),
            ),
            (
                "IdentityError::StorageError",
                identity().needs_db_reconnect(),
            ),
            (
                "IdentityError::Db",
                IdentityError::Db(disconnect_connection()).needs_db_reconnect(),
            ),
            (
                "IdentityError::OpenMlsStorageError",
                IdentityError::OpenMlsStorageError(SqlKeyStoreError::Connection(
                    disconnect_connection(),
                ))
                .needs_db_reconnect(),
            ),
            (
                "IdentityUpdateError::Storage",
                identity_update().needs_db_reconnect(),
            ),
            (
                "IdentityUpdateError::Db",
                IdentityUpdateError::Db(disconnect_connection()).needs_db_reconnect(),
            ),
            (
                "IdentityUpdateError::Identity",
                IdentityUpdateError::Identity(identity()).needs_db_reconnect(),
            ),
            (
                "IdentityUpdateError::Publish",
                IdentityUpdateError::Publish(Box::new(identity_update())).needs_db_reconnect(),
            ),
            (
                "InstallationDiffError::Storage",
                diff().needs_db_reconnect(),
            ),
            (
                "InstallationDiffError::Db",
                InstallationDiffError::Db(disconnect_connection()).needs_db_reconnect(),
            ),
            (
                "InstallationDiffError::IdentityDependency",
                InstallationDiffError::IdentityDependency(dependency()).needs_db_reconnect(),
            ),
            (
                "InstallationDiffError::IdentityUpdate",
                InstallationDiffError::IdentityUpdate(identity_update()).needs_db_reconnect(),
            ),
            (
                "IdentityDependencyError::Storage",
                dependency().needs_db_reconnect(),
            ),
            (
                "IdentityDependencyError::IdentityUpdate",
                IdentityDependencyError::IdentityUpdate(Box::new(identity_update()))
                    .needs_db_reconnect(),
            ),
            (
                "SubscribeError::Group",
                SubscribeError::from(group()).needs_db_reconnect(),
            ),
            (
                "SubscribeError::Storage",
                SubscribeError::Storage(disconnect_storage()).needs_db_reconnect(),
            ),
            (
                "SubscribeError::Db",
                SubscribeError::Db(disconnect_connection()).needs_db_reconnect(),
            ),
            (
                "SubscribeError::LocalDelivery",
                SubscribeError::LocalDelivery(local_delivery()).needs_db_reconnect(),
            ),
            (
                "SubscribeError::ReceiveGroup",
                SubscribeError::ReceiveGroup(Box::new(processing())).needs_db_reconnect(),
            ),
            (
                "SubscribeError::Enriched(Storage)",
                SubscribeError::Enriched(EnrichMessageError::Storage(disconnect_storage()))
                    .needs_db_reconnect(),
            ),
            (
                "SubscribeError::Enriched(DbConnection)",
                SubscribeError::Enriched(EnrichMessageError::DbConnection(disconnect_connection()))
                    .needs_db_reconnect(),
            ),
            (
                "LocalDeliveryError::Storage",
                local_delivery().needs_db_reconnect(),
            ),
            (
                "LocalDeliveryError::SessionFailure",
                LocalDeliveryError::SessionFailure(Arc::new(local_delivery())).needs_db_reconnect(),
            ),
            (
                "LocalDeliveryError::NetworkRecoveryExhausted",
                LocalDeliveryError::NetworkRecoveryExhausted {
                    attempts: 1,
                    source: Some(incoming()),
                }
                .needs_db_reconnect(),
            ),
            (
                "LocalDeliveryError::NetworkFailure",
                LocalDeliveryError::NetworkFailure(incoming()).needs_db_reconnect(),
            ),
            (
                "DeviceSyncError::Storage",
                device_sync().needs_db_reconnect(),
            ),
            (
                "DeviceSyncError::Db",
                DeviceSyncError::Db(disconnect_connection()).needs_db_reconnect(),
            ),
            (
                "DeviceSyncError::IdentityUpdate",
                DeviceSyncError::IdentityUpdate(identity_update()).needs_db_reconnect(),
            ),
            (
                "DeviceSyncError::Group",
                DeviceSyncError::Group(group()).needs_db_reconnect(),
            ),
            (
                "DeviceSyncError::MlsStore",
                DeviceSyncError::MlsStore(MlsStoreError::Storage(disconnect_storage()))
                    .needs_db_reconnect(),
            ),
            (
                "DeviceSyncError::OwnStateUnavailable",
                DeviceSyncError::OwnStateUnavailable(identity_update()).needs_db_reconnect(),
            ),
            (
                "DeviceSyncError::Sync",
                DeviceSyncError::Sync(summary()).needs_db_reconnect(),
            ),
            (
                "DeviceSyncError::Archive",
                DeviceSyncError::Archive(xmtp_archive::ArchiveError::Storage(disconnect_storage()))
                    .needs_db_reconnect(),
            ),
            (
                "TaskWorkerError::Storage",
                TaskWorkerError::Storage(disconnect_storage()).needs_db_reconnect(),
            ),
            (
                "TaskWorkerError::LoadGroup",
                TaskWorkerError::LoadGroup(MlsStoreError::Storage(disconnect_storage()))
                    .needs_db_reconnect(),
            ),
            (
                "TaskWorkerError::Group",
                TaskWorkerError::Group(group()).needs_db_reconnect(),
            ),
            (
                "TaskWorkerError::DeviceSync",
                TaskWorkerError::DeviceSync(device_sync()).needs_db_reconnect(),
            ),
            (
                "TaskWorkerError::Identity",
                TaskWorkerError::Identity(identity()).needs_db_reconnect(),
            ),
            (
                "TaskWorkerError::KeyPackageMaintenance",
                TaskWorkerError::KeyPackageMaintenance(KeyPackageMaintenanceError::Storage(
                    disconnect_storage(),
                ))
                .needs_db_reconnect(),
            ),
            (
                "TaskWorkerError::Notification(Storage)",
                TaskWorkerError::Notification(NotificationError::Storage(disconnect_storage()))
                    .needs_db_reconnect(),
            ),
            (
                "TaskWorkerError::Notification(Group)",
                TaskWorkerError::Notification(NotificationError::Group(group()))
                    .needs_db_reconnect(),
            ),
            (
                "ConfigurationFetchError::Storage",
                ConfigurationFetchError::Storage(disconnect_storage()).db_needs_connection(),
            ),
        ];
        let hidden: Vec<&str> = cases
            .iter()
            .filter(|(_, forwarded)| !forwarded)
            .map(|(wrapper, _)| *wrapper)
            .collect();
        assert!(
            hidden.is_empty(),
            "wrappers that hide a disconnect: {hidden:?}"
        );
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod restart_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A worker whose every turn is refused by a rate limit.
    struct RateLimited {
        runs: Arc<AtomicUsize>,
        /// The rejection each turn fails with.
        status: tonic::Status,
    }

    #[xmtp_common::async_trait]
    impl Worker for RateLimited {
        fn kind(&self) -> WorkerKind {
            WorkerKind::CommitLog
        }

        async fn run_tasks(&mut self) -> WorkerResult<()> {
            self.runs.fetch_add(1, Ordering::SeqCst);
            let refused = xmtp_api::ApiError::Api(xmtp_proto::api::NetworkError::new(
                xmtp_api_grpc::error::GrpcError::Status(self.status.clone()),
            ));
            Err(Box::new(crate::groups::commit_log::CommitLogError::Api(
                refused,
            )))
        }
    }

    /// A worker that failed with a rate limit restarts after the delay, not
    /// after the one-second restart delay.
    // verifies: RATE-041
    #[xmtp_common::test(unwrap_try = true)]
    async fn a_rate_limited_worker_restarts_after_the_delay() {
        tokio::time::pause();
        let runs = Arc::new(AtomicUsize::new(0));
        let cancel = CancellationToken::new();
        let worker = Box::new(RateLimited {
            runs: runs.clone(),
            status: xmtp_api_grpc::error::rate_limited_status("request", Duration::from_secs(300)),
        });
        let handle = tokio::spawn(worker.spawn(cancel.clone()));
        tokio::time::sleep(Duration::from_secs(10)).await;
        assert_eq!(runs.load(Ordering::SeqCst), 1);
        tokio::time::sleep(Duration::from_secs(300)).await;
        assert_eq!(runs.load(Ordering::SeqCst), 2);
        cancel.cancel();
        handle.await?;
    }

    /// A delay too large for the clock holds the restart without a panic,
    /// which a release build would turn into an abort.
    // verifies: RATE-041
    #[xmtp_common::test(unwrap_try = true)]
    async fn an_out_of_range_delay_holds_the_restart_without_panicking() {
        tokio::time::pause();
        let runs = Arc::new(AtomicUsize::new(0));
        let cancel = CancellationToken::new();
        let worker = Box::new(RateLimited {
            runs: runs.clone(),
            status: xmtp_api_grpc::error::rate_limited_status_raw(i64::MAX, 0),
        });
        let handle = tokio::spawn(worker.spawn(cancel.clone()));
        tokio::time::sleep(Duration::from_secs(365 * 24 * 60 * 60)).await;
        assert_eq!(runs.load(Ordering::SeqCst), 1);
        cancel.cancel();
        handle.await?;
    }
}
