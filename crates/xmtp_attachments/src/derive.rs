use sha2::{Digest as _, Sha256};
use xmtp_content_types::remote_attachment::RemoteAttachment;

use crate::{AttachmentError, KeyMaterial, local_file_name};

/// Append a ciphertext digest to the configured base URL.
pub fn download_url(base_url: &str, content_digest_hex: &str) -> String {
    format!("{base_url}/{content_digest_hex}")
}

/// Build the metadata published for a staged ciphertext.
pub fn remote_attachment(
    base_url: &str,
    content_digest_hex: &str,
    material: &KeyMaterial,
    content_length: u32,
    filename: Option<&str>,
) -> RemoteAttachment {
    let url = download_url(base_url, content_digest_hex);
    let scheme = url
        .split_once("://")
        .map_or(String::new(), |(scheme, _)| format!("{scheme}://"));
    RemoteAttachment {
        url,
        content_digest: content_digest_hex.to_owned(),
        secret: material.secret.to_vec(),
        salt: material.salt.to_vec(),
        nonce: material.nonce.to_vec(),
        content_length: Some(content_length),
        filename: filename.map(str::to_owned),
        scheme,
    }
}

/// Hash all key material to name the local attachment directory.
pub fn attachment_key(ra: &RemoteAttachment) -> Result<String, AttachmentError> {
    let material = KeyMaterial::from_remote(ra)?;
    let mut digest = [0u8; 32];
    hex::decode_to_slice(&ra.content_digest, &mut digest)
        .map_err(|_| AttachmentError::new(crate::AttachmentFailureCause::Malformed))?;
    let mut hash = Sha256::new();
    hash.update(digest);
    hash.update(material.secret);
    hash.update(material.salt);
    hash.update(material.nonce);
    Ok(hex::encode(hash.finalize()))
}

/// Derive the relative plaintext path without reading storage or a network.
pub fn plaintext_rel_path(ra: &RemoteAttachment) -> Result<String, AttachmentError> {
    Ok(format!(
        "{}/{}",
        attachment_key(ra)?,
        local_file_name(ra.filename.as_deref())
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AttachmentFailureCause;

    fn material() -> KeyMaterial {
        KeyMaterial {
            secret: std::array::from_fn(|index| index as u8 + 32),
            salt: std::array::from_fn(|index| index as u8 + 64),
            nonce: std::array::from_fn(|index| index as u8 + 96),
        }
    }

    fn remote() -> RemoteAttachment {
        remote_attachment(
            "https://attachments.example.test/objects",
            &hex::encode((0u8..32).collect::<Vec<_>>()),
            &material(),
            1234,
            Some("report.pdf"),
        )
    }

    // verifies: ATCH-010
    #[xmtp_common::test(unwrap_try = true)]
    async fn download_url_form() {
        assert_eq!(
            download_url("https://example.test/objects", "abc123"),
            "https://example.test/objects/abc123"
        );
    }

    // verifies: ATCH-011
    #[xmtp_common::test(unwrap_try = true)]
    async fn remote_attachment_fields() {
        let ra = remote();
        assert_eq!(
            ra.url,
            format!(
                "https://attachments.example.test/objects/{}",
                ra.content_digest
            )
        );
        assert_eq!(ra.scheme, "https://");
        assert_eq!(ra.content_length, Some(1234));
        assert_eq!(ra.filename.as_deref(), Some("report.pdf"));
        assert_eq!(ra.secret, material().secret);
        assert_eq!(ra.salt, material().salt);
        assert_eq!(ra.nonce, material().nonce);
        assert_eq!(KeyMaterial::from_remote(&ra)?, material());
    }

    // This digest was computed separately with Python hashlib from bytes 0..108.
    // verifies: ATCH-041
    #[xmtp_common::test(unwrap_try = true)]
    async fn attachment_key_vector() {
        let ra = remote();
        let expected = "44d21db70716bd7644cb0d819fa6791805ebc526ea32996a60e41dc753fcfafc";
        assert_eq!(attachment_key(&ra)?, expected);
        assert_eq!(plaintext_rel_path(&ra)?, format!("{expected}/report.pdf"));
    }

    // verifies: ATCH-059
    #[xmtp_common::test(unwrap_try = true)]
    async fn malformed_key_material() {
        let valid = remote();
        let mut bad_cases = Vec::new();
        let mut bad = valid.clone();
        bad.content_digest.pop();
        bad_cases.push(bad);
        let mut bad = valid.clone();
        bad.content_digest.replace_range(..1, "A");
        bad_cases.push(bad);
        let mut bad = valid.clone();
        bad.secret.pop();
        bad_cases.push(bad);
        let mut bad = valid.clone();
        bad.salt.pop();
        bad_cases.push(bad);
        let mut bad = valid;
        bad.nonce.pop();
        bad_cases.push(bad);
        for ra in bad_cases {
            assert_eq!(
                KeyMaterial::from_remote(&ra).unwrap_err().cause,
                AttachmentFailureCause::Malformed
            );
            assert_eq!(
                attachment_key(&ra).unwrap_err().cause,
                AttachmentFailureCause::Malformed
            );
            assert_eq!(
                plaintext_rel_path(&ra).unwrap_err().cause,
                AttachmentFailureCause::Malformed
            );
        }
    }
}
