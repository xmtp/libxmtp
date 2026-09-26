use super::*;
use crate::{server_configuration::BlockedConnection, tester};
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use xmtp_attachments::GcmDecryptor;
use xmtp_common::StreamHandle as _;
use xmtp_configuration::{AttachmentsConfiguration, ServerConfiguration};
use xmtp_db::{ConnectionExt as _, diesel::RunQueryDsl as _};
use xmtp_events::{EventFilter, EventKind};

fn bytes() -> AttachmentSource {
    AttachmentSource::Bytes {
        bytes: b"attachment content".to_vec(),
        filename: Some("note.txt".into()),
        mime_type: "text/plain".into(),
    }
}

fn offer(configuration: &mut ServerConfiguration) {
    configuration.attachments = Some(AttachmentsConfiguration {
        base_url: "http://localhost:5050/attachments".into(),
        max_upload_bytes: 10_485_760,
        retention_seconds: 0,
    });
}

async fn serve_body(body: Vec<u8>) -> (String, Arc<AtomicUsize>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind attachment test server");
    let url = format!(
        "http://{}/file",
        listener.local_addr().expect("test server address")
    );
    let requests = Arc::new(AtomicUsize::new(0));
    let seen = requests.clone();
    drop(xmtp_common::task::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            seen.fetch_add(1, Ordering::SeqCst);
            let mut request = [0u8; 1024];
            let _ = stream.read(&mut request).await;
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            if stream.write_all(header.as_bytes()).await.is_err() {
                break;
            }
            if stream.write_all(&body).await.is_err() {
                break;
            }
        }
    }));
    (url, requests)
}

async fn capture_transfers(body: Vec<u8>) -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind capture server");
    let url = format!(
        "http://{}/attachment",
        listener.local_addr().expect("capture address")
    );
    let captured = Arc::new(Mutex::new(Vec::new()));
    let seen = captured.clone();
    drop(xmtp_common::task::spawn(async move {
        for _ in 0..2 {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            let mut request = Vec::new();
            let mut chunk = [0u8; 4096];
            let header_end = loop {
                let Ok(count) = stream.read(&mut chunk).await else {
                    return;
                };
                if count == 0 {
                    return;
                }
                request.extend_from_slice(&chunk[..count]);
                if let Some(end) = request.windows(4).position(|part| part == b"\r\n\r\n") {
                    break end + 4;
                }
            };
            let head = String::from_utf8_lossy(&request[..header_end]).into_owned();
            let length = head
                .lines()
                .find_map(|line| {
                    line.split_once(':')
                        .filter(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                        .and_then(|(_, value)| value.trim().parse::<usize>().ok())
                })
                .unwrap_or(0);
            while request.len() - header_end < length {
                let Ok(count) = stream.read(&mut chunk).await else {
                    return;
                };
                if count == 0 {
                    return;
                }
                request.extend_from_slice(&chunk[..count]);
            }
            seen.lock().push(head.clone());
            let content = if head.starts_with("GET ") {
                body.as_slice()
            } else {
                &[]
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                content.len()
            );
            if stream.write_all(response.as_bytes()).await.is_err() {
                return;
            }
            if stream.write_all(content).await.is_err() {
                return;
            }
        }
    }));
    (url, captured)
}

async fn paused_put(
    status: u16,
) -> (
    String,
    tokio::sync::oneshot::Receiver<()>,
    tokio::sync::oneshot::Sender<()>,
) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind PUT capture listener");
    let url = format!(
        "http://{}/attachment",
        listener.local_addr().expect("read capture address")
    );
    let (entered, seen) = tokio::sync::oneshot::channel();
    let (release, resume) = tokio::sync::oneshot::channel();
    drop(xmtp_common::task::spawn(async move {
        let (mut stream, _) = listener
            .accept()
            .await
            .expect("accept PUT capture connection");
        let mut request = Vec::new();
        let mut chunk = [0u8; 4096];
        let header_end = loop {
            let count = stream
                .read(&mut chunk)
                .await
                .expect("read PUT request headers");
            if count == 0 {
                return;
            }
            request.extend_from_slice(&chunk[..count]);
            if let Some(end) = request.windows(4).position(|part| part == b"\r\n\r\n") {
                break end + 4;
            }
        };
        let head = String::from_utf8_lossy(&request[..header_end]);
        let length = head
            .lines()
            .find_map(|line| {
                line.split_once(':')
                    .filter(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                    .and_then(|(_, value)| value.trim().parse::<usize>().ok())
            })
            .unwrap_or(0);
        while request.len() - header_end < length {
            let count = stream
                .read(&mut chunk)
                .await
                .expect("read PUT request body");
            if count == 0 {
                return;
            }
            request.extend_from_slice(&chunk[..count]);
        }
        let _ = entered.send(());
        let _ = resume.await;
        let response =
            format!("HTTP/1.1 {status} Test\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        let _ = stream.write_all(response.as_bytes()).await;
    }));
    (url, seen, release)
}

fn signed_put_api(url: String, calls: usize) -> xmtp_api_backend::MockBackendClient {
    use xmtp_proto::backend_v1::CreateUploadResponse;
    let mut api = xmtp_api_backend::MockBackendClient::new();
    api.expect_create_upload().times(calls).returning(move |_| {
        Ok(CreateUploadResponse {
            method: "PUT".into(),
            url: url.clone(),
            headers: vec![],
            expires_in_seconds: 3600,
        })
    });
    api
}

mod download;
mod lifecycle;
mod storage;
mod upload;
