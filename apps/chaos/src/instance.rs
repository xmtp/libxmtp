//! One process owns one client. Process exit stops every retained worker.
use crate::{
    faults::disk::{ChaosStore, DiskControl, DiskFault, FaultConnection, FaultDb},
    protocol::*,
};
use alloy_signer_local::PrivateKeySigner;
use anyhow::{Context, Result, bail, ensure};
use futures::StreamExt;
use serde::Serialize;
use serde_json::{Value, json};
use std::{collections::BTreeSet, sync::Arc};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    sync::Mutex,
    task::JoinHandle,
};
use tracing::Instrument;
use xmtp_api_backend::{BackendClient, definitions::ApiClient};
use xmtp_common::{
    ErrorCode, ExponentialBackoff, Retry, RetryableError, retry_async,
    time::{Duration, timeout},
};
use xmtp_db::{
    ConnectionExt, DbConnection, NativeDb, consent_record::ConsentState, group::GroupQueryArgs,
    sql_key_store::SqlKeyStore,
};
use xmtp_id::InboxOwner;
use xmtp_mls::{Client, context::XmtpMlsLocalContext, identity::IdentityStrategy};
use xmtp_proto::{api::ToBoxedClient, types::GroupId};

type ChaosClient = Client<
    Arc<
        XmtpMlsLocalContext<Arc<ApiClient>, ChaosStore, SqlKeyStore<DbConnection<FaultConnection>>>,
    >,
>;
type ChaosGroup = xmtp_mls::groups::MlsGroup<
    Arc<
        XmtpMlsLocalContext<Arc<ApiClient>, ChaosStore, SqlKeyStore<DbConnection<FaultConnection>>>,
    >,
>;
const STREAM_TOKEN_CAP: usize = 4096;
const STREAM_ERROR_CAP: usize = 16;

fn published_token_bytes(
    connection: &mut xmtp_db::diesel::SqliteConnection,
    wanted: &BTreeSet<String>,
) -> xmtp_db::diesel::QueryResult<Vec<Vec<u8>>> {
    use xmtp_db::diesel::prelude::*;
    use xmtp_db::schema::group_messages::dsl;

    // Deferred tokens can be older than the newest message page.
    dsl::group_messages
        .filter(dsl::delivery_status.eq(xmtp_db::group_message::DeliveryStatus::Published))
        .filter(dsl::decrypted_message_bytes.eq_any(wanted.iter().map(String::as_bytes)))
        .select(dsl::decrypted_message_bytes)
        .distinct()
        .limit(STREAM_TOKEN_CAP as i64)
        .load(connection)
}

#[derive(Debug, thiserror::Error)]
#[error(transparent)]
struct LeaseOpenError(xmtp_mls::subscriptions::SubscribeError);

impl RetryableError for LeaseOpenError {
    fn is_retryable(&self) -> bool {
        self.0.error_code()
            == xmtp_db::stream_storage::StreamStorageError::AlreadyActive.error_code()
    }
}

async fn open_after_lease<T, F, Fut>(mut open: F, budget: Duration, poll: Duration) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = std::result::Result<T, xmtp_mls::subscriptions::SubscribeError>>,
{
    let retries = usize::try_from(budget.as_nanos() / poll.as_nanos())?;
    let retry = Retry::builder()
        .retries(retries)
        .with_strategy(
            ExponentialBackoff::builder()
                .duration(poll)
                .multiplier(1)
                .max_jitter(Duration::ZERO)
                .total_wait_max(budget)
                .build(),
        )
        .build();
    let span = tracing::info_span!("chaos.stream.open", lease_waits = 0_u64);
    let open_span = span.clone();
    let mut lease_waits = 0_u64;
    timeout(
        budget,
        async {
            retry_async!(
                retry,
                (async {
                    let result = open().await.map_err(LeaseOpenError);
                    if let Err(error) = &result
                        && error.is_retryable()
                    {
                        lease_waits += 1;
                        open_span.record("lease_waits", lease_waits);
                        tracing::debug!(
                            error_code = error.0.error_code(),
                            lease_waits,
                            "waiting for previous stream lease to expire"
                        );
                    }
                    result
                })
            )
        }
        .instrument(span),
    )
    .await
    .context("message stream open exceeded the consumer lease deadline")?
    .map_err(|error| anyhow::Error::new(error.0))
}

