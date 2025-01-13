use prost::Message;
use std::io::Read;
use xmtp_proto::xmtp::device_sync::{
    backup_element::Element, BackupElement, BackupElementSelection, BackupMetadata,
};
use zstd::stream::Decoder;

use crate::groups::device_sync::DeviceSyncError;

pub(super) struct BackupImporter<'a> {
    decoder: Decoder<'a, Vec<u8>>,
}

impl<'a> BackupImporter<'a> {
    pub fn get_metadata(reader: impl Read) -> Result<BackupMetadata, DeviceSyncError> {
        let mut decoder = Decoder::new(reader)?;

        let mut data = vec![];
        let mut buffer = [0u8; 1024];
        while decoder.read(&mut buffer)? != 0 {
            data.extend_from_slice(&buffer);

            if let Ok(el) = BackupElement::decode(&*data) {
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

        panic!("No metadata found");
    }
}
