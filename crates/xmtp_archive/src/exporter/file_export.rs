use super::ArchiveExporter;
use crate::ArchiveError;
use futures_util::AsyncReadExt;
use std::path::Path;
use tokio::{
    fs::File,
    io::{AsyncWrite, AsyncWriteExt},
};

impl ArchiveExporter {
    pub async fn write_to_file(&mut self, path: impl AsRef<Path>) -> Result<(), ArchiveError> {
        let mut file = File::create(path.as_ref()).await?;
        self.write_to(&mut file).await
    }

    async fn write_to<W: AsyncWrite + Unpin>(
        &mut self,
        writer: &mut W,
    ) -> Result<(), ArchiveError> {
        let mut buffer = [0u8; 1024];

        let mut amount = self.read(&mut buffer).await?;
        while amount != 0 {
            writer.write_all(&buffer[..amount]).await?;
            amount = self.read(&mut buffer).await?;
        }

        writer.flush().await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ENC_KEY_SIZE,
        archive_options::{ArchiveOptions, BackupElementSelection},
        importer::ArchiveImporter,
    };
    use futures::{
        StreamExt,
        io::{BufReader, Cursor},
    };
    use std::{
        io,
        pin::Pin,
        task::{Context, Poll},
    };
    use tokio::io::AsyncWrite;
    use xmtp_db::{
        consent_record::{ConsentState, ConsentType, StoredConsentRecord},
        prelude::QueryConsentRecord,
    };
    use xmtp_proto::xmtp::device_sync::{BackupElement, backup_element::Element};

    const KEY: [u8; ENC_KEY_SIZE] = [7; ENC_KEY_SIZE];

    #[derive(Clone, Copy)]
    enum WriteBehavior {
        Short(usize),
        Zero,
        Error,
        FlushError,
    }

    struct TestWriter {
        bytes: Vec<u8>,
        behavior: WriteBehavior,
        pending_next: bool,
        pendings: usize,
        writes: usize,
        flushes: usize,
    }

    impl TestWriter {
        fn new(behavior: WriteBehavior) -> Self {
            Self {
                bytes: Vec::new(),
                behavior,
                pending_next: false,
                pendings: 0,
                writes: 0,
                flushes: 0,
            }
        }
    }

    impl AsyncWrite for TestWriter {
        fn poll_write(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            if self.pending_next {
                self.pending_next = false;
                self.pendings += 1;
                cx.waker().wake_by_ref();
                return Poll::Pending;
            }
            self.writes += 1;
            match self.behavior {
                WriteBehavior::Short(limit) => {
                    let amount = limit.min(buf.len());
                    self.bytes.extend_from_slice(&buf[..amount]);
                    self.pending_next = true;
                    Poll::Ready(Ok(amount))
                }
                WriteBehavior::Zero => Poll::Ready(Ok(0)),
                WriteBehavior::Error => Poll::Ready(Err(io::Error::other("write failed"))),
                WriteBehavior::FlushError => {
                    self.bytes.extend_from_slice(buf);
                    Poll::Ready(Ok(buf.len()))
                }
            }
        }

        fn poll_flush(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            self.flushes += 1;
            if matches!(self.behavior, WriteBehavior::FlushError) {
                Poll::Ready(Err(io::Error::other("flush failed")))
            } else {
                Poll::Ready(Ok(()))
            }
        }

        fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            self.poll_flush(cx)
        }
    }

    fn exporter_with_records() -> (ArchiveExporter, Vec<BackupElement>) {
        let db = xmtp_db::NativeDb::builder()
            .ephemeral()
            .build_unencrypted()
            .expect("ephemeral database");
        let store = xmtp_db::EncryptedMessageStore::new(db).expect("archive test database");
        let connection = store.db();
        let records = [
            StoredConsentRecord {
                entity_type: ConsentType::InboxId,
                state: ConsentState::Allowed,
                entity: "inbox-one".into(),
                consented_at_ns: 11,
            },
            StoredConsentRecord {
                entity_type: ConsentType::InboxId,
                state: ConsentState::Denied,
                entity: "inbox-two".into(),
                consented_at_ns: 23,
            },
        ];
        for record in &records {
            connection
                .insert_newer_consent_record(record.clone())
                .expect("store consent");
        }
        let exporter = ArchiveExporter::new(
            ArchiveOptions {
                elements: vec![BackupElementSelection::Consent],
                ..Default::default()
            },
            connection,
            &KEY,
        );
        let expected = records
            .into_iter()
            .map(|record| BackupElement {
                element: Some(Element::Consent(record.into())),
            })
            .collect();
        (exporter, expected)
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn short_writes_with_pending_keep_every_record() {
        // verifies: ARCH-001, ARCH-017
        for limit in [3, 7] {
            let (mut exporter, records) = exporter_with_records();
            let expected_metadata = exporter.metadata.clone();
            let mut writer = TestWriter::new(WriteBehavior::Short(limit));
            exporter.write_to(&mut writer).await?;
            assert!(writer.writes > 1);
            assert!(writer.pendings > 0);
            assert_eq!(writer.flushes, 1);

            let reader = Box::pin(BufReader::new(Cursor::new(writer.bytes)));
            let mut importer = ArchiveImporter::load(reader, &KEY).await?;
            assert_eq!(importer.metadata().backup_version, 0);
            assert_eq!(
                importer.metadata().exported_at_ns,
                expected_metadata.exported_at_ns
            );
            let mut imported = Vec::new();
            while let Some(record) = importer.next().await {
                imported.push(record?);
            }
            assert_eq!(imported, records);
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn write_zero_is_an_error() {
        let (mut exporter, _) = exporter_with_records();
        let mut writer = TestWriter::new(WriteBehavior::Zero);
        let error = exporter.write_to(&mut writer).await.unwrap_err();
        assert!(
            matches!(error, ArchiveError::IO(ref error) if error.kind() == io::ErrorKind::WriteZero)
        );
        assert_eq!(writer.flushes, 0);
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn write_error_is_returned() {
        let (mut exporter, _) = exporter_with_records();
        let mut writer = TestWriter::new(WriteBehavior::Error);
        let error = exporter.write_to(&mut writer).await.unwrap_err();
        assert!(
            matches!(error, ArchiveError::IO(ref error) if error.to_string() == "write failed")
        );
        assert_eq!(writer.flushes, 0);
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn flush_error_is_returned() {
        let (mut exporter, _) = exporter_with_records();
        let mut writer = TestWriter::new(WriteBehavior::FlushError);
        let error = exporter.write_to(&mut writer).await.unwrap_err();
        assert!(
            matches!(error, ArchiveError::IO(ref error) if error.to_string() == "flush failed")
        );
        assert_eq!(writer.flushes, 1);
        assert!(!writer.bytes.is_empty());
    }
}