#[derive(Default)]
struct StreamTask {
    enabled: bool,
    task: Option<JoinHandle<()>>,
}

impl StreamTask {
    async fn recover<F, Fut>(&mut self, open: F) -> Result<bool>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<JoinHandle<()>>>,
    {
        if !self.enabled || self.task.as_ref().is_some_and(|task| !task.is_finished()) {
            return Ok(false);
        }
        if let Some(task) = self.task.take() {
            // Completion also drops the old iterator and releases its lease.
            task.await.context("message stream task failed")?;
        }
        self.task = Some(open().await?);
        Ok(true)
    }
}

#[derive(Clone, Serialize)]
struct StreamErrorRecord {
    code: &'static str,
    retryable: bool,
}

#[derive(Clone, Default, Serialize)]
struct StreamDiagnostics {
    opens: u64,
    reopens: u64,
    eof_count: u64,
    error_count: u64,
    errors: Vec<StreamErrorRecord>,
}

pub(crate) fn all_consent() -> Vec<ConsentState> {
    vec![
        ConsentState::Allowed,
        ConsentState::Unknown,
        ConsentState::Denied,
    ]
}

struct State {
    client: ChaosClient,
    disk: DiskControl,
    stream: Mutex<StreamTask>,
    tokens: Arc<Mutex<BTreeSet<String>>>,
    stream_diagnostics: Arc<Mutex<StreamDiagnostics>>,
}

impl State {
    async fn stream(&self, enabled: bool) -> Result<Value> {
        let mut handle = self.stream.lock().await;
        handle.enabled = enabled;
        if let Some(task) = handle.task.take() {
            task.abort();
            let _ = task.await;
        }
        if enabled {
            handle.task = Some(self.open_stream().await?);
        }
        Ok(json!({"stream":enabled}))
    }

    async fn recover_stream(&self) -> Result<()> {
        let reopened = self
            .stream
            .lock()
            .await
            .recover(|| self.open_stream())
            .await?;
        if reopened {
            let mut diagnostics = self.stream_diagnostics.lock().await;
            diagnostics.reopens += 1;
            tracing::info!(
                reopens = diagnostics.reopens,
                "message stream reopened after completion"
            );
        }
        Ok(())
    }

    async fn open_stream(&self) -> Result<JoinHandle<()>> {
        let stream = open_after_lease(
            || {
                self.client
                    .stream_all_messages_owned(None, Some(all_consent()))
            },
            xmtp_configuration::DEFAULT_CONSUMER_LEASE_DURATION
                + xmtp_configuration::ACTIVE_DATABASE_POLL_INTERVAL,
            xmtp_configuration::ACTIVE_DATABASE_POLL_INTERVAL,
        )
        .await?;
        let tokens = self.tokens.clone();
        let diagnostics = self.stream_diagnostics.clone();
        let generation = {
            let mut diagnostics = diagnostics.lock().await;
            diagnostics.opens += 1;
            diagnostics.opens
        };
        let span = tracing::info_span!(
            "chaos.stream",
            generation,
            eof = false,
            error_count = 0_u64,
            error_code = tracing::field::Empty,
            retryable = tracing::field::Empty,
            otel.status_code = tracing::field::Empty,
        );
        Ok(tokio::spawn(
            async move {
                futures::pin_mut!(stream);
                let mut error_count = 0_u64;
                while let Some(item) = stream.next().await {
                    match item {
                        Ok(message) => {
                            if let Ok(token) = String::from_utf8(message.decrypted_message_bytes)
                                && token.starts_with("xchaos:")
                            {
                                let mut seen = tokens.lock().await;
                                if seen.len() >= STREAM_TOKEN_CAP {
                                    seen.pop_first();
                                }
                                seen.insert(token);
                            }
                        }
                        Err(error) => {
                            error_count += 1;
                            let record = StreamErrorRecord {
                                code: error.error_code(),
                                retryable: error.is_retryable(),
                            };
                            let span = tracing::Span::current();
                            span.record("error_count", error_count);
                            span.record("error_code", record.code);
                            span.record("retryable", record.retryable);
                            span.record("otel.status_code", "ERROR");
                            tracing::warn!(
                                error_code = record.code,
                                retryable = record.retryable,
                                "message stream yielded an error"
                            );
                            let mut diagnostics = diagnostics.lock().await;
                            diagnostics.error_count += 1;
                            if diagnostics.errors.len() == STREAM_ERROR_CAP {
                                diagnostics.errors.remove(0);
                            }
                            diagnostics.errors.push(record);
                        }
                    }
                }
                diagnostics.lock().await.eof_count += 1;
                tracing::Span::current().record("eof", true);
                tracing::info!(error_count, "message stream reached EOF");
            }
            .instrument(span),
        ))
    }

