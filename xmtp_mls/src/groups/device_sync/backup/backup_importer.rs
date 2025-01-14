use prost::Message;
use std::{
    collections::VecDeque,
    io::{BufReader, Read},
};
use xmtp_proto::xmtp::device_sync::{
    backup_element::Element, BackupElement, BackupElementSelection, BackupMetadata,
};
use zstd::Decoder;

use crate::groups::device_sync::DeviceSyncError;

pub(super) struct BackupImporter<'a> {
    decoder: Decoder<'a, Vec<u8>>,
}

impl<'a> BackupImporter<'a> {
    pub fn get_metadata(reader: impl Read) -> Result<BackupMetadata, DeviceSyncError> {
        let reader = BufReader::new(reader);
        let mut decoder = Decoder::new(reader)?;

        let mut data = Vec::new();
        let mut buffer = [0u8; 1024];
        let mut len = 0;

        loop {
            let amount = decoder.read(&mut buffer)?;
            if amount == 0 {
                break;
            }
            data.extend_from_slice(&buffer[..amount]);

            if len == 0 && data.len() >= 4 {
                let bytes = data.drain(0..4).collect::<Vec<u8>>();
                len = u32::from_le_bytes(bytes.try_into().expect("Is 4 bytes")) as usize;
                tracing::info!("Len is {len}");
            }
            if len != 0 && data.len() >= len {
                if let Ok(el) = BackupElement::decode(&data[..len]) {
                    tracing::info!("Decoded something");
                    let BackupElement {
                        element: Some(Element::Metadata(metadata)),
                    } = el
                    else {
                        // TODO: make an actual error for this
                        panic!("First element is not metadata");
                    };

                    return Ok(metadata);
                }
            }

            tracing::info!("AAC");
        }

        panic!("No metadata found");
    }
}
