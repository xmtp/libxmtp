use super::*;
use crate::{ArchiveImporter, ENC_KEY_SIZE};
use futures::{
    StreamExt,
    io::{BufReader, Cursor},
    task::noop_waker,
};
use futures_util::AsyncReadExt;
use std::sync::atomic::{AtomicUsize, Ordering};
use xmtp_proto::xmtp::device_sync::consent_backup::ConsentSave;

const KEY: [u8; ENC_KEY_SIZE] = [7; ENC_KEY_SIZE];
const NONCE: [u8; NONCE_SIZE] = [255; NONCE_SIZE];
const LARGE_RECORD_SIZE: usize = 150_000;
const MAX_POLLS: usize = 1_000_000;

#[derive(Clone, Copy, Default)]
enum Failure {
    #[default]
    None,
    WriteZero,
    Write,
    Flush,
    Close,
}

struct TestEncoder {
    inner: ZstdEncoder<Vec<u8>>,
    accepted: Vec<u8>,
    limit: usize,
    pending_write: bool,
    write_pendings: usize,
    flush_polls: usize,
    close_polls: usize,
    flushing: bool,
    closing: bool,
    failure: Failure,
}

impl TestEncoder {
    fn new(limit: usize, failure: Failure) -> Self {
        Self {
            inner: ZstdEncoder::new(Vec::new()),
            accepted: Vec::new(),
            limit,
            pending_write: true,
            write_pendings: 0,
            flush_polls: 0,
            close_polls: 0,
            flushing: false,
            closing: false,
            failure,
        }
    }
}

impl BufferedEncoder for TestEncoder {
    fn output(&mut self) -> &mut Vec<u8> {
        self.inner.get_mut()
    }
}

impl AsyncWrite for TestEncoder {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        assert!(
            !this.flushing && !this.closing,
            "write before flush/close completed"
        );
        if this.pending_write {
            this.pending_write = false;
            this.write_pendings += 1;
            cx.waker().wake_by_ref();
            return Poll::Pending;
        }
        match this.failure {
            Failure::WriteZero => return Poll::Ready(Ok(0)),
            Failure::Write => return Poll::Ready(Err(io::Error::other("write failed"))),
            _ => {}
        }
        let amount =
            ready!(Pin::new(&mut this.inner).poll_write(cx, &buf[..buf.len().min(this.limit)]))?;
        this.accepted.extend_from_slice(&buf[..amount]);
        this.pending_write = true;
        Poll::Ready(Ok(amount))
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        this.flush_polls += 1;
        this.flushing = true;
        if !this.flush_polls.is_multiple_of(3) {
            cx.waker().wake_by_ref();
            return Poll::Pending;
        }
        if matches!(this.failure, Failure::Flush) {
            this.flushing = false;
            return Poll::Ready(Err(io::Error::other("flush failed")));
        }
        ready!(Pin::new(&mut this.inner).poll_flush(cx))?;
        this.flushing = false;
        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        assert!(!this.flushing, "close before flush completed");
        this.close_polls += 1;
        this.closing = true;
        if this.close_polls < 3 {
            cx.waker().wake_by_ref();
            return Poll::Pending;
        }
        if matches!(this.failure, Failure::Close) {
            return Poll::Ready(Err(io::Error::other("close failed")));
        }
        Pin::new(&mut this.inner).poll_close(cx)
    }
}

