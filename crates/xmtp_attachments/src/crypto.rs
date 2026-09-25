use aes::Aes256;
use aes::cipher::{BlockEncrypt, KeyInit as _, StreamCipher, generic_array::GenericArray};
use ctr::cipher::KeyIvInit as _;
use ghash::GHash;
use ghash::universal_hash::UniversalHash as _;
use subtle::ConstantTimeEq as _;
use xmtp_content_types::{encryption::derive_key, remote_attachment::RemoteAttachment};

use crate::{AttachmentError, AttachmentFailureCause};

const TAG_LEN: usize = 16;
const BLOCK_LEN: usize = 16;

/// The random values used by the attachment encryption scheme.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyMaterial {
    pub secret: [u8; 32],
    pub salt: [u8; 32],
    pub nonce: [u8; 12],
}

impl KeyMaterial {
    pub fn random() -> Self {
        Self {
            secret: xmtp_common::rand_array(),
            salt: xmtp_common::rand_array(),
            nonce: xmtp_common::rand_array(),
        }
    }

    pub fn from_remote(ra: &RemoteAttachment) -> Result<Self, AttachmentError> {
        if ra.content_digest.len() != 64
            || !ra
                .content_digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(AttachmentError::new(AttachmentFailureCause::Malformed));
        }
        Ok(Self {
            secret: ra
                .secret
                .as_slice()
                .try_into()
                .map_err(|_| AttachmentError::new(AttachmentFailureCause::Malformed))?,
            salt: ra
                .salt
                .as_slice()
                .try_into()
                .map_err(|_| AttachmentError::new(AttachmentFailureCause::Malformed))?,
            nonce: ra
                .nonce
                .as_slice()
                .try_into()
                .map_err(|_| AttachmentError::new(AttachmentFailureCause::Malformed))?,
        })
    }
}

struct GcmCore {
    cipher: ctr::Ctr32BE<Aes256>,
    hash: GHash,
    partial: [u8; BLOCK_LEN],
    partial_len: usize,
    ciphertext_len: u64,
    tag_mask: [u8; TAG_LEN],
}

impl GcmCore {
    fn new(material: &KeyMaterial) -> Self {
        // HKDF expansion to 32 bytes cannot fail with SHA-256.
        let key =
            derive_key(&material.secret, &material.salt).expect("32-byte HKDF expansion is valid");
        let aes = Aes256::new((&key).into());
        let mut h = GenericArray::default();
        aes.encrypt_block(&mut h);
        let hash = GHash::new(&h);
        let mut j0 = [0u8; 16];
        j0[..12].copy_from_slice(&material.nonce);
        j0[15] = 1;
        let mut tag_mask = GenericArray::clone_from_slice(&j0);
        aes.encrypt_block(&mut tag_mask);
        let mut counter = j0;
        counter[15] = 2;
        Self {
            cipher: ctr::Ctr32BE::<Aes256>::new((&key).into(), (&counter).into()),
            hash,
            partial: [0; BLOCK_LEN],
            partial_len: 0,
            ciphertext_len: 0,
            tag_mask: tag_mask.into(),
        }
    }

    fn authenticate(&mut self, mut input: &[u8]) {
        self.ciphertext_len += input.len() as u64;
        if self.partial_len != 0 {
            let take = (BLOCK_LEN - self.partial_len).min(input.len());
            self.partial[self.partial_len..self.partial_len + take].copy_from_slice(&input[..take]);
            self.partial_len += take;
            input = &input[take..];
            if self.partial_len == BLOCK_LEN {
                self.hash.update(&[self.partial.into()]);
                self.partial_len = 0;
            } else {
                return;
            }
        }
        let blocks = input.len() / BLOCK_LEN;
        for block in input[..blocks * BLOCK_LEN].chunks_exact(BLOCK_LEN) {
            self.hash.update(&[GenericArray::clone_from_slice(block)]);
        }
        let tail = &input[blocks * BLOCK_LEN..];
        self.partial[..tail.len()].copy_from_slice(tail);
        self.partial_len = tail.len();
    }

    fn tag(mut self) -> [u8; TAG_LEN] {
        if self.partial_len != 0 {
            self.partial[self.partial_len..].fill(0);
            self.hash.update(&[self.partial.into()]);
        }
        let mut lengths = [0u8; BLOCK_LEN];
        lengths[8..].copy_from_slice(&(self.ciphertext_len * 8).to_be_bytes());
        self.hash.update(&[lengths.into()]);
        let digest = self.hash.finalize();
        let mut tag = self.tag_mask;
        for (byte, hash_byte) in tag.iter_mut().zip(digest.iter()) {
            *byte ^= hash_byte;
        }
        tag
    }
}

/// Encrypts chunks and returns the tag after the final chunk.
pub struct GcmEncryptor(GcmCore);

