use super::{
    DeviceSyncClient, DeviceSyncError, IterWithContent,
    preference_sync::{PreferenceUpdate, store_preference_updates},
};
use crate::{
    client::ClientError,
    context::XmtpSharedContext,
    groups::{GroupError, device_sync::archive::insert_importer},
    subscriptions::{LocalEvents, SyncWorkerEvent},
    worker::{
        BoxedWorker, DynMetrics, MetricsCasting, Worker, WorkerFactory, WorkerKind, WorkerResult,
        metrics::WorkerMetrics,
    },
};
use futures::TryFutureExt;
use std::{sync::Arc, time::Duration};
use tokio::sync::{OnceCell, broadcast};
#[cfg(not(target_arch = "wasm32"))]
use tokio_util::compat::TokioAsyncReadCompatExt;
use tracing::instrument;
use xmtp_archive::{ArchiveImporter, exporter::ArchiveExporter};
use xmtp_common::{Event, fmt::TruncatedHex};
use xmtp_db::{
    StoreOrIgnore,
    group_message::{MsgQueryArgs, StoredGroupMessage},
    processed_device_sync_messages::StoredProcessedDeviceSyncMessages,
};
use xmtp_db::{prelude::*, tasks::NewTask};
use xmtp_macro::log_event;
use xmtp_proto::{
    ConversionError,
    xmtp::{
        device_sync::{
            BackupElementSelection, BackupOptions,
            content::{
                DeviceSyncAcknowledge, DeviceSyncKeyType, DeviceSyncReply as DeviceSyncReplyProto,
                DeviceSyncRequest as DeviceSyncRequestProto,
                PreferenceUpdates as PreferenceUpdatesProto,
                device_sync_content::Content as ContentProto, device_sync_key_type::Key,
            },
        },
        mls::database::{SendSyncArchive, Task},
    },
};

const ENC_KEY_SIZE: usize = xmtp_archive::ENC_KEY_SIZE;

pub struct SyncWorker<Context> {
    client: DeviceSyncClient<Context>,
    receiver: broadcast::Receiver<SyncWorkerEvent>,
    init: OnceCell<()>,
    metrics: Arc<WorkerMetrics<SyncMetric>>,
}

impl<Context> SyncWorker<Context>
where
    Context: XmtpSharedContext + 'static,
{
    pub fn new(context: Context, metrics: Option<DynMetrics>) -> Self {
        let receiver = context.worker_events().subscribe();
        let metrics = metrics
            .and_then(|m| m.as_sync_metrics())
            .unwrap_or(Arc::new(WorkerMetrics::new(context.installation_id())));
        let client = DeviceSyncClient::new(context, metrics.clone());

        Self {
            client,
            receiver,
            init: OnceCell::new(),
            metrics,
        }
    }
}

struct Factory<Context> {
    context: Context,
}

impl<Context> WorkerFactory for Factory<Context>
where
    Context: XmtpSharedContext + 'static,
{
    fn create(&self, metrics: Option<DynMetrics>) -> (BoxedWorker, Option<DynMetrics>) {
        let worker = SyncWorker::new(self.context.clone(), metrics);
        let metrics = worker.metrics.clone();

        (Box::new(worker) as Box<_>, Some(metrics as Arc<_>))
    }

    fn kind(&self) -> WorkerKind {
        WorkerKind::DeviceSync
    }
}

#[xmtp_common::async_trait]
impl<Context> Worker for SyncWorker<Context>
where
    Context: XmtpSharedContext + 'static,
{
    fn kind(&self) -> WorkerKind {
        WorkerKind::DeviceSync
    }

    fn metrics(&self) -> Option<DynMetrics> {
        Some(self.metrics.clone())
    }

    fn factory<C>(context: C) -> impl WorkerFactory + 'static
    where
        C: XmtpSharedContext + 'static,
    {
        Factory { context }
    }

    async fn run_tasks(&mut self) -> WorkerResult<()> {
        self.run().map_err(|e| Box::new(e) as Box<_>).await
    }
}

