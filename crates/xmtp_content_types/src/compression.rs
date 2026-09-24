//! Compression for the bytes inside an `EncodedContent` envelope.

use std::io::{self, Read, Write};

use flate2::{
    Compression as FlateCompression,
    read::{DeflateDecoder, GzDecoder, ZlibDecoder},
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

fn read_bounded(reader: &mut impl Read, peak: &mut usize) -> Result<Vec<u8>, DecompressFailure> {
    let mut output = Vec::new();
    let mut chunk = [0u8; COMPRESSION_CHUNK_BYTES];
    loop {
        let remaining = MAX_DECOMPRESSED_BYTES - output.len();
        let request = remaining.saturating_add(1).min(COMPRESSION_CHUNK_BYTES);
        let read = reader
            .read(&mut chunk[..request])
            .map_err(DecompressFailure::Invalid)?;
        if read == 0 {
            return Ok(output);
        }
        if read > remaining {
            return Err(DecompressFailure::Limit);
        }
        output.extend_from_slice(&chunk[..read]);
        *peak = (*peak).max(output.capacity());
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
pub fn decompress(mut content: EncodedContent) -> Result<EncodedContent, CodecError> {
    let Some(raw_algorithm) = content.compression else {
        return Ok(content);
    };
    let algorithm = Compression::try_from(raw_algorithm)
        .map_err(|_| CodecError::Decode(format!("unknown compression value {raw_algorithm}")))?;
    let mut peak = 0;
    let result = match algorithm {
        Compression::Gzip => {
            read_bounded(&mut GzDecoder::new(content.content.as_slice()), &mut peak)
        }
        Compression::Deflate => {
            match read_bounded(&mut ZlibDecoder::new(content.content.as_slice()), &mut peak) {
                Ok(bytes) => Ok(bytes),
                Err(DecompressFailure::Invalid(_)) => read_bounded(
                    &mut DeflateDecoder::new(content.content.as_slice()),
                    &mut peak,
                ),
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
        let mut peak = 0;
        let result = read_bounded(&mut ZlibDecoder::new(compressed.as_slice()), &mut peak);
        assert!(matches!(result, Err(DecompressFailure::Limit)));
        assert!(peak <= MAX_DECOMPRESSED_BYTES + COMPRESSION_CHUNK_BYTES);
        let mut content = TextCodec::encode(String::new())?;
        content.content = compressed;
        content.compression = Some(Compression::Deflate as i32);
        assert!(matches!(decompress(content), Err(CodecError::Decode(_))));
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
