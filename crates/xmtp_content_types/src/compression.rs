//! Compression for the bytes inside an `EncodedContent` envelope.

use std::io::{self, Read, Write};

use flate2::{
    Compression as FlateCompression,
    read::{DeflateDecoder, MultiGzDecoder, ZlibDecoder},
    write::{GzEncoder, ZlibEncoder},
};
use xmtp_proto::xmtp::mls::message_contents::{Compression, EncodedContent};

use crate::CodecError;

pub const MAX_DECOMPRESSED_BYTES: usize = 16_777_216;
pub const COMPRESSION_CHUNK_BYTES: usize = 64 * 1024;

#[derive(Debug)]
enum DecompressFailure {
    Invalid(io::Error),
    Limit,
}

/// Total decompressed bytes allowed while decoding one message and its nested content.
#[derive(Debug)]
pub struct DecompressionBudget {
    remaining: usize,
    peak_capacity: usize,
}

impl Default for DecompressionBudget {
    fn default() -> Self {
        Self::new()
    }
}

impl DecompressionBudget {
    pub const fn new() -> Self {
        Self {
            remaining: MAX_DECOMPRESSED_BYTES,
            peak_capacity: 0,
        }
    }

    /// Largest output buffer capacity used by one decompression.
    pub const fn peak_capacity(&self) -> usize {
        self.peak_capacity
    }

    /// Total bytes successfully decompressed so far.
    pub const fn used(&self) -> usize {
        MAX_DECOMPRESSED_BYTES - self.remaining
    }
}

fn read_bounded(
    reader: &mut impl Read,
    budget: &mut DecompressionBudget,
) -> Result<Vec<u8>, DecompressFailure> {
    let mut output = Vec::new();
    let mut chunk = [0u8; COMPRESSION_CHUNK_BYTES];
    loop {
        let remaining = budget.remaining - output.len();
        let request = remaining.saturating_add(1).min(COMPRESSION_CHUNK_BYTES);
        let read = reader
            .read(&mut chunk[..request])
            .map_err(DecompressFailure::Invalid)?;
        if read == 0 {
            budget.remaining -= output.len();
            return Ok(output);
        }
        if read > remaining {
            return Err(DecompressFailure::Limit);
        }
        output.reserve_exact(read);
        output.extend_from_slice(&chunk[..read]);
        budget.peak_capacity = budget.peak_capacity.max(output.capacity());
    }
}

/// Compresses content with a wire-compatible zlib or gzip frame.
// implements: CTYPE-023
pub fn compress(
    mut content: EncodedContent,
    algorithm: Compression,
) -> Result<EncodedContent, CodecError> {
    if content.compression.is_some() {
        return Err(CodecError::Encode("content is already compressed".into()));
    }
    let bytes = match algorithm {
        Compression::Deflate => {
            let mut encoder = ZlibEncoder::new(Vec::new(), FlateCompression::default());
            encoder
                .write_all(&content.content)
                .map_err(|e| CodecError::Encode(e.to_string()))?;
            encoder
                .finish()
                .map_err(|e| CodecError::Encode(e.to_string()))?
        }
        Compression::Gzip => {
            let mut encoder = GzEncoder::new(Vec::new(), FlateCompression::default());
            encoder
                .write_all(&content.content)
                .map_err(|e| CodecError::Encode(e.to_string()))?;
            encoder
                .finish()
                .map_err(|e| CodecError::Encode(e.to_string()))?
        }
    };
    content.content = bytes;
    content.compression = Some(algorithm as i32);
    Ok(content)
}

/// Leaves content unchanged when the caller does not request compression.
pub fn compress_if_requested(
    content: EncodedContent,
    algorithm: Option<Compression>,
) -> Result<EncodedContent, CodecError> {
    match algorithm {
        Some(algorithm) => compress(content, algorithm),
        None => Ok(content),
    }
}

/// Decompresses before a caller selects a content codec.
// implements: CTYPE-024, CTYPE-025
pub fn decompress(content: EncodedContent) -> Result<EncodedContent, CodecError> {
    decompress_with_budget(content, &mut DecompressionBudget::new())
}

