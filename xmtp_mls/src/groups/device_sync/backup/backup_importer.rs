use crate::groups::device_sync::DeviceSyncError;
use prost::Message;
use std::io::{BufReader, Read};
use xmtp_proto::xmtp::device_sync::{backup_element::Element, BackupElement, BackupMetadata};
use zstd::stream::Decoder;

pub(super) struct BackupImporter<'a> {
    decoded: Vec<u8>,
    decoder: Decoder<'a, BufReader<Box<dyn Read>>>,
}

impl<'a> BackupImporter<'a> {
    pub fn open(reader: impl Read + 'static) -> Result<Self, DeviceSyncError> {
        let reader = Box::new(reader) as Box<_>;
        let decoder = Decoder::new(reader)?;
        Ok(Self {
            decoder,
            decoded: vec![],
        })
    }

    pub fn next_element(&mut self) -> Result<Option<BackupElement>, DeviceSyncError> {
        let mut buffer = [0u8; 1024];
        let mut element_len = 0;
        loop {
            let amount = self.decoder.read(&mut buffer)?;
            self.decoded.extend_from_slice(&buffer[..amount]);

            if element_len == 0 && self.decoded.len() >= 4 {
                let bytes = self.decoded.drain(..4).collect::<Vec<_>>();
                element_len = u32::from_le_bytes(bytes.try_into().expect("is 4 bytes")) as usize;
            }

            if element_len != 0 && self.decoded.len() >= element_len {
                let element = BackupElement::decode(&self.decoded[..element_len]);
                self.decoded.drain(..element_len);
                return Ok(Some(element?));
            }

            if amount == 0 && self.decoded.len() == 0 {
                break;
            }
        }

        Ok(None)
    }

    pub fn get_metadata(
        reader: impl Read + 'static,
    ) -> Result<Option<BackupMetadata>, DeviceSyncError> {
        let el = Self::open(reader)?.next_element()?;
        let Some(el) = el else {
            return Ok(None);
        };
        let BackupElement {
            element: Some(Element::Metadata(metadata)),
        } = el
        else {
            return Ok(None);
        };

        Ok(Some(metadata))
    }
}