    fn groups(&self) -> Result<Vec<ChaosGroup>> {
        Ok(self.client.find_groups(GroupQueryArgs {
            consent_states: Some(all_consent()),
            ..Default::default()
        })?)
    }

    fn group(&self, id: &str) -> Result<ChaosGroup> {
        Ok(self
            .client
            .group(&GroupId::try_from(hex::decode(id)?.as_slice())?)?)
    }

    async fn operate(&self, operation: Operation) -> Result<Value> {
        match operation {
            Operation::Create { members } => {
                let group = self
                    .client
                    .create_group_with_members(&members, None, None)
                    .await?;
                Ok(json!({"group":hex::encode(group.group_id)}))
            }
            Operation::Add { group, inbox } => {
                self.group(&group)?.add_members(&[inbox]).await?;
                Ok(json!({}))
            }
            Operation::Remove { group, inbox } => {
                self.group(&group)?
                    .remove_members(&[inbox.as_str()])
                    .await?;
                Ok(json!({}))
            }
            Operation::Readd { group, inbox } => {
                let group = self.group(&group)?;
                group.remove_members(&[inbox.as_str()]).await?;
                group.add_members(&[inbox]).await?;
                Ok(json!({}))
            }
            Operation::Metadata { group, value } => {
                self.group(&group)?.update_group_name(value).await?;
                Ok(json!({}))
            }
            Operation::Send { group, token } => {
                let id = self
                    .group(&group)?
                    .send_message(token.as_bytes(), Default::default())
                    .await?;
                Ok(json!({"message":hex::encode(id)}))
            }
            Operation::PendingSend { group, token } => {
                let group = self.group(&group)?;
                let id = group.send_message_optimistic(token.as_bytes(), Default::default())?;
                group.publish_messages().await?;
                Ok(json!({"message":hex::encode(id)}))
            }
            Operation::Sync { group } => {
                self.group(&group)?.sync().await?;
                Ok(json!({}))
            }
            Operation::SyncAll => {
                self.client
                    .sync_all_welcomes_and_groups(Some(all_consent()))
                    .await?;
                Ok(json!({}))
            }
            Operation::NewInstallation { .. } => {
                bail!("new installations belong to the supervisor")
            }
            Operation::RestartStream => {
                let enabled = self.stream.lock().await.enabled;
                self.stream(enabled).await
            }
            Operation::Consent { group, state } => {
                let consent = match state {
                    0 => ConsentState::Allowed,
                    1 => ConsentState::Unknown,
                    _ => ConsentState::Denied,
                };
                self.group(&group)?.update_consent_state(consent)?;
                Ok(json!({}))
            }
            Operation::UpdateInstallations { group } => {
                self.group(&group)?.update_installations().await?;
                Ok(json!({}))
            }
        }
    }