/// Decompresses one envelope and charges its output to a shared message budget.
pub fn decompress_with_budget(
    mut content: EncodedContent,
    budget: &mut DecompressionBudget,
) -> Result<EncodedContent, CodecError> {
    let Some(raw_algorithm) = content.compression else {
        return Ok(content);
    };
    let algorithm = Compression::try_from(raw_algorithm)
        .map_err(|_| CodecError::Decode(format!("unknown compression value {raw_algorithm}")))?;
    let result = match algorithm {
        Compression::Gzip => {
            read_bounded(&mut MultiGzDecoder::new(content.content.as_slice()), budget)
        }
        Compression::Deflate => {
            match read_bounded(&mut ZlibDecoder::new(content.content.as_slice()), budget) {
                Ok(bytes) => Ok(bytes),
                Err(DecompressFailure::Invalid(_)) => {
                    read_bounded(&mut DeflateDecoder::new(content.content.as_slice()), budget)
                }
                Err(DecompressFailure::Limit) => Err(DecompressFailure::Limit),
            }
        }
    };
    content.content = result.map_err(|error| match error {
        DecompressFailure::Invalid(error) => {
            CodecError::Decode(format!("decompression failed: {error}"))
        }
        DecompressFailure::Limit => CodecError::Decode(format!(
            "decompressed content exceeds {MAX_DECOMPRESSED_BYTES} bytes"
        )),
    })?;
    content.compression = None;
    Ok(content)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ContentCodec, text::TextCodec};

    // Generated with Python zlib.compressobj(wbits=-15) for "hello compressed XMTP".
    // verifies: CTYPE-024
    #[xmtp_common::test(unwrap_try = true)]
    async fn decompress_ios_raw_deflate_vector() {
        let mut content = TextCodec::encode(String::new())?;
        content.content = hex::decode("cb48cdc9c95748cecf2d284a2d2e4e4d5188f00d090000")?;
        content.compression = Some(Compression::Deflate as i32);
        assert_eq!(
            TextCodec::decode(decompress(content)?)?,
            "hello compressed XMTP"
        );
    }

    // Generated with Python zlib.compress for "hello compressed XMTP".
    // verifies: CTYPE-024
    #[xmtp_common::test(unwrap_try = true)]
    async fn decompress_kotlin_zlib_vector() {
        let mut content = TextCodec::encode(String::new())?;
        content.content =
            hex::decode("789ccb48cdc9c95748cecf2d284a2d2e4e4d5188f00d090000599907d3")?;
        content.compression = Some(Compression::Deflate as i32);
        assert_eq!(
            TextCodec::decode(decompress(content)?)?,
            "hello compressed XMTP"
        );
    }

    // Generated with macOS compression_encode_buffer using COMPRESSION_LZFSE
    // (0x801) on eight copies of "hello compressed XMTP".
    // verifies: CTYPE-024
    #[xmtp_common::test(unwrap_try = true)]
    async fn lzfse_labelled_gzip_is_decode_failure() {
        let mut content = TextCodec::encode(String::new())?;
        content.content = hex::decode(
            "6276786ea800000026000000e00568656c6c6f20636f6d7072657373656420584d54503815f077e25450060000000000000062767824",
        )?;
        content.compression = Some(Compression::Gzip as i32);
        assert!(matches!(decompress(content), Err(CodecError::Decode(_))));
    }

    // verifies: CTYPE-024
    #[xmtp_common::test(unwrap_try = true)]
    async fn unknown_compression_is_decode_failure() {
        let mut content = TextCodec::encode("hello".into())?;
        content.compression = Some(99);
        assert!(matches!(decompress(content), Err(CodecError::Decode(_))));
    }

    // verifies: CTYPE-025
    #[xmtp_common::test(unwrap_try = true)]
    async fn decompression_stops_at_limit() {
        let source = vec![b'A'; MAX_DECOMPRESSED_BYTES + 1];
        let mut encoder = ZlibEncoder::new(Vec::new(), FlateCompression::default());
        encoder.write_all(&source)?;
        let compressed = encoder.finish()?;
        assert!(compressed.len() < 20_000);
        let mut budget = DecompressionBudget::new();
        let result = read_bounded(&mut ZlibDecoder::new(compressed.as_slice()), &mut budget);
        assert!(matches!(result, Err(DecompressFailure::Limit)));
        assert!(budget.peak_capacity() <= MAX_DECOMPRESSED_BYTES + COMPRESSION_CHUNK_BYTES);
        let mut content = TextCodec::encode(String::new())?;
        content.content = compressed;
        content.compression = Some(Compression::Deflate as i32);
        assert!(matches!(decompress(content), Err(CodecError::Decode(_))));
    }

    // verifies: CTYPE-025
    #[xmtp_common::test(unwrap_try = true)]
    async fn stored_block_then_compressed_tail_stays_within_capacity_limit() {
        use flate2::write::DeflateEncoder;

        let first = vec![b'B'; 49_052];
        let tail = vec![b'A'; MAX_DECOMPRESSED_BYTES + 1 - first.len()];
        let mut raw_tail = DeflateEncoder::new(Vec::new(), FlateCompression::default());
        raw_tail.write_all(&tail)?;
        let raw_tail = raw_tail.finish()?;

        // One non-final stored DEFLATE block, then a final compressed block.
        let length = u16::try_from(first.len())?;
        let mut compressed = vec![0x78, 0x9c, 0x00];
        compressed.extend_from_slice(&length.to_le_bytes());
        compressed.extend_from_slice(&(!length).to_le_bytes());
        compressed.extend_from_slice(&first);
        compressed.extend_from_slice(&raw_tail);
        let mut s1 = 1u32;
        let mut s2 = 0u32;
        for byte in first.iter().chain(&tail) {
            s1 = (s1 + u32::from(*byte)) % 65_521;
            s2 = (s2 + s1) % 65_521;
        }
        compressed.extend_from_slice(&((s2 << 16) | s1).to_be_bytes());

        let mut budget = DecompressionBudget::new();
        let result = read_bounded(&mut ZlibDecoder::new(compressed.as_slice()), &mut budget);
        assert!(matches!(result, Err(DecompressFailure::Limit)));
        assert!(budget.peak_capacity() <= MAX_DECOMPRESSED_BYTES + COMPRESSION_CHUNK_BYTES);
        let mut content = TextCodec::encode(String::new())?;
        content.content = compressed;
        content.compression = Some(Compression::Deflate as i32);
        assert!(matches!(decompress(content), Err(CodecError::Decode(_))));
    }

    // verifies: CTYPE-024
    #[xmtp_common::test(unwrap_try = true)]
    async fn gzip_decodes_all_members() {
        let mut content = TextCodec::encode(String::new())?;
        let mut first = GzEncoder::new(Vec::new(), FlateCompression::default());
        first.write_all(b"hello ")?;
        let mut second = GzEncoder::new(Vec::new(), FlateCompression::default());
        second.write_all(b"world")?;
        content.content = first.finish()?;
        content.content.extend(second.finish()?);
        content.compression = Some(Compression::Gzip as i32);
        assert_eq!(decompress(content)?.content, b"hello world");
    }

    // verifies: CTYPE-023, CTYPE-024
    #[xmtp_common::test(unwrap_try = true)]
    async fn compress_round_trip() {
        let original = TextCodec::encode("hello compressed XMTP".into())?;
        assert_eq!(compress_if_requested(original.clone(), None)?, original);
        for (algorithm, magic) in [
            (Compression::Deflate, &[0x78][..]),
            (Compression::Gzip, &[0x1f, 0x8b][..]),
        ] {
            let compressed = compress_if_requested(original.clone(), Some(algorithm))?;
            assert_eq!(compressed.compression, Some(algorithm as i32));
            assert!(compressed.content.starts_with(magic));
            assert_eq!(decompress(compressed)?, original);
        }
    }
}
