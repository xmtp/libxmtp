use std::{
    cell::Cell,
    io::{Read, Seek, SeekFrom, Write},
};

use flate2::read::{DeflateDecoder, MultiGzDecoder, ZlibDecoder};
use prost::Message as _;
use xmtp_content_types::{
    ContentCodec as _,
    attachment::{Attachment, AttachmentCodec},
};
use xmtp_proto::xmtp::mls::message_contents::{Compression, EncodedContent};

use crate::{AttachmentError, AttachmentFailureCause};

const CONTENT_FIELD_TAG: u8 = 0x22;
const MAX_METADATA_BYTES: usize = 65_536;
const MAX_PARAMETER_KEY_BYTES: usize = 256;
const MAX_DECOMPRESSED_BYTES: usize = 16_777_216;
const COPY_CHUNK_BYTES: usize = 8192;

fn invalid() -> AttachmentError {
    AttachmentError::new(AttachmentFailureCause::NotAnAttachment)
}

fn local_storage() -> AttachmentError {
    AttachmentError::new(AttachmentFailureCause::LocalStorage)
}

struct TrackedSource<'a, R> {
    source: &'a mut R,
    read_failed: &'a Cell<bool>,
}

impl<R: Read> Read for TrackedSource<'_, R> {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        self.source
            .read(output)
            .inspect_err(|_| self.read_failed.set(true))
    }
}

fn decode_error(read_failed: &Cell<bool>) -> AttachmentError {
    if read_failed.get() {
        local_storage()
    } else {
        invalid()
    }
}

/// Encode the protobuf fields before the attachment content.
///
/// Map entries use sorted keys, so the same inputs produce the same bytes.
/// Task 10 must compute this prefix once per creation and reuse those bytes.
/// The caller writes exactly `content_len` content bytes after this prefix.
pub fn encoded_prefix(filename: Option<&str>, mime_type: &str, content_len: u64) -> Vec<u8> {
    let mut envelope = AttachmentCodec::encode(Attachment {
        filename: filename.map(str::to_owned),
        mime_type: mime_type.to_owned(),
        content: Vec::new(),
    })
    .expect("attachment encoding has no failure path");
    let mut field = EncodedContent {
        r#type: envelope.r#type.take(),
        ..Default::default()
    };
    let mut prefix = field.encode_to_vec();
    let mut parameters = envelope.parameters.drain().collect::<Vec<_>>();
    parameters.sort_unstable_by(|left, right| left.0.cmp(&right.0));
    field.r#type = None;
    for (key, value) in parameters {
        field.parameters.insert(key, value);
        prefix.extend_from_slice(&field.encode_to_vec());
        field.parameters.clear();
    }
    field.fallback = envelope.fallback.take();
    prefix.extend_from_slice(&field.encode_to_vec());
    if content_len != 0 {
        prefix.push(CONTENT_FIELD_TAG);
        encode_varint(content_len, &mut prefix);
    }
    prefix
}

/// Check the fields that the streaming decoder keeps before content bytes.
/// This uses the same envelope as `encoded_prefix` and the decoder's limit.
pub fn retained_fields_fit(filename: Option<&str>, mime_type: &str) -> bool {
    let value_bytes = mime_type.len().saturating_add(filename.map_or(0, str::len));
    if value_bytes > MAX_METADATA_BYTES {
        return false;
    }
    let prefix = encoded_prefix(filename, mime_type, 0);
    AttachmentDecoder::new().push(&prefix).is_ok()
}

pub fn ciphertext_len(prefix_len: usize, content_len: u64) -> u64 {
    (prefix_len as u64)
        .saturating_add(content_len)
        .saturating_add(16)
}

fn encode_varint(mut value: u64, output: &mut Vec<u8>) {
    while value >= 0x80 {
        output.push((value as u8) | 0x80);
        value >>= 7;
    }
    output.push(value as u8);
}

fn decode_varint(bytes: &[u8]) -> Result<Option<u64>, AttachmentError> {
    if bytes.len() > 10 {
        return Err(invalid());
    }
    let mut value = 0u64;
    for (index, &byte) in bytes.iter().enumerate() {
        if index == 9 && byte > 1 {
            return Err(invalid());
        }
        value |= u64::from(byte & 0x7f) << (index * 7);
        if byte < 0x80 {
            return Ok(Some(value));
        }
    }
    if bytes.len() == 10 {
        Err(invalid())
    } else {
        Ok(None)
    }
}

#[derive(Debug, Clone, Copy)]
enum ParseState {
    Tag,
    Length { field: u64, tag_len: usize },
    Data { remaining: usize, kind: DataKind },
    OtherVarint { retain: bool },
    Fixed { remaining: usize },
}

#[derive(Debug, Clone, Copy)]
enum DataKind {
    Content,
    Type,
    Parameter,
    Skip,
}

#[derive(Debug, Clone, Copy)]
enum EntryState {
    Tag,
    Length { field: u64 },
    Data { field: u64, remaining: usize },
    Varint,
    Fixed { remaining: usize },
}

/// Parse one map entry with bounded key and value buffers.
struct ParameterEntry {
    state: EntryState,
    header: Vec<u8>,
    key: Vec<u8>,
    value: Vec<u8>,
    key_too_long: bool,
    value_too_long: bool,
    serialized_len: usize,
}

impl ParameterEntry {
    fn new(prefix_len: usize) -> Self {
        Self {
            state: EntryState::Tag,
            header: Vec::new(),
            key: Vec::new(),
            value: Vec::new(),
            key_too_long: false,
            value_too_long: false,
            serialized_len: prefix_len,
        }
    }

