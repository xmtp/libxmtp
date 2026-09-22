//! Bounded child process transport. A successful stop always confirms process exit.

use crate::protocol::{
    Command, InstanceConfig, MAX_FRAME_BYTES, RPC_TIMEOUT_SECS, Request, Response,
};
use anyhow::{Context, Result, anyhow, ensure};
use serde_json::Value;
use std::{
    collections::HashMap,
    fs::{File, OpenOptions},
    io::Write,
    path::Path,
    process::Stdio,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};
use tokio::{
    io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin},
    sync::{Mutex as AsyncMutex, oneshot},
};
use xmtp_common::{
    StreamHandle,
    time::{Duration, timeout},
};

const LOG_FRAME_BYTES: usize = 16 * 1024;
const ROUND_LOG_BYTES: u64 = 8 * 1024 * 1024;
const STOP_TIMEOUT: Duration = Duration::from_secs(10);
const LOG_DRAIN_TIMEOUT: Duration = Duration::from_secs(5);

pub(crate) type ChildLogs = Arc<Mutex<File>>;
type Reply = std::result::Result<Value, String>;

#[derive(Default)]
struct Pending {
    replies: HashMap<u64, oneshot::Sender<Reply>>,
    closed: Option<String>,
}

impl Pending {
    fn close(&mut self, reason: String) {
        if self.closed.is_none() {
            self.closed = Some(reason.clone());
        }
        for (_, reply) in self.replies.drain() {
            let _ = reply.send(Err(reason.clone()));
        }
    }
}

// A cancelled caller must not retain a pending reply or leave a partial frame usable.
struct RequestGuard {
    id: u64,
    pending: Arc<Mutex<Pending>>,
    writing: bool,
}

impl Drop for RequestGuard {
    fn drop(&mut self) {
        let mut pending = self.pending.lock().unwrap_or_else(|e| e.into_inner());
        pending.replies.remove(&self.id);
        if self.writing {
            pending.close("child request write was interrupted".into());
        }
    }
}

struct ReaderTask {
    handle: Box<dyn StreamHandle<StreamOutput = ()>>,
    done: AsyncMutex<Option<oneshot::Receiver<()>>>,
}

impl ReaderTask {
    fn spawn(future: impl Future<Output = ()> + Send + 'static) -> Self {
        let (done, receiver) = oneshot::channel();
        let handle = xmtp_common::spawn(None, async move {
            future.await;
            let _ = done.send(());
        });
        Self {
            handle: Box::new(handle),
            done: AsyncMutex::new(Some(receiver)),
        }
    }

    async fn finish(&self) {
        if let Some(done) = self.done.lock().await.take()
            && timeout(LOG_DRAIN_TIMEOUT, done).await.is_err()
        {
            self.handle.end();
        }
    }
}

impl Drop for ReaderTask {
    fn drop(&mut self) {
        self.handle.end();
    }
}

pub(crate) struct InstanceProcess {
    pub slot: usize,
    pub inbox_id: String,
    pub installation_id: String,
    child: AsyncMutex<Child>,
    input: AsyncMutex<ChildStdin>,
    pending: Arc<Mutex<Pending>>,
    next_id: AtomicU64,
    lifecycle: AsyncMutex<()>,
    reader: ReaderTask,
    stderr: ReaderTask,
}