impl GcmEncryptor {
    pub fn new(material: &KeyMaterial) -> Self {
        Self(GcmCore::new(material))
    }

    pub fn update(&mut self, input: &[u8], out: &mut Vec<u8>) {
        let start = out.len();
        out.extend_from_slice(input);
        self.0.cipher.apply_keystream(&mut out[start..]);
        self.0.authenticate(&out[start..]);
    }

    pub fn finish(self) -> [u8; TAG_LEN] {
        self.0.tag()
    }
}

/// Decrypts chunks while retaining the final 16 bytes as the tag.
///
/// `update` releases unauthenticated plaintext. The caller must write it to a
/// temporary file and make it readable only after `finish` succeeds.
pub struct GcmDecryptor {
    core: GcmCore,
    tail: Vec<u8>,
}

impl GcmDecryptor {
    pub fn new(material: &KeyMaterial) -> Self {
        Self {
            core: GcmCore::new(material),
            tail: Vec::with_capacity(TAG_LEN),
        }
    }

    pub fn update(&mut self, input: &[u8], out: &mut Vec<u8>) {
        let release = self
            .tail
            .len()
            .saturating_add(input.len())
            .saturating_sub(TAG_LEN);
        let from_tail = release.min(self.tail.len());
        if from_tail != 0 {
            let old = self.tail.drain(..from_tail).collect::<Vec<_>>();
            self.decrypt_part(&old, out);
        }
        let from_input = release - from_tail;
        self.decrypt_part(&input[..from_input], out);
        self.tail.extend_from_slice(&input[from_input..]);
    }

    fn decrypt_part(&mut self, ciphertext: &[u8], out: &mut Vec<u8>) {
        self.core.authenticate(ciphertext);
        let start = out.len();
        out.extend_from_slice(ciphertext);
        self.core.cipher.apply_keystream(&mut out[start..]);
    }

    pub fn finish(self) -> Result<(), AttachmentError> {
        let Self { core, tail } = self;
        if tail.len() != TAG_LEN || !bool::from(core.tag().as_slice().ct_eq(tail.as_slice())) {
            return Err(AttachmentError::new(
                AttachmentFailureCause::DecryptionFailed,
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use aes_gcm::{Aes256Gcm, KeyInit as _, aead::Aead as _};

    use super::*;

    // verifies: ATCH-012
    #[xmtp_common::test(unwrap_try = true)]
    async fn gcm_stream_matches_one_shot() {
        let material = KeyMaterial {
            secret: [7; 32],
            salt: [9; 32],
            nonce: [11; 12],
        };
        let key = derive_key(&material.secret, &material.salt)?;
        let cipher = Aes256Gcm::new((&key).into());
        // The native run checks every required length. Wasm runs a shorter
        // smoke pass because debug AES in the browser is much slower.
        let max_len = if cfg!(target_arch = "wasm32") {
            1024
        } else {
            70_000
        };
        let mut plaintext = vec![0u8; max_len];
        let mut seed = 0x91c3_7d52_u64;
        for byte in &mut plaintext {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            *byte = seed as u8;
        }
        for len in 0..=max_len {
            let expected = cipher
                .encrypt((&material.nonce).into(), &plaintext[..len])
                .expect("fixed key and nonce");
            let mut encryptor = GcmEncryptor::new(&material);
            let mut actual = Vec::new();
            let mut at = 0;
            while at < len {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                let end = (at + 1 + (seed as usize % 4096)).min(len);
                encryptor.update(&plaintext[at..end], &mut actual);
                at = end;
            }
            actual.extend_from_slice(&encryptor.finish());
            assert_eq!(actual, expected, "length {len}");

            let mut decryptor = GcmDecryptor::new(&material);
            let mut decoded = Vec::new();
            let mut at = 0;
            while at < actual.len() {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                let end = (at + 1 + (seed as usize % 3072)).min(actual.len());
                decryptor.update(&actual[at..end], &mut decoded);
                at = end;
            }
            decryptor.finish()?;
            assert_eq!(decoded, plaintext[..len], "length {len}");

            if [0, 1, 15, 16, 17, 1024, 70_000].contains(&len) {
                let mut positions = vec![len, actual.len() - 1];
                if len != 0 {
                    positions.extend([0, len - 1]);
                }
                for pos in positions {
                    for bit in 0..8 {
                        let mut changed = actual.clone();
                        changed[pos] ^= 1 << bit;
                        let mut decryptor = GcmDecryptor::new(&material);
                        let mut out = Vec::new();
                        for chunk in changed.chunks(13) {
                            decryptor.update(chunk, &mut out);
                        }
                        assert!(matches!(
                            decryptor.finish(),
                            Err(AttachmentError {
                                cause: AttachmentFailureCause::DecryptionFailed
                            })
                        ));
                    }
                }
            }
        }
    }
}