    fn relevant(&self) -> bool {
        !self.key_too_long && matches!(self.key.as_slice(), b"mimeType" | b"filename")
    }

    fn push(&mut self, input: &[u8]) -> Result<(), AttachmentError> {
        self.serialized_len = self.serialized_len.saturating_add(input.len());
        let mut at = 0;
        while at < input.len() {
            match self.state {
                EntryState::Tag | EntryState::Length { .. } | EntryState::Varint => {
                    self.header.push(input[at]);
                    at += 1;
                    let Some(value) = decode_varint(&self.header)? else {
                        continue;
                    };
                    match self.state {
                        EntryState::Tag => {
                            if value >> 3 == 0 || value > u64::from(u32::MAX) {
                                return Err(invalid());
                            }
                            let field = value >> 3;
                            let wire = value & 7;
                            if matches!(field, 1 | 2) && wire != 2 {
                                return Err(invalid());
                            }
                            self.state = match wire {
                                0 => EntryState::Varint,
                                1 => EntryState::Fixed { remaining: 8 },
                                2 => EntryState::Length { field },
                                5 => EntryState::Fixed { remaining: 4 },
                                _ => return Err(invalid()),
                            };
                        }
                        EntryState::Length { field } => {
                            let remaining = usize::try_from(value).map_err(|_| invalid())?;
                            match field {
                                1 => {
                                    self.key.clear();
                                    self.key_too_long = remaining > MAX_PARAMETER_KEY_BYTES;
                                }
                                2 => {
                                    self.value.clear();
                                    self.value_too_long = remaining > MAX_METADATA_BYTES;
                                }
                                _ => {}
                            }
                            self.state = if remaining == 0 {
                                EntryState::Tag
                            } else {
                                EntryState::Data { field, remaining }
                            };
                        }
                        EntryState::Varint => self.state = EntryState::Tag,
                        _ => unreachable!(),
                    }
                    self.header.clear();
                }
                EntryState::Data { field, remaining } => {
                    let take = remaining.min(input.len() - at);
                    let bytes = &input[at..at + take];
                    match field {
                        1 if !self.key_too_long => self.key.extend_from_slice(bytes),
                        2 if !self.value_too_long => {
                            self.value.extend_from_slice(bytes);
                        }
                        _ => {}
                    }
                    at += take;
                    self.state = if take == remaining {
                        EntryState::Tag
                    } else {
                        EntryState::Data {
                            field,
                            remaining: remaining - take,
                        }
                    };
                }
                EntryState::Fixed { remaining } => {
                    let take = remaining.min(input.len() - at);
                    at += take;
                    self.state = if take == remaining {
                        EntryState::Tag
                    } else {
                        EntryState::Fixed {
                            remaining: remaining - take,
                        }
                    };
                }
            }
        }
        Ok(())
    }

    fn finish(self) -> Result<Option<(Vec<u8>, usize)>, AttachmentError> {
        if !matches!(self.state, EntryState::Tag) || !self.header.is_empty() {
            return Err(invalid());
        }
        if !self.relevant() {
            return Ok(None);
        }
        if self.value_too_long {
            return Err(invalid());
        }
        let mut entry = vec![0x0a];
        encode_varint(self.key.len() as u64, &mut entry);
        entry.extend_from_slice(&self.key);
        entry.push(0x12);
        encode_varint(self.value.len() as u64, &mut entry);
        entry.extend_from_slice(&self.value);
        let mut field = vec![0x12];
        encode_varint(entry.len() as u64, &mut field);
        field.extend_from_slice(&entry);
        Ok(Some((field, self.serialized_len)))
    }
}

/// Metadata of an attachment envelope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedMeta {
    pub mime_type: String,
    pub filename: Option<String>,
    /// True when `finish` wrote decompressed bytes to the output sink.
    pub compressed: bool,
}

/// Content events returned by the streaming attachment decoder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentChunk<'a> {
    /// Discard earlier content and start again at byte zero.
    Reset,
    /// Write these bytes after the last content event.
    Bytes(&'a [u8]),
}

/// Parses a protobuf envelope across arbitrary chunk boundaries.
///
/// Apply every event returned by `push` to a temporary content sink, in order,
/// before the next call. `Reset` truncates the sink to zero and rewinds it.
/// The decoder retains only bounded metadata, not content bytes.
pub struct AttachmentDecoder {
    state: ParseState,
    header: Vec<u8>,
    metadata: Vec<u8>,
    retained_bytes: usize,
    parameter: Option<ParameterEntry>,
    content_fields: usize,
    sink_has_content: bool,
}

impl Default for AttachmentDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl AttachmentDecoder {
    pub fn new() -> Self {
        Self {
            state: ParseState::Tag,
            header: Vec::new(),
            metadata: Vec::new(),
            retained_bytes: 0,
            parameter: None,
            content_fields: 0,
            sink_has_content: false,
        }
    }

    fn append_metadata(&mut self, bytes: &[u8]) -> Result<(), AttachmentError> {
        self.append_retained(bytes, bytes.len())
    }

    fn append_retained(
        &mut self,
        bytes: &[u8],
        serialized_len: usize,
    ) -> Result<(), AttachmentError> {
        if serialized_len > MAX_METADATA_BYTES.saturating_sub(self.retained_bytes) {
            return Err(invalid());
        }
        self.retained_bytes += serialized_len;
        self.metadata.extend_from_slice(bytes);
        Ok(())
    }