impl InstanceProcess {
    pub async fn spawn(
        config: &InstanceConfig,
        config_path: &Path,
        logs: ChildLogs,
    ) -> Result<Self> {
        let mut options = OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(config_path)
            .context("open child configuration")?;
        serde_json::to_writer(&mut file, config).context("write child configuration")?;
        file.flush()?;
        let mut child = tokio::process::Command::new(std::env::current_exe()?)
            .arg("instance")
            .arg(config_path)
            .env("XMTP_NO_PANIC_ON_DB_LOCK", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .context("start instance process")?;
        let input = child.stdin.take().context("child stdin unavailable")?;
        let output = child.stdout.take().context("child stdout unavailable")?;
        let errors = child.stderr.take().context("child stderr unavailable")?;
        let pending = Arc::new(Mutex::new(Pending::default()));
        let (ready, ready_rx) = oneshot::channel();
        pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .replies
            .insert(0, ready);
        let read_pending = pending.clone();
        let reader = ReaderTask::spawn(async move {
            let result = read_responses(BufReader::new(output), &read_pending).await;
            let reason = match result {
                Ok(()) => "instance response stream closed".into(),
                Err(error) => format!("instance response stream failed: {error:#}"),
            };
            read_pending
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .close(reason);
        });
        let log_pending = pending.clone();
        let slot = config.slot;
        let stderr = ReaderTask::spawn(async move {
            if let Err(error) = read_logs(BufReader::new(errors), slot, logs).await {
                log_pending
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .close(format!("instance log capture failed: {error:#}"));
            }
        });
        let mut process = Self {
            slot,
            inbox_id: String::new(),
            installation_id: String::new(),
            child: AsyncMutex::new(child),
            input: AsyncMutex::new(input),
            pending,
            next_id: AtomicU64::new(1),
            lifecycle: AsyncMutex::new(()),
            reader,
            stderr,
        };
        let ready = async {
            let value = timeout(Duration::from_secs(RPC_TIMEOUT_SECS), ready_rx)
                .await
                .context("instance startup timed out")?
                .context("instance startup channel closed")?
                .map_err(anyhow::Error::msg)?;
            let inbox = value
                .get("inbox_id")
                .and_then(Value::as_str)
                .context("instance ready response has no inbox id")?
                .to_owned();
            let installation = value
                .get("installation_id")
                .and_then(Value::as_str)
                .context("instance ready response has no installation id")?
                .to_owned();
            Ok::<_, anyhow::Error>((inbox, installation))
        }
        .await;
        match ready {
            Ok((inbox, installation)) => {
                process.inbox_id = inbox;
                process.installation_id = installation;
                Ok(process)
            }
            Err(error) => {
                process
                    .crash()
                    .await
                    .context("stop failed instance startup")?;
                Err(error)
            }
        }
    }

    pub async fn call(&self, command: Command) -> Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        ensure!(id != 0, "instance request id exhausted");
        let mut bytes = serde_json::to_vec(&Request { id, command })?;
        ensure!(bytes.len() < MAX_FRAME_BYTES, "oversize instance request");
        bytes.push(b'\n');
        let (reply, receiver) = oneshot::channel();
        {
            let mut pending = self.pending.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(reason) = &pending.closed {
                return Err(anyhow!(reason.clone()));
            }
            pending.replies.insert(id, reply);
        }
        let mut guard = RequestGuard {
            id,
            pending: self.pending.clone(),
            writing: false,
        };
        timeout(Duration::from_secs(RPC_TIMEOUT_SECS), async {
            {
                let mut input = self.input.lock().await;
                {
                    let pending = self.pending.lock().unwrap_or_else(|e| e.into_inner());
                    if let Some(reason) = &pending.closed {
                        return Err(anyhow!(reason.clone()));
                    }
                }
                guard.writing = true;
                input
                    .write_all(&bytes)
                    .await
                    .context("write instance request")?;
                input.flush().await.context("flush instance request")?;
                guard.writing = false;
            }
            receiver
                .await
                .context("instance response channel closed")?
                .map_err(anyhow::Error::msg)
        })
        .await
        .context("instance request timed out")?
    }

    pub async fn crash(&self) -> Result<()> {
        let _lifecycle = self.lifecycle.lock().await;
        self.kill_and_wait().await?;
        self.finish_readers().await;
        Ok(())
    }

    /// True means graceful exit. False means exit required a process kill.
    pub async fn stop(&self) -> Result<bool> {
        let _lifecycle = self.lifecycle.lock().await;
        let exited = self.child.lock().await.try_wait()?;
        if let Some(status) = exited {
            self.finish_readers().await;
            return Ok(status.success());
        }
        let graceful = matches!(
            timeout(STOP_TIMEOUT, self.call(Command::Shutdown)).await,
            Ok(Ok(_))
        );
        if graceful {
            let exited = {
                let mut child = self.child.lock().await;
                timeout(STOP_TIMEOUT, child.wait()).await
            };
            if matches!(exited, Ok(Ok(_))) {
                self.finish_readers().await;
                return Ok(true);
            }
        }
        self.kill_and_wait().await?;
        self.finish_readers().await;
        Ok(false)
    }

    async fn kill_and_wait(&self) -> Result<()> {
        let mut child = self.child.lock().await;
        if child.try_wait()?.is_none()
            && let Err(error) = timeout(STOP_TIMEOUT, child.kill())
                .await
                .context("instance kill timed out")?
            && child.try_wait()?.is_none()
        {
            return Err(error).context("kill instance process");
        }
        timeout(STOP_TIMEOUT, child.wait())
            .await
            .context("instance exit timed out")??;
        self.pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .close("instance process stopped".into());
        Ok(())
    }

    async fn finish_readers(&self) {
        self.reader.finish().await;
        self.stderr.finish().await;
    }
}

async fn read_frame(
    reader: &mut (impl AsyncBufRead + Unpin),
    cap: usize,
) -> Result<Option<Vec<u8>>> {
    let mut frame = Vec::new();
    let count = reader
        .take((cap + 1) as u64)
        .read_until(b'\n', &mut frame)
        .await?;
    if count == 0 {
        return Ok(None);
    }
    ensure!(count <= cap, "oversize instance response");
    ensure!(frame.last() == Some(&b'\n'), "incomplete instance response");
    Ok(Some(frame))
}