fn fixture(
    limit: usize,
    failure: Failure,
) -> (
    ArchiveReader<TestEncoder>,
    BackupMetadataSave,
    Vec<BackupElement>,
    Arc<AtomicUsize>,
) {
    let metadata = BackupMetadataSave {
        elements: vec![2],
        exported_at_ns: 13,
        start_ns: None,
        end_ns: None,
    };
    let records = (0..3)
        .map(|index| BackupElement {
            element: Some(Element::Consent(ConsentSave {
                entity: format!("{index}:{}", "record".repeat(LARGE_RECORD_SIZE / 6)),
                ..Default::default()
            })),
        })
        .collect::<Vec<_>>();
    let polls = Arc::new(AtomicUsize::new(0));
    let mut pending = true;
    let mut input = Some(records.clone());
    let source_polls = polls.clone();
    let stream = futures::stream::poll_fn(move |cx| {
        source_polls.fetch_add(1, Ordering::SeqCst);
        if pending {
            pending = false;
            cx.waker().wake_by_ref();
            Poll::Pending
        } else {
            Poll::Ready(input.take().map(Ok))
        }
    });
    let mut header = BACKUP_VERSION.to_le_bytes().to_vec();
    header.extend_from_slice(&NONCE);
    let reader = ArchiveReader {
        stage: Stage::Nonce,
        stream: BatchExportStream {
            buffer: Vec::new(),
            input_streams: vec![Box::pin(stream)],
        },
        position: 0,
        encoder: TestEncoder::new(limit, failure),
        frame: Vec::new(),
        frame_position: 0,
        #[allow(deprecated)]
        cipher: Aes256Gcm::new(GenericArray::from_slice(&KEY)),
        #[allow(deprecated)]
        nonce: GenericArray::from(NONCE),
        nonce_buffer: header,
    };
    (reader, metadata, records, polls)
}

fn collect(
    reader: &mut ArchiveReader<TestEncoder>,
    metadata: &BackupMetadataSave,
) -> io::Result<Vec<u8>> {
    let waker = noop_waker();
    let mut cx = Context::from_waker(&waker);
    let mut bytes = Vec::new();
    let mut buffer = [0; 257];
    for _ in 0..MAX_POLLS {
        match Pin::new(&mut *reader).poll_read(&mut cx, &mut buffer, metadata) {
            Poll::Ready(Ok(0)) => return Ok(bytes),
            Poll::Ready(Ok(amount)) => bytes.extend_from_slice(&buffer[..amount]),
            Poll::Ready(Err(error)) => return Err(error),
            Poll::Pending => {}
        }
    }
    panic!("encoder made no progress");
}

async fn assert_archive(
    bytes: Vec<u8>,
    accepted: &[u8],
    metadata: &BackupMetadataSave,
    records: Vec<BackupElement>,
) {
    let mut expected = Vec::new();
    #[allow(deprecated)]
    let cipher = Aes256Gcm::new(GenericArray::from_slice(&KEY));
    #[allow(deprecated)]
    let mut nonce = GenericArray::from(NONCE);
    for record in std::iter::once(BackupElement {
        element: Some(Element::Metadata(metadata.clone())),
    })
    .chain(records.iter().cloned())
    {
        let encrypted = cipher
            .encrypt(&nonce, record.encode_to_vec().as_slice())
            .unwrap();
        expected.extend_from_slice(&(encrypted.len() as u32).to_le_bytes());
        expected.extend_from_slice(&encrypted);
        nonce.increment();
    }
    assert_eq!(accepted.len(), expected.len());
    assert!(accepted == expected, "ciphertext bytes differ");
    assert_eq!(&bytes[..2], &BACKUP_VERSION.to_le_bytes());
    assert_eq!(&bytes[2..2 + NONCE_SIZE], &NONCE);
    let mut decoded = Vec::new();
    let mut decoder = async_compression::futures::bufread::ZstdDecoder::new(Cursor::new(
        &bytes[2 + NONCE_SIZE..],
    ));
    decoder.read_to_end(&mut decoded).await.unwrap();
    assert!(decoded == expected, "decompressed frames differ");
    let mut importer = ArchiveImporter::load(Box::pin(BufReader::new(Cursor::new(bytes))), &KEY)
        .await
        .unwrap();
    assert_eq!(importer.metadata().exported_at_ns, metadata.exported_at_ns);
    let mut imported = Vec::new();
    while let Some(record) = importer.next().await {
        imported.push(record.unwrap());
    }
    assert_eq!(imported, records);
}