    /// Return ordered content events. Byte slices borrow from this input chunk.
    /// Repeated content fields in one input chunk keep only the last field.
    pub fn push<'a>(&mut self, input: &'a [u8]) -> Result<Vec<ContentChunk<'a>>, AttachmentError> {
        let mut at = 0;
        let mut content = Vec::with_capacity(2);
        let sink_has_content = self.sink_has_content;
        while at < input.len() {
            match self.state {
                ParseState::Tag | ParseState::Length { .. } | ParseState::OtherVarint { .. } => {
                    self.header.push(input[at]);
                    at += 1;
                    let Some(value) = decode_varint(&self.header)? else {
                        continue;
                    };
                    match self.state {
                        ParseState::Tag => {
                            if value >> 3 == 0 || value > u64::from(u32::MAX) {
                                return Err(invalid());
                            }
                            let field = value >> 3;
                            let wire = value & 7;
                            if (matches!(field, 1..=4) && wire != 2) || (field == 5 && wire != 0) {
                                return Err(invalid());
                            }
                            match wire {
                                0 => {
                                    let retain = field == 5;
                                    let header = std::mem::take(&mut self.header);
                                    if retain {
                                        self.append_metadata(&header)?;
                                    }
                                    self.state = ParseState::OtherVarint { retain };
                                }
                                1 | 5 => {
                                    self.header.clear();
                                    self.state = ParseState::Fixed {
                                        remaining: if wire == 1 { 8 } else { 4 },
                                    };
                                }
                                2 => {
                                    self.state = ParseState::Length {
                                        field,
                                        tag_len: self.header.len(),
                                    };
                                    self.header.clear();
                                }
                                _ => return Err(invalid()),
                            }
                        }
                        ParseState::Length { field, tag_len } => {
                            let remaining = usize::try_from(value).map_err(|_| invalid())?;
                            let kind = match field {
                                1 => DataKind::Type,
                                2 => DataKind::Parameter,
                                4 => DataKind::Content,
                                _ => DataKind::Skip,
                            };
                            if matches!(kind, DataKind::Content) {
                                if self.content_fields > 0 {
                                    content.clear();
                                    if sink_has_content {
                                        content.push(ContentChunk::Reset);
                                    }
                                }
                                self.content_fields = self.content_fields.saturating_add(1);
                            } else if matches!(kind, DataKind::Type) {
                                let mut header = Vec::new();
                                encode_varint((field << 3) | 2, &mut header);
                                header.extend_from_slice(&self.header);
                                // The retained bytes use a canonical tag; the limit uses the input tag.
                                self.append_retained(&header, tag_len + self.header.len())?;
                                if remaining
                                    > MAX_METADATA_BYTES.saturating_sub(self.retained_bytes)
                                {
                                    return Err(invalid());
                                }
                            } else if matches!(kind, DataKind::Parameter) {
                                self.parameter =
                                    Some(ParameterEntry::new(tag_len + self.header.len()));
                            }
                            self.header.clear();
                            if remaining == 0 && matches!(kind, DataKind::Parameter) {
                                self.parameter.take().unwrap().finish()?;
                            }
                            self.state = if remaining == 0 {
                                ParseState::Tag
                            } else {
                                ParseState::Data { remaining, kind }
                            };
                        }
                        ParseState::OtherVarint { retain } => {
                            let header = std::mem::take(&mut self.header);
                            if retain {
                                self.append_metadata(&header)?;
                            }
                            self.state = ParseState::Tag;
                        }
                        _ => unreachable!(),
                    }
                }
                ParseState::Data { remaining, kind } => {
                    let take = remaining.min(input.len() - at);
                    let bytes = &input[at..at + take];
                    match kind {
                        DataKind::Content => content.push(ContentChunk::Bytes(bytes)),
                        DataKind::Type => self.append_metadata(bytes)?,
                        DataKind::Parameter => self.parameter.as_mut().unwrap().push(bytes)?,
                        DataKind::Skip => {}
                    }
                    at += take;
                    if take == remaining
                        && matches!(kind, DataKind::Parameter)
                        && let Some((field, serialized_len)) =
                            self.parameter.take().unwrap().finish()?
                    {
                        self.append_retained(&field, serialized_len)?;
                    }
                    self.state = if take == remaining {
                        ParseState::Tag
                    } else {
                        ParseState::Data {
                            remaining: remaining - take,
                            kind,
                        }
                    };
                }
                ParseState::Fixed { remaining } => {
                    let take = remaining.min(input.len() - at);
                    at += take;
                    self.state = if take == remaining {
                        ParseState::Tag
                    } else {
                        ParseState::Fixed {
                            remaining: remaining - take,
                        }
                    };
                }
            }
        }
        if let Some(last) = content.last() {
            self.sink_has_content = matches!(last, ContentChunk::Bytes(_));
        }
        Ok(content)
    }

    /// Validate metadata and, when needed, decompress the temporary content.
    ///
    /// `source` reads the content slices written by `push`. `output` receives
    /// decompressed bytes only when compression is present. Neither sink is
    /// published until this function and GCM authentication both succeed.
    pub fn finish<R: Read + Seek, W: Write>(
        self,
        source: &mut R,
        output: &mut W,
    ) -> Result<DecodedMeta, AttachmentError> {
        if !matches!(self.state, ParseState::Tag) || !self.header.is_empty() {
            return Err(invalid());
        }
        let envelope = EncodedContent::decode(self.metadata.as_slice()).map_err(|_| invalid())?;
        let ty = envelope.r#type.ok_or_else(invalid)?;
        if ty.authority_id != "xmtp.org" || ty.type_id != "attachment" || ty.version_major != 1 {
            return Err(invalid());
        }
        let compressed = match envelope.compression {
            None => false,
            Some(raw) if raw == Compression::Gzip as i32 => {
                source
                    .seek(SeekFrom::Start(0))
                    .map_err(|_| local_storage())?;
                let read_failed = Cell::new(false);
                let tracked = TrackedSource {
                    source,
                    read_failed: &read_failed,
                };
                copy_bounded(&mut MultiGzDecoder::new(tracked), output, &read_failed)?;
                true
            }
            Some(raw) if raw == Compression::Deflate as i32 => {
                source
                    .seek(SeekFrom::Start(0))
                    .map_err(|_| local_storage())?;
                // Validate zlib before writing. If it fails, try raw DEFLATE.
                let read_failed = Cell::new(false);
                let tracked = TrackedSource {
                    source: &mut *source,
                    read_failed: &read_failed,
                };
                let zlib_ok = probe_zlib(&mut ZlibDecoder::new(tracked), &read_failed)?;
                source
                    .seek(SeekFrom::Start(0))
                    .map_err(|_| local_storage())?;
                let read_failed = Cell::new(false);
                let tracked = TrackedSource {
                    source,
                    read_failed: &read_failed,
                };
                if zlib_ok {
                    copy_bounded(&mut ZlibDecoder::new(tracked), output, &read_failed)?;
                } else {
                    copy_bounded(&mut DeflateDecoder::new(tracked), output, &read_failed)?;
                }
                true
            }
            _ => return Err(invalid()),
        };
        Ok(DecodedMeta {
            mime_type: envelope
                .parameters
                .get("mimeType")
                .cloned()
                .unwrap_or_default(),
            filename: envelope.parameters.get("filename").cloned(),
            compressed,
        })
    }
}

