//! Native file encryption compatible with one-tag AES-256-GCM attachments.

use std::{
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
};

use aes::{
    Aes256,
    cipher::{Block, BlockEncrypt, KeyInit, KeyIvInit, StreamCipher},
};
use ctr::Ctr32BE;
use ghash::{GHash, universal_hash::UniversalHash};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

use crate::{
    CodecError,
    encryption::{
        self, AES_GCM_NONCE_SIZE, AES_GCM_TAG_SIZE, EncryptionKeys, HKDF_SALT_SIZE, SECRET_SIZE,
    },
};

const FILE_CHUNK_BYTES: usize = 64 * 1024;
// NIST SP 800-38D limits one GCM plaintext to 2^36 - 32 bytes.
const MAX_GCM_BYTES: u64 = (1u64 << 36) - 32;

struct GcmStream {
    ctr: Ctr32BE<Aes256>,
    ghash: GHash,
    mask: Block<Aes256>,
}

impl GcmStream {
    fn new(secret: &[u8], salt: &[u8], nonce: &[u8]) -> Result<Self, CodecError> {
        if salt.len() != HKDF_SALT_SIZE || nonce.len() != AES_GCM_NONCE_SIZE {
            return Err(CodecError::Decode(
                "invalid file encryption salt or nonce".into(),
            ));
        }
        let key = encryption::derive_key(secret, salt).map_err(CodecError::Decode)?;
        let aes = Aes256::new_from_slice(&key)
            .map_err(|e| CodecError::Decode(format!("invalid AES key: {e}")))?;
        let mut h = Block::<Aes256>::default();
        aes.encrypt_block(&mut h);
        let ghash = GHash::new(ghash::Key::from_slice(&h));
        let mut mask = Block::<Aes256>::default();
        mask[..AES_GCM_NONCE_SIZE].copy_from_slice(nonce);
        mask[15] = 1;
        aes.encrypt_block(&mut mask);
        let mut iv = Block::<Aes256>::default();
        iv[..AES_GCM_NONCE_SIZE].copy_from_slice(nonce);
        iv[15] = 2;
        let ctr = Ctr32BE::<Aes256>::new((&key).into(), &iv);
        Ok(Self { ctr, ghash, mask })
    }

    fn encrypt_chunk(&mut self, chunk: &mut [u8]) {
        self.ctr.apply_keystream(chunk);
        self.ghash.update_padded(chunk);
    }

    fn decrypt_chunk(&mut self, chunk: &mut [u8]) {
        self.ghash.update_padded(chunk);
        self.ctr.apply_keystream(chunk);
    }

    fn finish(mut self, ciphertext_len: u64) -> [u8; AES_GCM_TAG_SIZE] {
        let mut lengths = ghash::Block::default();
        lengths[8..].copy_from_slice(&(ciphertext_len * 8).to_be_bytes());
        self.ghash.update(&[lengths]);
        let mut tag = [0u8; AES_GCM_TAG_SIZE];
        tag.copy_from_slice(&self.ghash.finalize());
        for (byte, mask) in tag.iter_mut().zip(self.mask.iter()) {
            *byte ^= mask;
        }
        tag
    }
}

fn output_parent(output: &Path) -> PathBuf {
    output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf()
}

fn encrypt_stream(
    input: &mut impl Read,
    output: &mut impl Write,
    secret: &[u8],
    salt: &[u8],
    nonce: &[u8],
) -> Result<(String, u64), CodecError> {
    let mut gcm = GcmStream::new(secret, salt, nonce)?;
    let mut digest = Sha256::new();
    let mut chunk = [0u8; FILE_CHUNK_BYTES];
    let mut length = 0u64;
    loop {
        let mut read = 0;
        while read < FILE_CHUNK_BYTES {
            let count = input
                .read(&mut chunk[read..])
                .map_err(|e| CodecError::Encode(e.to_string()))?;
            if count == 0 {
                break;
            }
            read += count;
        }
        if read == 0 {
            break;
        }
        length = length
            .checked_add(read as u64)
            .filter(|value| *value <= MAX_GCM_BYTES)
            .ok_or_else(|| CodecError::Encode("file exceeds AES-GCM size limit".into()))?;
        gcm.encrypt_chunk(&mut chunk[..read]);
        output
            .write_all(&chunk[..read])
            .map_err(|e| CodecError::Encode(e.to_string()))?;
        digest.update(&chunk[..read]);
    }
    let tag = gcm.finish(length);
    output
        .write_all(&tag)
        .map_err(|e| CodecError::Encode(e.to_string()))?;
    digest.update(tag);
    Ok((
        hex::encode(digest.finalize()),
        length + AES_GCM_TAG_SIZE as u64,
    ))
}

/// Encrypts a file in 64 KiB chunks and writes one attachment-compatible tag.
pub fn encrypt_file(input: &Path, output: &Path) -> Result<EncryptionKeys, CodecError> {
    let mut source = File::open(input).map_err(|e| CodecError::Encode(e.to_string()))?;
    let mut target = tempfile::NamedTempFile::new_in(output_parent(output))
        .map_err(|e| CodecError::Encode(e.to_string()))?;
    let secret: [u8; SECRET_SIZE] = xmtp_common::rand_array();
    let salt: [u8; HKDF_SALT_SIZE] = xmtp_common::rand_array();
    let nonce: [u8; AES_GCM_NONCE_SIZE] = xmtp_common::rand_array();
    let (digest, length) = encrypt_stream(&mut source, &mut target, &secret, &salt, &nonce)?;
    target
        .persist(output)
        .map_err(|e| CodecError::Encode(e.to_string()))?;
    Ok(EncryptionKeys {
        secret: secret.to_vec(),
        salt: salt.to_vec(),
        nonce: nonce.to_vec(),
        digest,
        length,
    })
}