async fn read_responses(
    mut reader: impl AsyncBufRead + Unpin,
    pending: &Arc<Mutex<Pending>>,
) -> Result<()> {
    while let Some(frame) = read_frame(&mut reader, MAX_FRAME_BYTES).await? {
        let response: Response =
            serde_json::from_slice(&frame).context("invalid instance response")?;
        let result = match (response.value, response.error) {
            (Some(value), None) => Ok(value),
            (None, Some(error)) => Err(error),
            _ => return Err(anyhow!("instance response must contain one result")),
        };
        if let Some(reply) = pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .replies
            .remove(&response.id)
        {
            // A timeout may remove the receiver before the child finishes.
            let _ = reply.send(result);
        }
    }
    Ok(())
}

async fn read_logs(
    mut reader: impl AsyncBufRead + Unpin,
    slot: usize,
    logs: ChildLogs,
) -> Result<()> {
    let mut continuation = false;
    loop {
        let mut frame = Vec::new();
        let count = (&mut reader)
            .take(LOG_FRAME_BYTES as u64)
            .read_until(b'\n', &mut frame)
            .await?;
        if count == 0 {
            return Ok(());
        }
        let ends_line = frame.last() == Some(&b'\n');
        if !continuation {
            let mut record = serde_json::to_vec(&serde_json::json!({
                "slot": slot, "stderr": String::from_utf8_lossy(&frame), "truncated": !ends_line,
            }))?;
            record.push(b'\n');
            let mut file = logs.lock().unwrap_or_else(|e| e.into_inner());
            if file.metadata()?.len().saturating_add(record.len() as u64) <= ROUND_LOG_BYTES {
                file.write_all(&record)?;
            }
        }
        continuation = !ends_line;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[xmtp_common::test(unwrap_try = true)]
    async fn responses_are_routed_by_id() {
        let pending = Arc::new(Mutex::new(Pending::default()));
        let (first, first_rx) = oneshot::channel();
        let (second, second_rx) = oneshot::channel();
        {
            let mut state = pending.lock().unwrap();
            state.replies.insert(1, first);
            state.replies.insert(2, second);
        }
        let frames = b"{\"id\":2,\"value\":\"second\",\"error\":null}\n{\"id\":1,\"value\":null,\"error\":\"first failed\"}\n";
        read_responses(&frames[..], &pending).await?;
        assert_eq!(second_rx.await?, Ok(Value::String("second".into())));
        assert_eq!(first_rx.await?, Err("first failed".into()));
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn framing_rejects_oversize_and_partial_responses() {
        let mut oversized = &b"12345\n"[..];
        assert!(read_frame(&mut oversized, 4).await.is_err());
        let mut partial = &b"123"[..];
        assert!(read_frame(&mut partial, 4).await.is_err());
        let mut valid = &b"123\n"[..];
        assert_eq!(read_frame(&mut valid, 4).await?, Some(b"123\n".to_vec()));
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn interrupted_request_write_fails_other_pending_calls() {
        let pending = Arc::new(Mutex::new(Pending::default()));
        let (reply, response) = oneshot::channel();
        pending.lock().unwrap().replies.insert(2, reply);
        let guard = RequestGuard {
            id: 1,
            pending: pending.clone(),
            writing: true,
        };
        drop(guard);
        assert!(response.await?.is_err());
        let pending = pending.lock().unwrap();
        assert!(pending.closed.is_some());
        assert!(pending.replies.is_empty());
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn log_capture_caps_long_lines_and_total_bytes() {
        let file = tempfile::tempfile()?;
        let logs = Arc::new(Mutex::new(file));
        let mut input = vec![b'x'; LOG_FRAME_BYTES * 2];
        input.extend_from_slice(b"\nlast line\n");
        read_logs(&input[..], 7, logs.clone()).await?;
        {
            let mut file = logs.lock().unwrap();
            use std::io::{Read, Seek, SeekFrom};
            file.seek(SeekFrom::Start(0))?;
            let mut text = String::new();
            file.read_to_string(&mut text)?;
            assert_eq!(text.lines().count(), 2);
            let first: Value = serde_json::from_str(text.lines().next().unwrap())?;
            assert_eq!(first["truncated"], true);
            file.set_len(ROUND_LOG_BYTES)?;
        }
        read_logs(&b"discarded\n"[..], 7, logs.clone()).await?;
        assert_eq!(logs.lock().unwrap().metadata()?.len(), ROUND_LOG_BYTES);
    }
}