fn copy_bounded(
    reader: &mut impl Read,
    output: &mut impl Write,
    read_failed: &Cell<bool>,
) -> Result<(), AttachmentError> {
    let mut count = 0usize;
    let mut chunk = [0u8; COPY_CHUNK_BYTES];
    loop {
        let request = (MAX_DECOMPRESSED_BYTES - count + 1).min(chunk.len());
        let read = reader
            .read(&mut chunk[..request])
            .map_err(|_| decode_error(read_failed))?;
        if read == 0 {
            return if read_failed.get() {
                Err(local_storage())
            } else {
                Ok(())
            };
        }
        if count + read > MAX_DECOMPRESSED_BYTES {
            return Err(invalid());
        }
        output
            .write_all(&chunk[..read])
            .map_err(|_| AttachmentError::new(AttachmentFailureCause::LocalStorage))?;
        count += read;
    }
}

fn probe_zlib(reader: &mut impl Read, read_failed: &Cell<bool>) -> Result<bool, AttachmentError> {
    let mut count = 0usize;
    let mut chunk = [0u8; COPY_CHUNK_BYTES];
    loop {
        let request = (MAX_DECOMPRESSED_BYTES - count + 1).min(chunk.len());
        let read = match reader.read(&mut chunk[..request]) {
            Ok(read) => read,
            Err(_) if read_failed.get() => return Err(local_storage()),
            Err(_) => return Ok(false),
        };
        if read == 0 {
            if read_failed.get() {
                return Err(local_storage());
            }
            return Ok(true);
        }
        if count + read > MAX_DECOMPRESSED_BYTES {
            return Err(invalid());
        }
        count += read;
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::io::{self, Cursor, Read, Seek, SeekFrom, Write as _};

    use flate2::{
        Compression as FlateCompression,
        write::{DeflateEncoder, GzEncoder, ZlibEncoder},
    };
    use xmtp_content_types::{
        ContentCodec as _,
        attachment::{Attachment, AttachmentCodec},
    };

    use super::*;

    #[derive(Clone, PartialEq, prost::Message)]
    struct OrderedAttachmentEnvelope {
        #[prost(message, optional, tag = "1")]
        r#type: Option<xmtp_proto::xmtp::mls::message_contents::ContentTypeId>,
        #[prost(btree_map = "string, string", tag = "2")]
        parameters: BTreeMap<String, String>,
        #[prost(string, optional, tag = "3")]
        fallback: Option<String>,
        #[prost(bytes = "vec", tag = "4")]
        content: Vec<u8>,
    }

    fn envelope(content: Vec<u8>) -> EncodedContent {
        AttachmentCodec::encode(Attachment {
            filename: Some("report.pdf".to_owned()),
            mime_type: "application/pdf".to_owned(),
            content,
        })
        .expect("attachment encoding has no failure path")
    }

    fn decode(bytes: &[u8]) -> Result<(Vec<u8>, Vec<u8>, DecodedMeta), AttachmentError> {
        let mut decoder = AttachmentDecoder::new();
        let mut temporary = Cursor::new(Vec::new());
        for byte in bytes.chunks(1) {
            for event in decoder.push(byte)? {
                match event {
                    ContentChunk::Reset => {
                        temporary.get_mut().clear();
                        temporary.set_position(0);
                    }
                    ContentChunk::Bytes(slice) => {
                        temporary.write_all(slice).map_err(|_| invalid())?;
                    }
                }
            }
        }
        let mut decompressed = Vec::new();
        let meta = decoder.finish(&mut temporary, &mut decompressed)?;
        Ok((temporary.into_inner(), decompressed, meta))
    }

    fn decode_split_metadata(bytes: &[u8], split: usize) -> Result<DecodedMeta, AttachmentError> {
        let mut decoder = AttachmentDecoder::new();
        decoder.push(&bytes[..split])?;
        decoder.push(&bytes[split..])?;
        decoder.finish(&mut Cursor::new(Vec::new()), &mut Vec::new())
    }

    struct FaultingSource {
        bytes: Cursor<Vec<u8>>,
        seeks: usize,
        fail_read_after_seeks: Option<usize>,
        fail_seek: bool,
    }

    impl Read for FaultingSource {
        fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
            if self
                .fail_read_after_seeks
                .is_some_and(|count| self.seeks >= count)
            {
                return Err(io::Error::other("local content read failed"));
            }
            self.bytes.read(output)
        }
    }

    impl Seek for FaultingSource {
        fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
            if self.fail_seek {
                return Err(io::Error::other("local content seek failed"));
            }
            self.seeks += 1;
            self.bytes.seek(position)
        }
    }

    fn finish_with_faulting_source(
        content: Vec<u8>,
        compression: Compression,
        fail_read_after_seeks: Option<usize>,
        fail_seek: bool,
    ) -> Result<DecodedMeta, AttachmentError> {
        let mut value = envelope(content.clone());
        value.compression = Some(compression as i32);
        let encoded = value.encode_to_vec();
        let mut decoder = AttachmentDecoder::new();
        let _ = decoder.push(&encoded)?;
        let mut source = FaultingSource {
            bytes: Cursor::new(content),
            seeks: 0,
            fail_read_after_seeks,
            fail_seek,
        };
        decoder.finish(&mut source, &mut Vec::new())
    }

    // verifies: ATCH-012
    #[xmtp_common::test(unwrap_try = true)]
    async fn encoder_matches_prost() {
        for len in [0, 1, 127, 128, 16_384, 70_000] {
            let content = vec![42; len];
            // One parameter has one protobuf map entry and a stable order.
            let expected = AttachmentCodec::encode(Attachment {
                filename: None,
                mime_type: "application/octet-stream".to_owned(),
                content: content.clone(),
            })?;
            let mut streamed = encoded_prefix(None, "application/octet-stream", len as u64);
            streamed.extend_from_slice(&content);
            assert_eq!(streamed, expected.encode_to_vec(), "length {len}");
            assert_eq!(
                ciphertext_len(streamed.len() - len, len as u64),
                streamed.len() as u64 + 16
            );

            let mut with_filename =
                encoded_prefix(Some("report.pdf"), "application/pdf", len as u64);
            assert_eq!(
                with_filename,
                encoded_prefix(Some("report.pdf"), "application/pdf", len as u64)
            );
            with_filename.extend_from_slice(&content);
            assert_eq!(
                EncodedContent::decode(with_filename.as_slice())?,
                envelope(content.clone())
            );
            let expected = envelope(content);
            assert_eq!(
                with_filename,
                OrderedAttachmentEnvelope {
                    r#type: expected.r#type,
                    parameters: expected.parameters.into_iter().collect(),
                    fallback: expected.fallback,
                    content: expected.content,
                }
                .encode_to_vec()
            );
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn ciphertext_len_saturates() {
        assert_eq!(ciphertext_len(1, u64::MAX), u64::MAX);
        assert_eq!(ciphertext_len(usize::MAX, u64::MAX), u64::MAX);
    }

    // verifies: ATCH-012
    #[xmtp_common::test(unwrap_try = true)]
    async fn decoder_any_field_order() {
        let original = envelope(b"file data".to_vec());
        let mut fields = Vec::new();
        let mut ty = original.clone();
        ty.parameters.clear();
        ty.fallback = None;
        ty.content.clear();
        fields.push(ty.encode_to_vec());
        let mut params = original.clone();
        params.r#type = None;
        params.fallback = None;
        params.content.clear();
        fields.push(params.encode_to_vec());
        let mut fallback = original.clone();
        fallback.r#type = None;
        fallback.parameters.clear();
        fallback.content.clear();
        fields.push(fallback.encode_to_vec());
        let mut content = original.clone();
        content.r#type = None;
        content.parameters.clear();
        content.fallback = None;
        fields.push(content.encode_to_vec());
        for order in [[0, 1, 2, 3], [3, 2, 1, 0], [1, 3, 0, 2]] {
            let bytes = order
                .iter()
                .flat_map(|&index| fields[index].iter().copied())
                .collect::<Vec<_>>();
            let (stored, decompressed, meta) = decode(&bytes)?;
            assert_eq!(stored, b"file data");
            assert!(decompressed.is_empty());
            assert_eq!(meta.mime_type, "application/pdf");
            assert_eq!(meta.filename.as_deref(), Some("report.pdf"));
            assert!(!meta.compressed);
        }
        let mut wrong = original.clone();
        wrong.r#type.as_mut().expect("type").type_id = "text".into();
        assert_eq!(
            decode(&wrong.encode_to_vec()).unwrap_err().cause,
            AttachmentFailureCause::NotAnAttachment
        );
        let mut large = original;
        large
            .parameters
            .insert("junk".to_owned(), "a".repeat(65_537));
        let (_, _, meta) = decode(&large.encode_to_vec())?;
        assert_eq!(meta.mime_type, "application/pdf");
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn decoder_skips_large_unknown_parameters() {
        let mut value = envelope(b"file data".to_vec());
        value.parameters.insert("junk".into(), "a".repeat(70_000));
        value.fallback = Some("b".repeat(70_000));
        let (stored, _, meta) = decode(&value.encode_to_vec())?;
        assert_eq!(stored, b"file data");
        assert_eq!(meta.mime_type, "application/pdf");
        assert_eq!(meta.filename.as_deref(), Some("report.pdf"));

        value
            .parameters
            .insert("filename".into(), "c".repeat(70_000));
        assert_eq!(
            decode(&value.encode_to_vec()).unwrap_err().cause,
            AttachmentFailureCause::NotAnAttachment
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn retained_fields_total_cap_is_enforced() {
        let with_values = |mime_type: usize, filename: usize| {
            let mut value = envelope(b"file data".to_vec());
            value
                .parameters
                .insert("mimeType".into(), "m".repeat(mime_type));
            value
                .parameters
                .insert("filename".into(), "f".repeat(filename));
            value
        };
        // The decoder keeps the type and the mimeType and filename entries.
        let retained_len = |value: &EncodedContent| {
            EncodedContent {
                r#type: value.r#type.clone(),
                parameters: value.parameters.clone(),
                ..Default::default()
            }
            .encoded_len()
        };

        // Each value is under the per-value cap. Together they are over the total.
        let over = with_values(40_000, 30_000);
        assert!(retained_len(&over) > MAX_METADATA_BYTES);
        assert_eq!(
            decode(&over.encode_to_vec()).unwrap_err().cause,
            AttachmentFailureCause::NotAnAttachment
        );

        let filename = (20_000..30_000)
            .find(|&len| retained_len(&with_values(40_000, len)) == MAX_METADATA_BYTES)
            .expect("a filename length that fills the cap exactly");
        let (stored, _, meta) = decode(&with_values(40_000, filename).encode_to_vec())?;
        assert_eq!(stored, b"file data");
        assert_eq!(meta.mime_type.len(), 40_000);
        assert_eq!(meta.filename.map(|name| name.len()), Some(filename));

        let one_over = with_values(40_000, filename + 1);
        assert_eq!(retained_len(&one_over), MAX_METADATA_BYTES + 1);
        assert_eq!(
            decode(&one_over.encode_to_vec()).unwrap_err().cause,
            AttachmentFailureCause::NotAnAttachment
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn overlong_outer_tags_count_toward_retained_cap() {
        let type_field = EncodedContent {
            r#type: envelope(Vec::new()).r#type,
            ..Default::default()
        }
        .encode_to_vec();
        assert_eq!(type_field[0], 0x0a);

        let build = |value_len: usize, overlong_type: bool| {
            let mut bytes = type_field.clone();
            if overlong_type {
                bytes.splice(0..1, [0x8a, 0x00]);
            }
            let split = if overlong_type { 1 } else { bytes.len() + 1 };
            let mut entry = b"\x0a\x08filename\x12".to_vec();
            encode_varint(value_len as u64, &mut entry);
            entry.extend(std::iter::repeat_n(b'f', value_len));
            bytes.extend_from_slice(if overlong_type {
                &[0x12][..]
            } else {
                &[0x92, 0x00][..]
            });
            encode_varint(entry.len() as u64, &mut bytes);
            bytes.extend_from_slice(&entry);
            (bytes, split)
        };

        for overlong_type in [false, true] {
            // The length varints stay three bytes at these sizes.
            let overhead = build(64_000, overlong_type).0.len() - 64_000;
            let at_cap = MAX_METADATA_BYTES - overhead;
            for (size, accepted) in [(at_cap, true), (at_cap + 1, false)] {
                let (bytes, split) = build(size, overlong_type);
                assert_eq!(bytes.len(), MAX_METADATA_BYTES + usize::from(!accepted));
                assert!(EncodedContent::decode(bytes.as_slice()).is_ok());
                let result = decode_split_metadata(&bytes, split);
                if accepted {
                    assert!(result.is_ok(), "at cap with overlong_type={overlong_type}");
                } else {
                    match result {
                        Err(error) => {
                            assert_eq!(error.cause, AttachmentFailureCause::NotAnAttachment);
                        }
                        Ok(_) => panic!("one byte over cap must fail"),
                    }
                }
            }
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn decoder_repeated_content_last_wins() {
        let mut bytes = envelope(b"first".to_vec()).encode_to_vec();
        bytes.extend_from_slice(b"\x22\x06second");
        assert_eq!(EncodedContent::decode(bytes.as_slice())?.content, b"second");
        let (stored, decompressed, _) = decode(&bytes)?;
        assert_eq!(stored, b"second");
        assert!(decompressed.is_empty());
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn decoder_repeated_compressed_content_last_wins() {
        let mut zlib = ZlibEncoder::new(Vec::new(), FlateCompression::default());
        zlib.write_all(b"second")?;
        let zlib = zlib.finish()?;
        let mut value = envelope(b"invalid zlib".to_vec());
        value.compression = Some(Compression::Deflate as i32);
        let mut bytes = value.encode_to_vec();
        bytes.push(CONTENT_FIELD_TAG);
        encode_varint(zlib.len() as u64, &mut bytes);
        bytes.extend_from_slice(&zlib);
        assert_eq!(EncodedContent::decode(bytes.as_slice())?.content, zlib);
        let (stored, decompressed, meta) = decode(&bytes)?;
        assert_eq!(stored, zlib);
        assert_eq!(decompressed, b"second");
        assert!(meta.compressed);
    }

    // verifies: ATCH-039
    #[xmtp_common::test(unwrap_try = true)]
    fn repeated_content_fields_coalesce_per_push() {
        let prefix = encoded_prefix(Some("report.pdf"), "application/pdf", 0);
        let mut repeated = Vec::with_capacity(64 * 1024);
        for _ in 0..(64 * 1024 / 3) {
            repeated.extend_from_slice(b"\x22\x01x");
        }

        let mut decoder = AttachmentDecoder::new();
        assert!(decoder.push(&prefix)?.is_empty());
        let mut stored = Cursor::new(Vec::new());
        for event in decoder.push(b"\x22\x01y")? {
            if let ContentChunk::Bytes(bytes) = event {
                stored.write_all(bytes)?;
            }
        }
        assert_eq!(stored.get_ref().as_slice(), b"y");

        let events = decoder.push(&repeated)?;
        assert!(events.len() <= 2, "too many events: {}", events.len());
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, ContentChunk::Reset))
                .count(),
            1
        );
        for event in events {
            match event {
                ContentChunk::Reset => {
                    stored.get_mut().clear();
                    stored.set_position(0);
                }
                ContentChunk::Bytes(bytes) => stored.write_all(bytes)?,
            }
        }
        decoder.finish(&mut stored, &mut Vec::new())?;
        assert_eq!(stored.into_inner(), b"x");

        let mut fresh = AttachmentDecoder::new();
        fresh.push(&prefix)?;
        let events = fresh.push(&repeated)?;
        assert!(events.len() <= 1, "too many events: {}", events.len());
        assert!(
            events
                .iter()
                .all(|event| !matches!(event, ContentChunk::Reset))
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn repeated_entry_fields_replace_not_append() {
        let mut bytes = envelope(b"content".to_vec()).encode_to_vec();
        let mut entry = Vec::new();
        for key in ["junk", "junk", "filename"] {
            entry.push(0x0a);
            encode_varint(key.len() as u64, &mut entry);
            entry.extend_from_slice(key.as_bytes());
        }
        for value in ["a".repeat(60_000), "b".repeat(60_000)] {
            entry.push(0x12);
            encode_varint(value.len() as u64, &mut entry);
            entry.extend_from_slice(value.as_bytes());
        }
        bytes.push(0x12);
        encode_varint(entry.len() as u64, &mut bytes);
        bytes.extend_from_slice(&entry);
        let expected = EncodedContent::decode(bytes.as_slice())?;
        assert_eq!(expected.parameters.get("filename").unwrap().len(), 60_000);
        assert!(
            expected.parameters["filename"]
                .bytes()
                .all(|byte| byte == b'b')
        );
        assert_eq!(
            decode(&bytes).expect_err("oversized entry must fail").cause,
            AttachmentFailureCause::NotAnAttachment
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn recognized_repeated_values_count_serialized() {
        let mut value = envelope(b"content".to_vec());
        value.parameters.remove("filename");
        for size in [40_000, 10_000] {
            let mut bytes = value.encode_to_vec();
            let mut entry = Vec::new();
            entry.extend_from_slice(b"\x0a\x08filename");
            for byte in *b"ab" {
                entry.push(0x12);
                encode_varint(size, &mut entry);
                entry.extend(std::iter::repeat_n(byte, size as usize));
            }
            bytes.push(0x12);
            encode_varint(entry.len() as u64, &mut bytes);
            bytes.extend_from_slice(&entry);
            if size == 40_000 {
                assert_eq!(
                    decode(&bytes).expect_err("oversized entry must fail").cause,
                    AttachmentFailureCause::NotAnAttachment
                );
            } else {
                let (_, _, meta) = decode(&bytes)?;
                assert_eq!(
                    meta.filename.as_deref(),
                    Some("b".repeat(size as usize).as_str())
                );
            }
        }
    }

    // verifies: ATCH-051
    #[xmtp_common::test(unwrap_try = true)]
    async fn protobuf_keys_above_u32_are_rejected() {
        // Field number 2^32 with wire type 0: the key does not fit in 32 bits.
        let oversized_key = 1u64 << 35;

        let mut entry = Vec::new();
        for (field, value) in [(0x0a, "filename"), (0x12, "report.pdf")] {
            entry.push(field);
            encode_varint(value.len() as u64, &mut entry);
            entry.extend_from_slice(value.as_bytes());
        }
        encode_varint(oversized_key, &mut entry);
        entry.push(0);
        let mut in_entry = envelope(b"content".to_vec()).encode_to_vec();
        in_entry.push(0x12);
        encode_varint(entry.len() as u64, &mut in_entry);
        in_entry.extend_from_slice(&entry);
        assert!(EncodedContent::decode(in_entry.as_slice()).is_err());
        assert_eq!(
            decode(&in_entry).unwrap_err().cause,
            AttachmentFailureCause::NotAnAttachment
        );

        let mut top_level = envelope(b"content".to_vec()).encode_to_vec();
        encode_varint(oversized_key, &mut top_level);
        top_level.push(0);
        assert!(EncodedContent::decode(top_level.as_slice()).is_err());
        assert_eq!(
            decode(&top_level).unwrap_err().cause,
            AttachmentFailureCause::NotAnAttachment
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn entry_value_before_final_key_is_kept() {
        let mut envelope = envelope(b"content".to_vec());
        envelope.parameters.remove("filename");
        let mut bytes = envelope.encode_to_vec();
        let mut entry = Vec::new();
        for (field, value) in [(0x0a, "junk"), (0x12, "report.pdf"), (0x0a, "filename")] {
            entry.push(field);
            encode_varint(value.len() as u64, &mut entry);
            entry.extend_from_slice(value.as_bytes());
        }
        bytes.push(0x12);
        encode_varint(entry.len() as u64, &mut bytes);
        bytes.extend_from_slice(&entry);

        let expected = EncodedContent::decode(bytes.as_slice())?;
        assert_eq!(expected.parameters["filename"], "report.pdf");
        // decode feeds the streaming parser one byte at a time.
        let (_, _, meta) = decode(&bytes)?;
        assert_eq!(meta.filename.as_deref(), Some("report.pdf"));
        assert_eq!(
            meta.filename.as_deref(),
            Some(expected.parameters["filename"].as_str())
        );
    }

    // verifies: CTYPE-001, CTYPE-016
    #[xmtp_common::test(unwrap_try = true)]
    async fn decoder_accepts_later_minor_versions() {
        let mut value = envelope(b"file data".to_vec());
        let ty = value.r#type.as_mut().expect("attachment type");
        ty.version_minor = 3;
        let (stored, decompressed, meta) = decode(&value.encode_to_vec())?;
        assert_eq!(stored, b"file data");
        assert!(decompressed.is_empty());
        assert_eq!(meta.mime_type, "application/pdf");
        assert_eq!(meta.filename.as_deref(), Some("report.pdf"));

        value
            .r#type
            .as_mut()
            .expect("attachment type")
            .version_major = 2;
        assert_eq!(
            decode(&value.encode_to_vec()).unwrap_err().cause,
            AttachmentFailureCause::NotAnAttachment
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn decoder_compressed_content() {
        let text = b"hello compressed XMTP";
        let mut zlib = ZlibEncoder::new(Vec::new(), FlateCompression::default());
        zlib.write_all(text)?;
        let zlib = zlib.finish()?;
        let mut raw = DeflateEncoder::new(Vec::new(), FlateCompression::default());
        raw.write_all(text)?;
        let raw = raw.finish()?;
        let mut first = GzEncoder::new(Vec::new(), FlateCompression::default());
        first.write_all(b"hello ")?;
        let mut second = GzEncoder::new(Vec::new(), FlateCompression::default());
        second.write_all(b"compressed XMTP")?;
        let gzip = [first.finish()?, second.finish()?].concat();
        for (algorithm, compressed) in [
            (Compression::Deflate, zlib),
            (Compression::Deflate, raw),
            (Compression::Gzip, gzip),
        ] {
            let mut value = envelope(compressed.clone());
            value.compression = Some(algorithm as i32);
            let (stored, output, meta) = decode(&value.encode_to_vec())?;
            assert_eq!(stored, compressed);
            assert_eq!(output, text);
            assert!(meta.compressed);
        }
        let mut oversized = ZlibEncoder::new(Vec::new(), FlateCompression::default());
        oversized.write_all(&vec![7u8; MAX_DECOMPRESSED_BYTES + 1])?;
        let mut value = envelope(oversized.finish()?);
        value.compression = Some(Compression::Deflate as i32);
        assert_eq!(
            decode(&value.encode_to_vec()).unwrap_err().cause,
            AttachmentFailureCause::NotAnAttachment
        );
        value.content = b"invalid compressed bytes".to_vec();
        assert_eq!(
            decode(&value.encode_to_vec()).unwrap_err().cause,
            AttachmentFailureCause::NotAnAttachment
        );
        value.compression = Some(99);
        assert_eq!(
            decode(&value.encode_to_vec()).unwrap_err().cause,
            AttachmentFailureCause::NotAnAttachment
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn source_io_errors_are_local_storage() {
        let text = b"valid compressed content";
        let mut gzip = GzEncoder::new(Vec::new(), FlateCompression::default());
        gzip.write_all(text)?;
        let gzip = gzip.finish()?;
        let mut zlib = ZlibEncoder::new(Vec::new(), FlateCompression::default());
        zlib.write_all(text)?;
        let zlib = zlib.finish()?;

        for (content, compression, fail_read_after_seeks, fail_seek) in [
            (gzip.clone(), Compression::Gzip, Some(1), false),
            (zlib.clone(), Compression::Deflate, Some(1), false),
            (zlib.clone(), Compression::Deflate, Some(2), false),
            (gzip, Compression::Gzip, None, true),
        ] {
            let error =
                finish_with_faulting_source(content, compression, fail_read_after_seeks, fail_seek)
                    .expect_err("source I/O failure must stop decoding");
            assert_eq!(error.cause, AttachmentFailureCause::LocalStorage);
        }

        let mut raw = DeflateEncoder::new(Vec::new(), FlateCompression::default());
        raw.write_all(text)?;
        let mut output = Vec::new();
        let compressed = raw.finish()?;
        let mut value = envelope(compressed.clone());
        value.compression = Some(Compression::Deflate as i32);
        let mut decoder = AttachmentDecoder::new();
        let encoded = value.encode_to_vec();
        let _ = decoder.push(&encoded)?;
        let mut source = Cursor::new(compressed);
        decoder.finish(&mut source, &mut output)?;
        assert_eq!(output, text);

        let mut malformed = envelope(b"not gzip".to_vec());
        malformed.compression = Some(Compression::Gzip as i32);
        assert_eq!(
            decode(&malformed.encode_to_vec()).unwrap_err().cause,
            AttachmentFailureCause::NotAnAttachment
        );
    }
}
