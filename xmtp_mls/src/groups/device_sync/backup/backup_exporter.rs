use super::{backup_stream::BackupStream, BackupOptions};
use crate::{groups::device_sync::DeviceSyncError, XmtpOpenMlsProvider};
use prost::Message;
use std::{
    io::{Read, Write},
    path::Path,
    sync::Arc,
};
use xmtp_proto::xmtp::device_sync::BackupMetadata;
use zstd::stream::Encoder;

pub(super) struct BackupExporter<'a> {
    stage: Stage,
    metadata: BackupMetadata,
    stream: BackupStream,
    position: usize,
    encoder: Encoder<'a, Vec<u8>>,
}

#[derive(Default)]
pub(super) enum Stage {
    #[default]
    Metadata,
    Elements,
}

impl<'a> BackupExporter<'a> {
    pub(super) fn new(opts: BackupOptions, provider: &Arc<XmtpOpenMlsProvider>) -> Self {
        Self {
            position: 0,
            stage: Stage::default(),
            stream: BackupStream::new(&opts, provider),
            metadata: opts.into(),
            encoder: Encoder::new(Vec::new(), 0).unwrap(),
        }
    }

    pub fn write_to_file(&mut self, path: impl AsRef<Path>) -> Result<(), DeviceSyncError> {
        let mut file = std::fs::File::create(path.as_ref())?;
        let mut buffer = [0u8; 1024];

        let mut amount = self.read(&mut buffer)?;
        while amount != 0 {
            file.write_all(&buffer[..amount])?;
            amount = self.read(&mut buffer)?;
        }

        file.flush()?;

        Ok(())
    }
}

impl<'a> Read for BackupExporter<'a> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        {
            // Read from the buffer while there is data
            let buffer_inner = self.encoder.get_ref();
            if self.position < buffer_inner.len() {
                let available = &buffer_inner[self.position..];
                let amount = available.len().min(buf.len());

                buf[..amount].clone_from_slice(&available[..amount]);
                self.position += amount;
                return Ok(amount);
            }
        }

        // The buffer is consumed. Reset.
        self.position = 0;
        self.encoder.get_mut().clear();

        // Time to fill the buffer with more data 8kb at a time.
        let mut byte_count = 0;
        while byte_count < 8_000 {
            let bytes = match self.stage {
                Stage::Metadata => {
                    self.stage = Stage::Elements;
                    serde_json::to_vec(&self.metadata)?
                }
                Stage::Elements => match self.stream.next() {
                    Some(element) => element.encode_to_vec(),
                    None => break,
                },
            };
            byte_count += bytes.len();
            self.encoder.write(&bytes)?;
        }
        self.encoder.flush()?;

        if byte_count > 0 {
            self.read(buf)
        } else {
            Ok(0)
        }
    }
}
