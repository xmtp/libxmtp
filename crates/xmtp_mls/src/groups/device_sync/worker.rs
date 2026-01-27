use super::{
    DeviceSyncClient, DeviceSyncError, IterWithContent,
    preference_sync::{PreferenceUpdate, store_preference_updates},
};
use crate::{
    client::ClientError,
    context::XmtpSharedContext,
    groups::{
        GroupError,
        device_sync::{AvailableArchive, archive::insert_importer},
    },
    subscriptions::{LocalEvents, SyncWorkerEvent},
    worker::{
        BoxedWorker, DynMetrics, MetricsCasting, Worker, WorkerFactory, WorkerKind, WorkerResult,
        metrics::WorkerMetrics,
    },
};
use futures::{StreamExt, TryFutureExt};
use rand::Rng;
use std::{sync::Arc, time::Duration};
use tokio::sync::{OnceCell, broadcast};
use tokio_util::compat::TokioAsyncReadCompatExt;
use tracing::instrument;
use xmtp_archive::{ArchiveImporter, BackupMetadata, exporter::ArchiveExporter};
use xmtp_common::{Event, NS_IN_DAY, fmt::ShortHex, time::now_ns};
use xmtp_db::group_message::{MsgQueryArgs, StoredGroupMessage};
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
const MAX_ATTEMPTS: i32 = 3;

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
                self.client.context.installation_id(),
                server_url = client.context.device_sync().server_url
            );

            // The only thing that sync init really does right now is ensures that there's a sync group.
            if conn.primary_sync_group()?.is_none() {
                log_event!(
                    Event::DeviceSyncNoPrimarySyncGroup,
                    self.client.context.installation_id()
                );
                let sync_group = client.get_sync_group().await?;
                log_event!(
                    Event::DeviceSyncCreatedPrimarySyncGroup,
                    self.client.context.installation_id(),
                    group_id = sync_group.group_id.short_hex()
                );

                // Ask the sync group for a sync payload if the url is present.
                if self.client.context.device_sync_server_url().is_some() {
                    self.client.send_sync_request().await?;
                }
            }

            log_event!(
                Event::DeviceSyncInitializingFinished,
                self.client.context.installation_id()
            );

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
                self.context.installation_id(),
                msg_type,
                external = is_external,
                msg_id = msg.id.short_hex(),
                group_id = msg.group_id.short_hex()
            );

            if let Err(err) = self.process_message(handle, &msg, content).await {
                log_event!(
                    Event::DeviceSyncMessageProcessingError,
                    self.context.installation_id(),
                    err = %err,
                    msg_id = msg.id.short_hex()
                );
                self.context
                    .db()
                    .increment_device_sync_msg_attempt(&msg.id, MAX_ATTEMPTS)?;
            } else {
                self.context
                    .db()
                    .mark_device_sync_msg_as_processed(&msg.id)?;
            }
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

                let Some(server_url) = self.context.device_sync_server_url() else {
                    log_event!(
                        Event::DeviceSyncNoServerUrl,
                        self.context.installation_id(),
                        request_id = request.request_id
                    );
                    return Ok(());
                };

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
                                        server_url: server_url.to_string(),
                                    },
                                ),
                            ),
                        })?,
                );

                // Mark this message as processed immediately.
                self.context
                    .db()
                    .mark_device_sync_msg_as_processed(&msg.id)?;

                handle.increment_metric(SyncMetric::PayloadTaskScheduled);
            }
            ContentProto::Reply(reply) => {
                if !is_external {
                    // Ignore our own messages
                    return Ok(());
                }

                // Check if this reply was asked for by this installation.
                if self.is_reply_requested_by_installation(&reply).await? {
                    self.process_archive(msg, reply).await.inspect_err(
                        |err| log_event!(Event::DeviceSyncArchiveImportFailure, self.context.installation_id(), err = %err),
                    )?;
                } else {
                    log_event!(
                        Event::DeviceSyncArchiveNotRequested,
                        self.context.installation_id()
                    );
                }
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
                    self.context.installation_id(),
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
        log_event!(
            Event::DeviceSyncRequestAcknowledged,
            self.context.installation_id(),
            request_id
        );
        Ok(())
    }

    pub(crate) async fn send_archive(
        &self,
        options: &BackupOptions,
        sync_group_id: &Vec<u8>,
        pin: Option<&str>,
        server_url: &str,
    ) -> Result<String, DeviceSyncError>
    where
        Context::Db: 'static,
    {
        log_event!(
            Event::DeviceSyncArchiveUploadStart,
            self.context.installation_id(),
            group_id = sync_group_id.short_hex(),
            server_url
        );

        let acknowledge = async || {
            if let Some(request_id) = &pin {
                self.acknowledge_sync_request(sync_group_id, request_id)
                    .await?;
            }

            Ok::<_, DeviceSyncError>(())
        };

        // Acknowledge the sync request
        acknowledge().await?;

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
        let url = format!("{server_url}/upload");
        let response = exporter.post_to_url(&url).await?;

        let request_id = pin.map(str::to_string).unwrap_or_else(|| {
            let pin = xmtp_common::rng().gen_range(0..=9999);
            format!("{pin:04}")
        });

        // Build a sync reply message that the new installation will consume
        let reply = DeviceSyncReplyProto {
            encryption_key: Some(DeviceSyncKeyType {
                key: Some(Key::Aes256Gcm(key)),
            }),
            request_id: request_id.clone(),
            url: format!("{server_url}/files/{response}",),
            metadata: Some(metadata),

            // Deprecated fields
            ..Default::default()
        };

        // Check acknowledgement one more time.
        // This ensures we were the first to acknowledge.
        acknowledge().await?;

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

        Ok(request_id)
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

        self.metrics.increment_metric(SyncMetric::RequestSent);
        log_event!(
            Event::DeviceSyncSentSyncRequest,
            self.context.installation_id(),
            group_id = sync_group.group_id.short_hex()
        );

        Ok(())
    }

    pub async fn send_sync_archive(
        &self,
        options: &BackupOptions,
        server_url: &str,
        pin: Option<&str>,
    ) -> Result<String, ClientError>
    where
        Context::Db: 'static,
    {
        let sync_group = self.get_sync_group().await?;
        sync_group
            .sync_with_conn()
            .await
            .map_err(GroupError::from)?;

        let pin = self
            .send_archive(options, &sync_group.group_id, pin, server_url)
            .await
            .map_err(|e| GroupError::DeviceSync(Box::new(e)))?;

        Ok(pin)
    }

    async fn is_reply_requested_by_installation(
        &self,
        reply: &DeviceSyncReplyProto,
    ) -> Result<bool, DeviceSyncError> {
        let sync_group = self.get_sync_group().await?;
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

    /// Processes sync archive with a matching pin. If no pin is provided, will process latest archive.
    pub async fn process_archive_with_pin(&self, pin: Option<&str>) -> Result<(), DeviceSyncError> {
        let mut offset = 0;
        let mut messages = vec![];
        loop {
            messages = self.context.db().sync_group_messages_paged(offset, 100)?;
            if messages.is_empty() {
                break;
            }

            offset += messages.len() as i64;
            for (msg, content) in messages.iter_with_content() {
                let reply = match (pin, content) {
                    (None, ContentProto::Reply(reply)) => reply,
                    (Some(pin), ContentProto::Reply(reply)) if reply.request_id == pin => reply,
                    _ => continue,
                };

                return self.process_archive(&msg, reply).await;
            }
        }

        Err(DeviceSyncError::MissingPayload(pin.map(str::to_string)))
    }

    pub fn list_available_archives(
        &self,
        days_cutoff: i64,
    ) -> Result<Vec<AvailableArchive>, DeviceSyncError> {
        let mut offset = 0;
        let mut messages = vec![];
        let mut result = vec![];
        let cutoff = now_ns() - days_cutoff * NS_IN_DAY;

        'outer: loop {
            messages = self.context.db().sync_group_messages_paged(offset, 100)?;

            if messages.is_empty() {
                break;
            }
            offset += messages.len() as i64;

            for (msg, content) in messages.iter_with_content() {
                if msg.sent_at_ns < cutoff {
                    break 'outer;
                }

                let ContentProto::Reply(reply) = content else {
                    continue;
                };

                let Some(metadata) = reply.metadata else {
                    tracing::warn!(
                        "Came across a device sync reply message with no metadata. request_id: {}",
                        reply.request_id
                    );
                    continue;
                };

                let metadata = BackupMetadata::from_metadata_version_unknown(metadata);
                result.push(AvailableArchive {
                    request_id: reply.request_id,
                    metadata,
                    sent_by_installation: msg.sender_installation_id,
                });
            }
        }

        Ok(result)
    }

    pub async fn process_archive(
        &self,
        msg: &StoredGroupMessage,
        reply: DeviceSyncReplyProto,
    ) -> Result<(), DeviceSyncError> {
        log_event!(
            Event::DeviceSyncArchiveProcessingStart,
            self.context.installation_id(),
            msg_id = msg.id.short_hex(),
            group_id = msg.group_id.short_hex()
        );
        if reply.kind() != BackupElementSelection::Unspecified {
            log_event!(Event::DeviceSyncV1Archive, self.context.installation_id());
            // This is a legacy payload, the legacy function will process it.
            return Ok(());
        }

        self.welcome_service.sync_welcomes().await?;

        // Get a download stream of the payload.
        log_event!(
            Event::DeviceSyncArchiveDownloading,
            self.context.installation_id()
        );
        let response = reqwest::Client::new().get(reply.url).send().await?;
        if let Err(err) = response.error_for_status_ref() {
            log_event!(
                Event::DeviceSyncPayloadDownloadFailure,
                self.context.installation_id(),
                status = %response.status(),
                err = %err
            );
            return Err(DeviceSyncError::Reqwest(err));
        }

        log_event!(
            Event::DeviceSyncArchiveImportStart,
            self.context.installation_id()
        );

        let stream = response
            .bytes_stream()
            .map(|result| result.map_err(std::io::Error::other));
        // Convert that stream into a reader
        let tokio_reader = tokio_util::io::StreamReader::new(stream);
        // Convert that tokio reader into a futures reader.
        // We use futures reader for WASM compat.
        let reader = tokio_reader.compat();

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

        log_event!(
            Event::DeviceSyncArchiveImportSuccess,
            self.context.installation_id()
        );
        Ok(())
    }
}

#[derive(PartialEq, Eq, Hash, Clone, Copy, Debug)]
pub enum SyncMetric {
    Init,
    SyncGroupCreated,
    SyncGroupWelcomesProcessed,
    RequestReceived,
    RequestSent,
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
