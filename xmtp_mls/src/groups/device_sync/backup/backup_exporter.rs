use super::{backup_stream::BackupStream, BackupOptions};
use crate::{groups::device_sync::DeviceSyncError, XmtpOpenMlsProvider};
use prost::Message;
use std::{
    io::{Read, Write},
    path::Path,
    sync::Arc,
};
use xmtp_proto::xmtp::device_sync::{backup_element::Element, BackupElement, BackupMetadata};
use zstd::stream::Encoder;

pub(super) struct BackupExporter<'a> {
    stage: Stage,
    metadata: BackupMetadata,
    stream: BackupStream,
    position: usize,
    encoder: Encoder<'a, Vec<u8>>,
    encoder_finished: bool,
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
            encoder_finished: false,
        }
    }

    pub fn write_to_file(&mut self, path: impl AsRef<Path>) -> Result<(), DeviceSyncError> {
        let mut file = std::fs::File::create(path.as_ref())?;
        let mut buffer = [0u8; 1024];

        let mut amount = self.read(&mut buffer)?;
        while amount != 0 {
            file.write(&buffer[..amount])?;
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
        while self.encoder.get_ref().len() < 8_000 {
            let bytes = match self.stage {
                Stage::Metadata => {
                    self.stage = Stage::Elements;
                    BackupElement {
                        element: Some(Element::Metadata(self.metadata.clone())),
                    }
                    .encode_to_vec()
                }
                Stage::Elements => match self.stream.next() {
                    Some(element) => element.encode_to_vec(),
                    None => {
                        if !self.encoder_finished {
                            self.encoder_finished = true;
                            self.encoder.do_finish()?;
                        }
                        break;
                    }
                },
            };
            self.encoder.write(&(bytes.len() as u32).to_le_bytes())?;
            self.encoder.write(&bytes)?;
        }
        self.encoder.flush()?;

        if self.encoder.get_ref().is_empty() {
            Ok(0)
        } else {
            self.read(buf)
        }
    }
}
