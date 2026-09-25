#![cfg(not(target_arch = "wasm32"))]

use std::{
    alloc::{GlobalAlloc, Layout, System},
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    sync::atomic::{AtomicUsize, Ordering},
};

use xmtp_attachments::{
    AttachmentOptions, DownloadSink, LocalStore, NativeStore, PutOutcome, Transfer, UploadRequest,
};

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

struct CountAlloc;

unsafe impl GlobalAlloc for CountAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            let live = LIVE.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            PEAK.fetch_max(live, Ordering::Relaxed);
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
        unsafe { System.dealloc(pointer, layout) };
    }

    unsafe fn realloc(&self, pointer: *mut u8, old: Layout, new: usize) -> *mut u8 {
        let result = unsafe { System.realloc(pointer, old, new) };
        if !result.is_null() {
            if new >= old.size() {
                let live = LIVE.fetch_add(new - old.size(), Ordering::Relaxed) + new - old.size();
                PEAK.fetch_max(live, Ordering::Relaxed);
            } else {
                LIVE.fetch_sub(old.size() - new, Ordering::Relaxed);
            }
        }
        result
    }
}

#[global_allocator]
static ALLOCATOR: CountAlloc = CountAlloc;

#[derive(Default)]
struct CountSink(usize);

#[async_trait::async_trait]
impl DownloadSink for CountSink {
    async fn write(&mut self, bytes: &[u8]) -> Result<(), xmtp_attachments::AttachmentError> {
        self.0 += bytes.len();
        Ok(())
    }
}

const SIZE: usize = 64 * 1024 * 1024;
const BLOCK: usize = 64 * 1024;

fn start_server(
    handler: impl FnOnce(TcpStream) + Send + 'static,
) -> (String, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/object", listener.local_addr().unwrap());
    let task = std::thread::spawn(move || {
        let (socket, _) = listener.accept().unwrap();
        handler(socket);
    });
    (url, task)
}

fn read_headers(reader: &mut BufReader<TcpStream>) -> String {
    let mut line = String::new();
    let mut headers = String::new();
    loop {
        line.clear();
        reader.read_line(&mut line).unwrap();
        if line == "\r\n" {
            break;
        }
        headers.push_str(&line);
    }
    headers
}

fn data_block() -> [u8; BLOCK] {
    let mut block = [0_u8; BLOCK];
    let mut state = 0x1234_5678_u32;
    for byte in &mut block {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        *byte = state as u8;
    }
    block
}

fn reset_peak() -> usize {
    let baseline = LIVE.load(Ordering::Relaxed);
    PEAK.store(baseline, Ordering::Relaxed);
    baseline
}

fn allowed() -> Result<Transfer, xmtp_attachments::AttachmentError> {
    Transfer::new(AttachmentOptions {
        allow_private_network: true,
        ..Default::default()
    })
}

// The raw socket server reuses its data buffer for each chunk.
#[xmtp_common::test(unwrap_try = true)]
async fn bounded_allocation() {
    let compressed_dir = tempfile::tempdir()?;
    let compressed_path = compressed_dir.path().join("body.gz");
    let block = data_block();
    let file = std::fs::File::create(&compressed_path)?;
    let mut encoder = flate2::write::GzEncoder::new(file, flate2::Compression::fast());
    for _ in 0..SIZE / BLOCK {
        encoder.write_all(&block)?;
    }
    encoder.finish()?;
    for gzip in [false, true] {
        let compressed_path = compressed_path.clone();
        let (url, task) = start_server(move |socket| {
            let mut reader = BufReader::new(socket);
            read_headers(&mut reader);
            let mut socket = reader.into_inner();
            let mut block = data_block();
            if gzip {
                socket
                    .write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Encoding: gzip\r\nConnection: close\r\n\r\n",
                    )
                    .unwrap();
                let mut input = std::fs::File::open(compressed_path).unwrap();
                loop {
                    let size = input.read(&mut block).unwrap();
                    if size == 0 {
                        break;
                    }
                    socket.write_all(&block[..size]).unwrap();
                }
            } else {
                socket
                    .write_all(
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {SIZE}\r\nConnection: close\r\n\r\n"
                        )
                        .as_bytes(),
                    )
                    .unwrap();
                for _ in 0..SIZE / BLOCK {
                    socket.write_all(&block).unwrap();
                }
            }
        });
        let mut sink = CountSink::default();
        let transfer = allowed()?;
        let baseline = reset_peak();
        transfer.get(&url, SIZE as u64, &mut sink).await?;
        assert_eq!(sink.0, SIZE);
        let attributable = PEAK.load(Ordering::Relaxed).saturating_sub(baseline);
        task.join().unwrap();
        assert!(
            attributable <= 1_048_576,
            "gzip={gzip}, peak allocation: {attributable}"
        );
    }

    let directory = tempfile::tempdir()?;
    let store = NativeStore::new(directory.path()).await?;
    let block = data_block();
    let mut writer = store.create_temp(".tmp/body").await?;
    for _ in 0..SIZE / BLOCK {
        writer.write(&block).await?;
    }
    store.sync(&mut writer).await?;
    drop(writer);
    store.rename(".tmp/body", "body").await?;
    let staged = store.open_read("body").await?;
    let (url, task) = start_server(|socket| {
        let mut reader = BufReader::with_capacity(BLOCK, socket);
        let headers = read_headers(&mut reader);
        let mut buffer = [0_u8; BLOCK];
        let mut count = 0;
        if headers
            .to_ascii_lowercase()
            .contains("transfer-encoding: chunked")
        {
            let mut line = String::new();
            loop {
                line.clear();
                reader.read_line(&mut line).unwrap();
                let chunk =
                    usize::from_str_radix(line.trim().split(';').next().unwrap(), 16).unwrap();
                if chunk == 0 {
                    break;
                }
                let mut remaining = chunk;
                while remaining > 0 {
                    let size = remaining.min(BLOCK);
                    reader.read_exact(&mut buffer[..size]).unwrap();
                    remaining -= size;
                    count += size;
                }
                reader.read_exact(&mut buffer[..2]).unwrap();
            }
        } else {
            let length = headers
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length: ")
                        .and_then(|n| n.trim().parse::<usize>().ok())
                })
                .unwrap();
            let mut remaining = length;
            while remaining > 0 {
                let size = remaining.min(BLOCK);
                reader.read_exact(&mut buffer[..size]).unwrap();
                remaining -= size;
                count += size;
            }
        }
        assert_eq!(count, SIZE);
        reader
            .get_mut()
            .write_all(b"HTTP/1.1 202 Accepted\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .unwrap();
    });
    let request = UploadRequest {
        method: "PUT".into(),
        url,
        headers: vec![],
        expires_in_seconds: 60,
    };
    let transfer = allowed()?;
    let baseline = reset_peak();
    assert_eq!(transfer.put(&request, staged).await?, PutOutcome::Stored);
    let attributable = PEAK.load(Ordering::Relaxed).saturating_sub(baseline);
    task.join().unwrap();
    assert!(
        attributable <= 1_048_576,
        "PUT peak allocation: {attributable}"
    );
}