impl<Context> SyncWorker<Context>
where
    Context: XmtpSharedContext + 'static,
{
    async fn run(&mut self) -> Result<(), DeviceSyncError> {
        // Wait for the identity to be ready & verified before doing anything
        while !self.client.context.identity().is_ready() {
            xmtp_common::task::yield_now().await
        }
        self.sync_init().await?;
        self.metrics.increment_metric(SyncMetric::Init);

        let tick_fut = Self::tick(self.client.context.clone());
        let run_fut = self.run_internal();

        tokio::select! {
            _ = tick_fut => Ok(()),
            res = run_fut => res,
        }
    }

    async fn run_internal(&mut self) -> Result<(), DeviceSyncError> {
        while let Ok(event) = self.receiver.recv().await {
            tracing::info!(
                "[{}] New event: {event:?}",
                self.client.context.installation_id()
            );

            match event {
                SyncWorkerEvent::NewSyncGroupFromWelcome(_group_id) => {
                    self.evt_new_sync_group_from_welcome().await?;
                }
                SyncWorkerEvent::NewSyncGroupMsg => {
                    self.evt_new_sync_group_msg(false).await?;
                }
                SyncWorkerEvent::Tick => {
                    self.evt_new_sync_group_msg(true).await?;
                }
                SyncWorkerEvent::SyncPreferences(preference_updates) => {
                    self.evt_sync_preferences(preference_updates).await?;
                }
                SyncWorkerEvent::CycleHMAC => {
                    self.evt_cycle_hmac().await?;
                }
            }
        }
        Ok(())
    }

    async fn tick(ctx: Context) {
        loop {
            xmtp_common::time::sleep(Duration::from_secs(20)).await;

            // We don't need to worry about a mutex lock for device sync
            // to ensure that a sync payload is not being processed by two
            // threads at once because there should only ever be one sync worker
            // and the sync worker processes all events in series.
            let _ = ctx.worker_events().send(SyncWorkerEvent::Tick);
        }
    }

    //// Ideally called when the client is registered.
    //// Will auto-send a sync request if sync group is created.
    #[instrument(level = "trace", skip_all)]
    async fn sync_init(&mut self) -> Result<(), DeviceSyncError> {
        let Self { init, client, .. } = &self;

        init.get_or_try_init(|| async {
            let conn = self.client.context.db();
            log_event!(
                Event::DeviceSyncInitializing,
                server_url = client.context.device_sync().server_url
            );

            // The only thing that sync init really does right now is ensures that there's a sync group.
            if conn.primary_sync_group()?.is_none() {
                log_event!(Event::DeviceSyncNoPrimarySyncGroup);
                let sync_group = client.get_sync_group().await?;
                log_event!(
                    Event::DeviceSyncCreatedPrimarySyncGroup,
                    group_id = sync_group.group_id.short_hex()
                );

                // Ask the sync group for a sync payload if the url is present.
                if self.client.context.device_sync_server_url().is_some() {
                    self.client.send_sync_request().await?;
                }
            }

            log_event!(Event::DeviceSyncInitializingFinished);

            Ok(())
        })
        .await
        .copied()
    }

    async fn evt_new_sync_group_from_welcome(&self) -> Result<(), DeviceSyncError> {
        tracing::info!("New sync group from welcome detected.");

        // A new sync group from a welcome indicates a new installation.
        // We need to add that installation to the groups.
        self.client.add_new_installation_to_groups().await?;

        self.metrics
            .increment_metric(SyncMetric::SyncGroupWelcomesProcessed);

        // Cycle the HMAC
        self.client.cycle_hmac().await?;

        Ok(())
    }

    async fn evt_new_sync_group_msg(&self, is_tick: bool) -> Result<(), DeviceSyncError> {
        let unprocessed_messages = self.client.context.db().unprocessed_sync_group_messages()?;

        if !is_tick || !unprocessed_messages.is_empty() {
            tracing::info!("Processing {} messages.", unprocessed_messages.len());
        }

        self.client
            .process_sync_group_messages(&self.metrics, unprocessed_messages)
            .await
    }

    async fn evt_sync_preferences(
        &self,
        updates: Vec<PreferenceUpdate>,
    ) -> Result<(), DeviceSyncError> {
        let updates = self.client.sync_preferences(updates).await?;

        updates.iter().for_each(|update| match update {
            PreferenceUpdate::Consent(_) => self.metrics.increment_metric(SyncMetric::ConsentSent),
            PreferenceUpdate::Hmac { .. } => self.metrics.increment_metric(SyncMetric::HmacSent),
        });
        Ok(())
    }

    async fn evt_cycle_hmac(&self) -> Result<(), DeviceSyncError> {
        self.client.cycle_hmac().await?;
        Ok(())
    }
}