#[xmtp_common::test(unwrap_try = true)]
async fn short_and_pending_writes_preserve_exact_frames() {
    // verifies: ARCH-001, ARCH-017
    for limit in [3, 7] {
        let (mut reader, metadata, records, polls) = fixture(limit, Failure::None);
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);
        let mut buffer = [0; 1024];
        let header = match Pin::new(&mut reader).poll_read(&mut cx, &mut buffer, &metadata) {
            Poll::Ready(Ok(amount)) => buffer[..amount].to_vec(),
            other => panic!("header read failed: {other:?}"),
        };
        assert!(
            Pin::new(&mut reader)
                .poll_read(&mut cx, &mut buffer, &metadata)
                .is_pending()
        );
        assert_eq!(reader.nonce.as_slice(), NONCE);
        assert_eq!(polls.load(Ordering::SeqCst), 0);
        let mut bytes = header;
        bytes.extend(collect(&mut reader, &metadata)?);
        assert!(reader.encoder.write_pendings > 1);
        assert_archive(bytes, &reader.encoder.accepted, &metadata, records).await;
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn pending_flush_and_close_finish_before_eof() {
    // verifies: ARCH-001, ARCH-017
    let (mut reader, metadata, records, polls) = fixture(usize::MAX, Failure::None);
    let bytes = collect(&mut reader, &metadata)?;
    assert!(reader.encoder.flush_polls >= 3);
    assert_eq!(reader.encoder.close_polls, 3);
    assert_archive(bytes, &reader.encoder.accepted, &metadata, records).await;
    let source_polls = polls.load(Ordering::SeqCst);
    assert!(collect(&mut reader, &metadata)?.is_empty());
    assert_eq!(polls.load(Ordering::SeqCst), source_polls);
    assert_eq!(reader.encoder.close_polls, 3);
}

#[xmtp_common::test(unwrap_try = true)]
async fn empty_output_buffer_does_not_advance_export() {
    let (mut reader, metadata, records, polls) = fixture(usize::MAX, Failure::None);
    let waker = noop_waker();
    let mut cx = Context::from_waker(&waker);
    let mut bytes = Vec::new();
    let mut buffer = [0; 257];
    for _ in 0..MAX_POLLS {
        let before = (
            reader.encoder.accepted.len(),
            reader.encoder.close_polls,
            reader.encoder.flush_polls,
            polls.load(Ordering::SeqCst),
        );
        assert!(matches!(
            Pin::new(&mut reader).poll_read(&mut cx, &mut [], &metadata),
            Poll::Ready(Ok(0))
        ));
        assert_eq!(
            before,
            (
                reader.encoder.accepted.len(),
                reader.encoder.close_polls,
                reader.encoder.flush_polls,
                polls.load(Ordering::SeqCst)
            )
        );
        match Pin::new(&mut reader).poll_read(&mut cx, &mut buffer, &metadata) {
            Poll::Ready(Ok(0)) => {
                assert_archive(bytes, &reader.encoder.accepted, &metadata, records).await;
                return;
            }
            Poll::Ready(Ok(amount)) => bytes.extend_from_slice(&buffer[..amount]),
            Poll::Ready(Err(error)) => panic!("read failed: {error}"),
            Poll::Pending => {}
        }
    }
    panic!("encoder made no progress");
}

#[xmtp_common::test(unwrap_try = true)]
async fn encoder_failures_are_returned() {
    for (failure, expected) in [
        (Failure::Write, "write failed"),
        (Failure::Flush, "flush failed"),
        (Failure::Close, "close failed"),
    ] {
        let (mut reader, metadata, _, _) = fixture(usize::MAX, failure);
        assert_eq!(
            collect(&mut reader, &metadata)
                .map(|_| ())
                .expect_err("export must fail")
                .to_string(),
            expected
        );
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn encoder_write_zero_is_an_error() {
    let (mut reader, metadata, _, _) = fixture(usize::MAX, Failure::WriteZero);
    assert_eq!(
        collect(&mut reader, &metadata).unwrap_err().kind(),
        io::ErrorKind::WriteZero
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn storage_failure_reaches_the_archive_reader() {
    let (mut reader, metadata, _, _) = fixture(usize::MAX, Failure::None);
    reader.stream.input_streams = vec![Box::pin(futures::stream::iter([Err(
        xmtp_db::StorageError::DbDeserialize,
    )]))];
    let error = collect(&mut reader, &metadata).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::Other);
    assert_eq!(
        error.to_string(),
        xmtp_db::StorageError::DbDeserialize.to_string()
    );
    #[cfg(not(target_arch = "wasm32"))]
    assert!(matches!(
        error
            .get_ref()
            .and_then(|source| source.downcast_ref::<xmtp_db::StorageError>()),
        Some(xmtp_db::StorageError::DbDeserialize)
    ));
}