    async fn command(&self, command: Command) -> Result<Value> {
        match command {
            Command::Operation { operation } => self.operate(operation).await,
            Command::Checkpoint => {
                self.recover_stream().await?;
                let groups = self
                    .groups()?
                    .into_iter()
                    .map(|g| g.group_id)
                    .collect::<Vec<_>>();
                let budget = xmtp_mls::diagnostics::RetryBudgets::default();
                let checkpoint = xmtp_mls::diagnostics::checkpoint(
                    &self.client.context,
                    &groups,
                    Duration::from_millis(budget.barrier_ms),
                )
                .await;
                Ok(serde_json::to_value(checkpoint)?)
            }
            Command::Snapshot => {
                let groups = self
                    .groups()?
                    .into_iter()
                    .map(|g| g.diagnostic_snapshot())
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(
                    json!({"process_id":std::process::id(),"inbox_id":self.client.inbox_id(),"installation_id":hex::encode(self.client.installation_public_key()),"groups":groups,"disk":self.disk.stats(),"contention":xmtp_mls::diagnostics::contention_snapshot(),"stream":*self.stream_diagnostics.lock().await}),
                )
            }
            Command::Counters => Ok(
                json!({"process_id":std::process::id(),"contention":xmtp_mls::diagnostics::contention_snapshot(),"disk":self.disk.stats(),"maybe_committed":self.disk.maybe_committed(),"stream":*self.stream_diagnostics.lock().await}),
            ),
            Command::Tokens { tokens } => {
                self.recover_stream().await?;
                let wanted: BTreeSet<_> = tokens.into_iter().collect();
                ensure!(wanted.len() <= STREAM_TOKEN_CAP, "token query cap exceeded");
                if wanted.is_empty() {
                    self.tokens.lock().await.clear();
                    return Ok(json!({"sync":[],"stream":[]}));
                }
                let published = self
                    .client
                    .db()
                    .raw_query(|connection| published_token_bytes(connection, &wanted))?;
                let synced = wanted
                    .iter()
                    .filter(|token| published.iter().any(|bytes| bytes == token.as_bytes()))
                    .collect::<Vec<_>>();
                let mut seen = self.tokens.lock().await;
                seen.retain(|token| wanted.contains(token));
                let streamed = seen.iter().cloned().collect::<Vec<_>>();
                Ok(json!({"sync":synced,"stream":streamed}))
            }
            Command::Publish { group } => {
                self.group(&group)?.publish_messages().await?;
                Ok(json!({}))
            }
            Command::Stream { enabled } => self.stream(enabled).await,
            Command::Disk { kind, duration_ms } => {
                let fault: DiskFault = serde_json::from_value(Value::String(kind))?;
                self.disk
                    .arm(fault, Duration::from_millis(duration_ms), 35)?;
                Ok(json!({}))
            }
            Command::Disconnect { duration_ms } => {
                self.disk
                    .disconnect_for(Duration::from_millis(duration_ms))?;
                Ok(json!({}))
            }
            Command::ClearFaults => {
                self.disk.clear()?;
                self.recover_stream().await?;
                Ok(json!({}))
            }
            Command::Drain => Ok(json!({})),
            Command::Shutdown => {
                self.stream(false).await?;
                self.client.close().await?;
                Ok(json!({}))
            }
        }
    }
}