/// Verifies the digest and tag before moving the plaintext to `output`.
pub fn decrypt_file(input: &Path, output: &Path, keys: &EncryptionKeys) -> Result<(), CodecError> {
    let mut source = File::open(input).map_err(|e| CodecError::Decode(e.to_string()))?;
    let total_len = source
        .metadata()
        .map_err(|e| CodecError::Decode(e.to_string()))?
        .len();
    if total_len != keys.length
        || total_len < AES_GCM_TAG_SIZE as u64
        || total_len - AES_GCM_TAG_SIZE as u64 > MAX_GCM_BYTES
    {
        return Err(CodecError::Decode("invalid encrypted file length".into()));
    }
    let mut target = tempfile::NamedTempFile::new_in(output_parent(output))
        .map_err(|e| CodecError::Decode(e.to_string()))?;
    let mut gcm = GcmStream::new(&keys.secret, &keys.salt, &keys.nonce)?;
    let mut digest = Sha256::new();
    let mut remaining = total_len - AES_GCM_TAG_SIZE as u64;
    let mut chunk = [0u8; FILE_CHUNK_BYTES];
    while remaining > 0 {
        let size = remaining.min(FILE_CHUNK_BYTES as u64) as usize;
        source
            .read_exact(&mut chunk[..size])
            .map_err(|e| CodecError::Decode(e.to_string()))?;
        digest.update(&chunk[..size]);
        gcm.decrypt_chunk(&mut chunk[..size]);
        target
            .write_all(&chunk[..size])
            .map_err(|e| CodecError::Decode(e.to_string()))?;
        remaining -= size as u64;
    }
    let mut tag = [0u8; AES_GCM_TAG_SIZE];
    source
        .read_exact(&mut tag)
        .map_err(|e| CodecError::Decode(e.to_string()))?;
    digest.update(tag);
    if hex::encode(digest.finalize()) != keys.digest {
        return Err(CodecError::Decode("content digest mismatch".into()));
    }
    let expected_tag = gcm.finish(total_len - AES_GCM_TAG_SIZE as u64);
    if !bool::from(expected_tag.ct_eq(&tag)) {
        return Err(CodecError::Decode("AES-GCM authentication failed".into()));
    }
    target
        .persist(output)
        .map_err(|e| CodecError::Decode(e.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ContentCodec,
        attachment::{Attachment, AttachmentCodec},
        remote_attachment::{RemoteAttachment, decrypt_attachment},
    };
    use prost::Message;

    // verifies: CTYPE-015
    #[xmtp_common::test(unwrap_try = true)]
    async fn decrypt_file_wrong_key_leaves_no_output() {
        let dir = tempfile::tempdir()?;
        let source = dir.path().join("plain");
        let encrypted = dir.path().join("encrypted");
        let output = dir.path().join("decrypted");
        std::fs::write(&source, b"secret file")?;
        let mut keys = encrypt_file(&source, &encrypted)?;
        keys.secret[0] ^= 1;
        assert!(decrypt_file(&encrypted, &output, &keys).is_err());
        assert!(!output.exists());
        assert_eq!(std::fs::read_dir(dir.path())?.count(), 2);
        let mut keys = encrypt_file(&source, &encrypted)?;
        keys.digest = "bad digest".into();
        assert!(decrypt_file(&encrypted, &output, &keys).is_err());
        assert!(!output.exists());
    }

    struct BoundedReader<'a> {
        bytes: &'a [u8],
        largest: usize,
    }
    impl Read for BoundedReader<'_> {
        fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
            self.largest = self.largest.max(output.len());
            self.bytes.read(output)
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn file_encryption_reads_bounded_chunks() {
        let bytes = vec![42u8; FILE_CHUNK_BYTES * 2 + 19];
        let mut reader = BoundedReader {
            bytes: &bytes,
            largest: 0,
        };
        let secret: [u8; SECRET_SIZE] = xmtp_common::rand_array();
        let salt: [u8; HKDF_SALT_SIZE] = xmtp_common::rand_array();
        let nonce: [u8; AES_GCM_NONCE_SIZE] = xmtp_common::rand_array();
        let mut ciphertext = Vec::new();
        let (digest, length) =
            encrypt_stream(&mut reader, &mut ciphertext, &secret, &salt, &nonce)?;
        assert_eq!(reader.largest, FILE_CHUNK_BYTES);
        let keys = EncryptionKeys {
            secret: secret.to_vec(),
            salt: salt.to_vec(),
            nonce: nonce.to_vec(),
            digest,
            length,
        };
        assert_eq!(encryption::decrypt_bytes(&ciphertext, &keys)?, bytes);

        let dir = tempfile::tempdir()?;
        let source = dir.path().join("plain");
        let encrypted = dir.path().join("encrypted");
        let output = dir.path().join("decrypted");
        let content = AttachmentCodec::encode(Attachment {
            filename: None,
            mime_type: "application/octet-stream".into(),
            content: bytes,
        })?;
        std::fs::write(&source, content.encode_to_vec())?;
        let keys = encrypt_file(&source, &encrypted)?;
        decrypt_file(&encrypted, &output, &keys)?;
        assert_eq!(std::fs::read(&source)?, std::fs::read(&output)?);
        let remote = RemoteAttachment {
            filename: None,
            content_length: Some(keys.length as u32),
            url: String::new(),
            content_digest: keys.digest,
            secret: keys.secret,
            salt: keys.salt,
            nonce: keys.nonce,
            scheme: String::new(),
        };
        assert_eq!(
            decrypt_attachment(&std::fs::read(encrypted)?, &remote)?
                .content
                .len(),
            FILE_CHUNK_BYTES * 2 + 19
        );
    }
}