impl<Context> DeviceSyncClient<Context>
where
    Context: XmtpSharedContext,
{
    async fn process_sync_group_messages(
        &self,
        handle: &WorkerMetrics<SyncMetric>,
        messages: Vec<StoredGroupMessage>,
    ) -> Result<(), DeviceSyncError>
    where
        Context::Db: 'static,
    {
        let installation_id = self.installation_id();

        for (msg, content) in messages.clone().iter_with_content() {
            let is_external = msg.sender_installation_id != installation_id;

            let msg_type = match &content {
                ContentProto::Request(_) => "Request",
                ContentProto::Reply(_) => "Reply",
                ContentProto::PreferenceUpdates(_) => "PreferenceUpdates",
                ContentProto::Acknowledge(_) => "Acknowledge",
            };

            log_event!(
                Event::DeviceSyncProcessingMessages,
                msg_type,
                external = is_external,
                msg_id = msg.id.short_hex(),
                group_id = msg.group_id.short_hex()
            );

            if let Err(err) = self.process_message(handle, &msg, content).await {
                log_event!(
                    Event::DeviceSyncMessageProcessingError,
                    err = %err,
                    msg_id = msg.id.short_hex()
                );
            };
        }

        for msg in messages {
            StoredProcessedDeviceSyncMessages { message_id: msg.id }
                .store_or_ignore(&self.context.db())?;
        }

        Ok(())
    }

    async fn process_message(
        &self,
        handle: &WorkerMetrics<SyncMetric>,
        msg: &StoredGroupMessage,
        content: ContentProto,
    ) -> Result<(), DeviceSyncError>
    where
        Context::Db: 'static,
    {
        let conn = self.context.db();
        let installation_id = self.context.installation_id();
        let is_external = msg.sender_installation_id != installation_id;

        match content {
            ContentProto::Request(request) => {
                if !is_external {
                    // Ignore our own messages
                    return Ok(());
                }

                self.context.task_channels().send(
                    NewTask::builder()
                        .originating_message_originator_id(msg.originator_id as i32)
                        .originating_message_sequence_id(msg.sequence_id)
                        .build(Task {
                            task: Some(
                                xmtp_proto::xmtp::mls::database::task::Task::SendSyncArchive(
                                    SendSyncArchive {
                                        options: request.options,
                                        request_id: Some(request.request_id),
                                        sync_group_id: msg.group_id.clone(),
                                    },
                                ),
                            ),
                        })?,
                );

                // Mark this message as processed immediately.
                StoredProcessedDeviceSyncMessages {
                    message_id: msg.id.clone(),
                }
                .store_or_ignore(&self.context.db())?;

                handle.increment_metric(SyncMetric::PayloadTaskScheduled);
            }
            ContentProto::Reply(reply) => {
                if !is_external {
                    // Ignore our own messages
                    return Ok(());
                }
                self.process_sync_payload(msg, reply).await.inspect_err(
                    |err| log_event!(Event::DeviceSyncArchiveImportFailure, err = %err),
                )?;
                handle.increment_metric(SyncMetric::PayloadProcessed);
            }
            ContentProto::PreferenceUpdates(PreferenceUpdatesProto { updates }) => {
                if is_external {
                    tracing::info!("Incoming preference updates: {updates:?}");
                }
                tracing::info!(
                    "{} storing preference updates",
                    self.context.installation_id()
                );
                // We'll process even our own messages here. The sync group message ordering takes authority over our own here.
                let updated = store_preference_updates(updates.clone(), &conn, handle)?;
                if !updated.is_empty() {
                    let _ = self
                        .context
                        .local_events()
                        .send(LocalEvents::PreferencesChanged(updated));
                }
            }
            ContentProto::Acknowledge(DeviceSyncAcknowledge { .. }) => {
                return Ok(());
            }
        }

        Ok(())
    }

    /// Acknowledge a sync request.
    /// Returns an error if request is already acknowledged by another installation.
    /// The first installation to acknowledge the sync request will be the installation to handle the response.
    pub async fn acknowledge_sync_request(
        &self,
        sync_group_id: &Vec<u8>,
        request_id: &str,
    ) -> Result<(), DeviceSyncError> {
        let sync_group = self.mls_store.group(sync_group_id)?;
        // Pull down any new messages
        sync_group.sync_with_conn().await?;

        let messages = sync_group.find_messages(&MsgQueryArgs::default())?;

        // Look in reverse for a request, and ensure it was not acknowledged by someone else.
        for (message, content) in messages.iter_with_content().rev() {
            let ContentProto::Acknowledge(acknowledge) = content else {
                continue;
            };
            if acknowledge.request_id != request_id {
                continue;
            }

            if message.sender_installation_id != self.installation_id() {
                // Request has already been acknowledged by another installation.
                // Let that installation handle it.
                log_event!(
                    Event::DeviceSyncRequestAlreadyAcknowledged,
                    request_id,
                    acknowledged_by = message.sender_installation_id.short_hex()
                );
                return Err(DeviceSyncError::AlreadyAcknowledged);
            }

            return Ok(());
        }

        // Acknowledge and break.
        self.send_device_sync_message(ContentProto::Acknowledge(DeviceSyncAcknowledge {
            request_id: request_id.to_string(),
        }))
        .await?;
        log_event!(Event::DeviceSyncRequestAcknowledged, request_id);
        Ok(())
    }

    pub(crate) async fn send_archive(
        &self,
        options: &BackupOptions,
        sync_group_id: &Vec<u8>,
        request_id: Option<&str>,
    ) -> Result<(), DeviceSyncError>
    where
        Context::Db: 'static,
    {
        log_event!(
            Event::DeviceSyncArchiveUploadStart,
            group_id = sync_group_id.short_hex()
        );
        let Some(device_sync_server_url) = &self.context.device_sync().server_url else {
            tracing::info!("No message history payload sent - server url not present.");
            return Ok(());
        };

        let acknowledge = async || {
            if let Some(request_id) = &request_id {
                match self
                    .acknowledge_sync_request(sync_group_id, request_id)
                    .await
                {
                    Err(DeviceSyncError::AlreadyAcknowledged) => return Ok(false),
                    result => result?,
                }
            }

            Ok::<_, DeviceSyncError>(true)
        };

        // Acknowledge the sync request
        if !acknowledge().await? {
            return Ok(());
        };

        // Generate a random encryption key
        let key = xmtp_common::rand_vec::<32>();

        tracing::info!("Building the exporter.");
        // Now we want to create an encrypted stream from our database to the history server.
        //
        // 1. Build the exporter
        let db = self.context.db();
        let exporter = ArchiveExporter::new(options.clone(), db, &key);
        let metadata = exporter.metadata().clone();

        tracing::info!("Uploading the archive.");
        // 5. Make the request
        let url = format!("{device_sync_server_url}/upload");
        let response = exporter.post_to_url(&url).await?;

        // Build a sync reply message that the new installation will consume
        let reply = DeviceSyncReplyProto {
            encryption_key: Some(DeviceSyncKeyType {
                key: Some(Key::Aes256Gcm(key)),
            }),
            request_id: request_id.map(str::to_string).unwrap_or_default(),
            url: format!("{device_sync_server_url}/files/{response}",),
            metadata: Some(metadata),

            // Deprecated fields
            ..Default::default()
        };

        // Check acknowledgement one more time.
        // This ensures we were the first to acknowledge.
        if !acknowledge().await? {
            return Ok(());
        };

        tracing::info!("Sending sync request reply message.");
        // Send the message out over the network
        self.send_device_sync_message(ContentProto::Reply(reply))
            .await?;

        // Update metrics.
        if options
            .elements
            .contains(&(BackupElementSelection::Consent as i32))
        {
            self.metrics
                .increment_metric(SyncMetric::ConsentPayloadSent);
        }
        if options
            .elements
            .contains(&(BackupElementSelection::Messages as i32))
        {
            self.metrics
                .increment_metric(SyncMetric::MessagesPayloadSent);
        }
        self.metrics.increment_metric(SyncMetric::PayloadSent);

        Ok(())
    }

    pub async fn send_sync_request(&self) -> Result<(), ClientError> {
        let sync_group = self.get_sync_group().await?;
        sync_group
            .sync_with_conn()
            .await
            .map_err(GroupError::from)?;

        let request = DeviceSyncRequestProto {
            request_id: xmtp_common::rand_string::<ENC_KEY_SIZE>(),
            options: Some(BackupOptions {
                elements: vec![
                    BackupElementSelection::Messages as i32,
                    BackupElementSelection::Consent as i32,
                ],
                ..Default::default()
            }),

            // Deprecated fields
            ..Default::default()
        };

        self.send_device_sync_message(ContentProto::Request(request))
            .await?;

        log_event!(
            Event::DeviceSyncSentSyncRequest,
            group_id = sync_group.group_id.short_hex()
        );

        Ok(())
    }

    async fn is_reply_requested_by_installation(
        &self,
        reply: &DeviceSyncReplyProto,
    ) -> Result<bool, DeviceSyncError> {
        let sync_group = self.get_sync_group().await?;
        let stored_group = self.context.db().find_group(&sync_group.group_id)?;
        let Some(stored_group) = stored_group else {
            return Err(DeviceSyncError::MissingSyncGroup);
        };

        if reply.request_id == stored_group.added_by_inbox_id {
            return Ok(true);
        }

        let messages = sync_group.find_messages(&MsgQueryArgs::default())?;

        for (msg, content) in messages.iter_with_content() {
            if let ContentProto::Request(DeviceSyncRequestProto { request_id, .. }) = content
                && *request_id == reply.request_id
                && msg.sender_installation_id == self.installation_id()
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub async fn process_sync_payload(
        &self,
        msg: &StoredGroupMessage,
        reply: DeviceSyncReplyProto,
    ) -> Result<(), DeviceSyncError> {
        log_event!(
            Event::DeviceSyncArchiveProcessingStart,
            msg_id = msg.id.short_hex(),
            group_id = msg.group_id.short_hex()
        );
        if reply.kind() != BackupElementSelection::Unspecified {
            log_event!(Event::DeviceSyncV1Archive);
            // This is a legacy payload, the legacy function will process it.
            return Ok(());
        }

        tracing::info!("Inspecting sync payload.");

        // Check if this reply was asked for by this installation.
        if !self.is_reply_requested_by_installation(&reply).await? {
            // This installation didn't ask for it. Ignore the reply.
            log_event!(Event::DeviceSyncArchiveNotRequested);
            return Ok(());
        }

        // If a payload was sent to this installation,
        // that means they also sent this installation a bunch of welcomes.
        log_event!(Event::DeviceSyncArchiveAccepted);
        self.welcome_service.sync_welcomes().await?;

        // Get a download stream of the payload.
        log_event!(Event::DeviceSyncArchiveDownloading);
        let response = reqwest::Client::new().get(reply.url).send().await?;
        if let Err(err) = response.error_for_status_ref() {
            log_event!(
                Event::DeviceSyncPayloadDownloadFailure,
                status = %response.status(),
                err = %err
            );
            return Err(DeviceSyncError::Reqwest(err));
        }

        log_event!(Event::DeviceSyncArchiveImportStart);
        #[cfg(not(target_arch = "wasm32"))]
        let reader = {
            use futures::StreamExt;
            let stream = response
                .bytes_stream()
                .map(|result| result.map_err(std::io::Error::other));

            // Convert that stream into a reader
            let tokio_reader = tokio_util::io::StreamReader::new(stream);
            // Convert that tokio reader into a futures reader.
            // We use futures reader for WASM compat.
            tokio_reader.compat()
        };
        #[cfg(target_arch = "wasm32")]
        let reader = {
            // WASM doesn't support request streaming. Consume the response instead.
            futures::io::Cursor::new(response.bytes().await?)
        };

        // Create an importer around that futures_reader.
        let Some(DeviceSyncKeyType {
            key: Some(Key::Aes256Gcm(key)),
        }) = reply.encryption_key
        else {
            return Err(ConversionError::Unspecified("encryption_key"))?;
        };

        let mut importer = ArchiveImporter::load(Box::pin(reader), &key).await?;

        tracing::info!("Importing the sync payload.");
        // Run the import.
        insert_importer(&mut importer, &self.context).await?;

        log_event!(Event::DeviceSyncArchiveImportSuccess);
        Ok(())
    }
}

#[derive(PartialEq, Eq, Hash, Clone, Copy, Debug)]
pub enum SyncMetric {
    Init,
    SyncGroupCreated,
    SyncGroupWelcomesProcessed,
    RequestReceived,
    ConsentPayloadSent,
    ConsentPayloadProcessed,
    MessagesPayloadSent,
    MessagesPayloadProcessed,
    PayloadSent,
    PayloadTaskScheduled,
    PayloadProcessed,
    HmacSent,
    HmacReceived,
    ConsentSent,
    ConsentReceived,
}

impl WorkerMetrics<SyncMetric> {
    pub async fn wait_for_init(&self) -> Result<(), xmtp_common::time::Expired> {
        self.register_interest(SyncMetric::Init, 1).wait().await
    }
}
