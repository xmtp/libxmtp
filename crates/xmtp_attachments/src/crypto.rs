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
/// GCM starts CTR at block 2, leaving at most 2^32 - 2 data blocks.
const MAX_GCM_INPUT_BYTES: u64 = (u32::MAX as u64 - 1) * BLOCK_LEN as u64;

/// The random values used by the attachment encryption scheme.
#[derive(Clone, PartialEq, Eq)]
pub struct KeyMaterial {
    pub secret: [u8; 32],
    pub salt: [u8; 32],
    pub nonce: [u8; 12],
}

impl std::fmt::Debug for KeyMaterial {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("KeyMaterial { .. }")
    }
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

    fn check_input_len(&self, input_len: usize) -> Result<(), AttachmentError> {
        let additional = u64::try_from(input_len)
            .map_err(|_| AttachmentError::new(AttachmentFailureCause::TooLarge))?;
        let remaining = MAX_GCM_INPUT_BYTES
            .checked_sub(self.ciphertext_len)
            .ok_or_else(|| AttachmentError::new(AttachmentFailureCause::TooLarge))?;
        if additional > remaining {
            return Err(AttachmentError::new(AttachmentFailureCause::TooLarge));
        }
        Ok(())
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
        self.hash.update_padded(&input[..blocks * BLOCK_LEN]);
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

    /// Returns `too_large` without changing `out` when the GCM counter limit is exceeded.
    pub fn update(&mut self, input: &[u8], out: &mut Vec<u8>) -> Result<(), AttachmentError> {
        self.0.check_input_len(input.len())?;
        let start = out.len();
        out.extend_from_slice(input);
        self.0.cipher.apply_keystream(&mut out[start..]);
        self.0.authenticate(&out[start..]);
        Ok(())
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

    /// Returns `too_large` without changing `out` when the GCM counter limit is exceeded.
    pub fn update(&mut self, input: &[u8], out: &mut Vec<u8>) -> Result<(), AttachmentError> {
        let release = input.len().saturating_sub(TAG_LEN - self.tail.len());
        self.core.check_input_len(release)?;
        let from_tail = release.min(self.tail.len());
        if from_tail != 0 {
            let old = self.tail.drain(..from_tail).collect::<Vec<_>>();
            self.decrypt_part(&old, out);
        }
        let from_input = release - from_tail;
        self.decrypt_part(&input[..from_input], out);
        self.tail.extend_from_slice(&input[from_input..]);
        Ok(())
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

    #[xmtp_common::test(unwrap_try = true)]
    fn key_material_debug_hides_secrets() {
        let material = KeyMaterial {
            secret: [0xab; 32],
            salt: [0xcd; 32],
            nonce: [0xef; 12],
        };
        let debug = format!("{material:?}");
        assert_eq!(debug, "KeyMaterial { .. }");
        assert!(!debug.contains(&hex::encode(material.secret)));
    }

    fn check_lengths(material: &KeyMaterial, plaintext: &[u8], start: usize, end: usize) {
        let key =
            derive_key(&material.secret, &material.salt).expect("32-byte HKDF expansion is valid");
        let cipher = Aes256Gcm::new((&key).into());
        let mut seed = 0x91c3_7d52_u64 ^ start as u64;
        for len in start..=end {
            let expected = cipher
                .encrypt((&material.nonce).into(), &plaintext[..len])
                .expect("fixed key and nonce");
            let mut encryptor = GcmEncryptor::new(material);
            let mut actual = Vec::new();
            let mut at = 0;
            while at < len {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                let end = (at + 1 + (seed as usize % 32_768)).min(len);
                encryptor
                    .update(&plaintext[at..end], &mut actual)
                    .expect("plaintext length is below the GCM limit");
                at = end;
            }
            actual.extend_from_slice(&encryptor.finish());
            assert_eq!(actual, expected, "length {len}");

            let mut decryptor = GcmDecryptor::new(material);
            let mut decoded = Vec::new();
            let mut at = 0;
            while at < actual.len() {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                let end = (at + 1 + (seed as usize % 24_576)).min(actual.len());
                decryptor
                    .update(&actual[at..end], &mut decoded)
                    .expect("ciphertext length is below the GCM limit");
                at = end;
            }
            decryptor.finish().expect("valid ciphertext and tag");
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
                        let mut decryptor = GcmDecryptor::new(material);
                        let mut out = Vec::new();
                        decryptor
                            .update(&changed, &mut out)
                            .expect("ciphertext length is below the GCM limit");
                        assert!(matches!(
                            decryptor.finish(),
                            Err(AttachmentError {
                                cause: AttachmentFailureCause::DecryptionFailed,
                                ..
                            })
                        ));
                    }
                }
            }
        }
    }

    // verifies: ATCH-012
    #[xmtp_common::test(unwrap_try = true)]
    async fn gcm_stream_matches_one_shot() {
        let material = KeyMaterial {
            secret: [7; 32],
            salt: [9; 32],
            nonce: [11; 12],
        };
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
        #[cfg(target_arch = "wasm32")]
        check_lengths(&material, &plaintext, 0, max_len);
        #[cfg(not(target_arch = "wasm32"))]
        std::thread::scope(|scope| {
            const WORKERS: usize = 10;
            let mut handles = Vec::with_capacity(WORKERS);
            // Total work is proportional to the sum of plaintext lengths.
            let boundary = |worker: usize| {
                ((max_len + 1) as f64 * (worker as f64 / WORKERS as f64).sqrt()) as usize
            };
            for worker in 0..WORKERS {
                let start = boundary(worker);
                let end = boundary(worker + 1) - 1;
                let material = &material;
                let plaintext = &plaintext;
                handles.push(scope.spawn(move || check_lengths(material, plaintext, start, end)));
            }
            for handle in handles {
                handle.join().expect("GCM worker completed");
            }
        });
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn gcm_counter_limit_rejects_before_output() {
        let material = KeyMaterial {
            secret: [7; 32],
            salt: [9; 32],
            nonce: [11; 12],
        };

        let mut encryptor = GcmEncryptor::new(&material);
        encryptor.0.ciphertext_len = MAX_GCM_INPUT_BYTES - 1;
        let mut encrypted = vec![0x5a];
        assert_eq!(
            encryptor.update(&[1, 2], &mut encrypted).unwrap_err().cause,
            AttachmentFailureCause::TooLarge
        );
        assert_eq!(encrypted, [0x5a]);
        assert_eq!(encryptor.0.ciphertext_len, MAX_GCM_INPUT_BYTES - 1);
        encryptor.update(&[1], &mut encrypted)?;
        assert_eq!(encrypted.len(), 2);
        assert_eq!(encryptor.0.ciphertext_len, MAX_GCM_INPUT_BYTES);
        assert_eq!(
            encryptor.update(&[2], &mut encrypted).unwrap_err().cause,
            AttachmentFailureCause::TooLarge
        );
        assert_eq!(encrypted.len(), 2);

        let mut decryptor = GcmDecryptor::new(&material);
        decryptor.core.ciphertext_len = MAX_GCM_INPUT_BYTES - 1;
        decryptor.tail = vec![0x3c; TAG_LEN];
        let mut decrypted = vec![0x5a];
        assert_eq!(
            decryptor.update(&[1, 2], &mut decrypted).unwrap_err().cause,
            AttachmentFailureCause::TooLarge
        );
        assert_eq!(decrypted, [0x5a]);
        assert_eq!(decryptor.tail, [0x3c; TAG_LEN]);
        assert_eq!(decryptor.core.ciphertext_len, MAX_GCM_INPUT_BYTES - 1);
        decryptor.update(&[1], &mut decrypted)?;
        assert_eq!(decrypted.len(), 2);
        assert_eq!(decryptor.core.ciphertext_len, MAX_GCM_INPUT_BYTES);
        assert_eq!(
            decryptor.update(&[2], &mut decrypted).unwrap_err().cause,
            AttachmentFailureCause::TooLarge
        );
        assert_eq!(decrypted.len(), 2);
    }
}