pub(crate) async fn run(config_path: &std::path::Path) -> Result<()> {
    let config: InstanceConfig = serde_json::from_slice(&std::fs::read(config_path)?)?;
    let run_id = config_path
        .parent()
        .and_then(|path| path.file_name())
        .context("instance config has no run directory")?
        .to_string_lossy()
        .into_owned();
    let endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
        .ok()
        .or_else(|| {
            std::env::var("XMTP_OTLP_GRPC_PORT")
                .ok()
                .map(|port| format!("http://127.0.0.1:{port}"))
        });
    let logging = xmtp_logging::XmtpLogging::builder()
        .level(xmtp_logging::Level::Debug)
        .stdout_level(xmtp_logging::Level::Warn)
        .json(true)
        .with_stderr()
        .with_telemetry(endpoint.map(|endpoint| xmtp_logging::TelemetryConfig {
            endpoint: Some(endpoint),
            service_name: Some("xmtp-chaos".into()),
            sample_ratio: 1.0,
            logs: false,
            resource_attributes: vec![
                ("xmtp.chaos.run".into(), run_id),
                ("xmtp.chaos.instance".into(), config.slot.to_string()),
                ("xmtp.chaos.seed".into(), config.seed.to_string()),
            ],
        }))
        .install()?;
    let wallet: PrivateKeySigner = config.wallet_key.parse()?;
    let identifier = wallet.get_identifier()?;
    let inbox = identifier.inbox_id(0)?;
    let key: [u8; 32] = hex::decode(&config.database_key)?
        .try_into()
        .map_err(|_| anyhow::anyhow!("invalid database key"))?;
    let native = NativeDb::builder()
        .persistent(config.database.to_string_lossy().into_owned())
        .key(key)
        .single_connection()
        .build()?;
    let fault_db = FaultDb::new(native, config.seed);
    let disk = fault_db.control();
    let store = ChaosStore::new(fault_db)?;
    let api =
        BackendClient::new(xmtp_api_grpc::GrpcClient::create(config.endpoint.parse()?)?.arced());
    let client = Client::builder(IdentityStrategy::new(inbox, identifier, 0, None))
        .api_client_with_streams(Arc::new(api))
        .store(store)
        .default_mls_store()?
        .with_remote_verifier()?
        .build()
        .await?;
    if let Some(mut request) = client.context.signature_request() {
        request
            .add_signature(
                wallet.sign(&request.signature_text())?,
                client.scw_verifier(),
            )
            .await?;
        client.register_identity(request).await?;
    }
    client.ensure_registration_visible().await?;
    let state = Arc::new(State {
        client,
        disk,
        stream: Mutex::new(StreamTask::default()),
        tokens: Arc::new(Mutex::new(BTreeSet::new())),
        stream_diagnostics: Arc::new(Mutex::new(StreamDiagnostics::default())),
    });
    state.stream(config.stream_owner).await?;
    let output = Arc::new(Mutex::new(tokio::io::stdout()));
    write_response(&output, Response{id:0,value:Some(json!({"inbox_id":state.client.inbox_id(),"installation_id":hex::encode(state.client.installation_public_key())})),error:None}).await?;
    let mut input = BufReader::new(tokio::io::stdin());
    let mut tasks = tokio::task::JoinSet::new();
    loop {
        let mut frame = Vec::new();
        let n = (&mut input)
            .take((MAX_FRAME_BYTES + 1) as u64)
            .read_until(b'\n', &mut frame)
            .await?;
        if n == 0 {
            break;
        }
        anyhow::ensure!(n <= MAX_FRAME_BYTES, "oversize command");
        let request: Request = serde_json::from_slice(&frame).context("invalid command")?;
        let shutdown = matches!(request.command, Command::Shutdown);
        if shutdown {
            tasks.shutdown().await;
        }
        if matches!(request.command, Command::Drain) {
            while let Some(result) = tasks.join_next().await {
                result??;
            }
        }
        let state = state.clone();
        let output = output.clone();
        let span = tracing::info_span!(
            "chaos.command",
            command_id = request.id,
            operation = request.command.name(),
            otel.status_code = tracing::field::Empty
        );
        let work = async move {
            let result = if matches!(request.command, Command::Operation { .. }) {
                xmtp_common::time::timeout(
                    xmtp_configuration::STREAM_BARRIER_TIMEOUT,
                    state.command(request.command),
                )
                .await
                .map_err(|_| anyhow::anyhow!("operation remains incomplete at its deadline"))
                .and_then(|result| result)
            } else {
                state.command(request.command).await
            };
            let response = match result {
                Ok(value) => Response {
                    id: request.id,
                    value: Some(value),
                    error: None,
                },
                Err(error) => {
                    tracing::Span::current().record("otel.status_code", "ERROR");
                    Response {
                        id: request.id,
                        value: None,
                        error: Some(format!("{error:#}")),
                    }
                }
            };
            write_response(&output, response).await
        }
        .instrument(span);
        if shutdown {
            work.await?;
            break;
        } else {
            tasks.spawn(work);
        }
        while let Some(result) = tasks.try_join_next() {
            result??;
        }
    }
    tasks.shutdown().await;
    logging.flush();
    Ok(())
}

async fn write_response(output: &Mutex<tokio::io::Stdout>, response: Response) -> Result<()> {
    let mut bytes = serde_json::to_vec(&response)?;
    anyhow::ensure!(bytes.len() < MAX_FRAME_BYTES, "oversize response");
    bytes.push(b'\n');
    let mut out = output.lock().await;
    out.write_all(&bytes).await?;
    out.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[xmtp_common::test(unwrap_try = true)]
    async fn deferred_token_lookup_is_not_limited_to_recent_messages() {
        use xmtp_db::diesel::sql_types::{Binary, Integer};
        use xmtp_db::diesel::{
            Connection, RunQueryDsl, SqliteConnection, connection::SimpleConnection,
        };
        use xmtp_db::group_message::DeliveryStatus;

        let mut connection = SqliteConnection::establish(":memory:")?;
        connection.batch_execute(
            "CREATE TABLE group_messages (decrypted_message_bytes BLOB NOT NULL, delivery_status INTEGER NOT NULL);")?;
        for (token, status) in [
            ("xchaos:old", DeliveryStatus::Published),
            ("xchaos:pending", DeliveryStatus::Unpublished),
        ] {
            xmtp_db::diesel::sql_query("INSERT INTO group_messages VALUES (?, ?)")
                .bind::<Binary, _>(token.as_bytes())
                .bind::<Integer, _>(status as i32)
                .execute(&mut connection)?;
        }
        connection.batch_execute(
            "WITH RECURSIVE n(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM n WHERE x<600)
             INSERT INTO group_messages SELECT x'00', 1 FROM n;",
        )?;
        let wanted = BTreeSet::from(["xchaos:old".into(), "xchaos:pending".into()]);
        assert_eq!(
            published_token_bytes(&mut connection, &wanted)?,
            vec![b"xchaos:old".to_vec()]
        );
    }
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn lease_busy() -> xmtp_mls::subscriptions::SubscribeError {
        xmtp_mls::subscriptions::local_delivery::LocalDeliveryError::Storage(
            xmtp_db::StorageError::Stream(
                xmtp_db::stream_storage::StreamStorageError::AlreadyActive,
            ),
        )
        .into()
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn stream_open_retries_typed_lease_busy_until_success() {
        const BUDGET: Duration = Duration::from_secs(1);
        const POLL: Duration = Duration::from_millis(1);
        let calls = AtomicUsize::new(0);
        let value = open_after_lease(
            || async {
                if calls.fetch_add(1, Ordering::SeqCst) == 0 {
                    Err(lease_busy())
                } else {
                    Ok("opened")
                }
            },
            BUDGET,
            POLL,
        )
        .await?;
        assert_eq!(value, "opened");
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn stream_open_does_not_retry_other_retryable_errors() {
        const BUDGET: Duration = Duration::from_secs(1);
        const POLL: Duration = Duration::from_millis(1);
        let calls = AtomicUsize::new(0);
        let result = open_after_lease(
            || async {
                calls.fetch_add(1, Ordering::SeqCst);
                Err::<(), _>(xmtp_mls::subscriptions::SubscribeError::StreamStale)
            },
            BUDGET,
            POLL,
        )
        .await;
        assert!(result.is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn stream_open_stops_when_busy_lease_exceeds_budget() {
        const BUDGET: Duration = Duration::from_millis(5);
        const POLL: Duration = Duration::from_millis(1);
        let result = open_after_lease(|| async { Err::<(), _>(lease_busy()) }, BUDGET, POLL).await;
        assert!(result.is_err());
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn completed_stream_reopens_once_without_resetting_replacement() {
        let (end, ended) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            let _ = ended.await;
        });
        let mut state = StreamTask {
            enabled: true,
            task: Some(task),
        };
        end.send(())?;
        xmtp_common::wait_for_eq(
            || async { state.task.as_ref().unwrap().is_finished() },
            true,
        )
        .await?;
        let opens = AtomicUsize::new(0);
        let open = || async {
            opens.fetch_add(1, Ordering::SeqCst);
            Ok(tokio::spawn(futures::future::pending()))
        };
        assert!(state.recover(open).await?);
        assert!(!state.recover(open).await?);
        assert_eq!(opens.load(Ordering::SeqCst), 1);
        state.task.take().unwrap().abort();
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn silent_live_stream_is_not_reopened() {
        let task = tokio::spawn(futures::future::pending());
        let id = task.id();
        let mut state = StreamTask {
            enabled: true,
            task: Some(task),
        };
        assert!(
            !state
                .recover(|| async { panic!("a live task must remain unchanged") })
                .await?
        );
        assert_eq!(state.task.as_ref().unwrap().id(), id);
        state.task.take().unwrap().abort();
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn stream_disabled_for_shared_database_does_not_acquire_lease() {
        let mut state = StreamTask::default();
        assert!(
            !state
                .recover(|| async { panic!("disabled consumer must not open") })
                .await?
        );
        assert!(state.task.is_none());
    }
}
