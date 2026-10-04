use super::*;

fn secret() -> Vec<u8> {
    (101..133).collect()
}

fn remote_attachment() -> RemoteAttachment {
    RemoteAttachment {
        url: "https://example.invalid/attachment".into(),
        content_digest: "digest".into(),
        secret: secret(),
        salt: vec![2; 32],
        nonce: vec![3; 12],
        scheme: "https".into(),
        content_length: Some(64),
        filename: Some("attachment.txt".into()),
    }
}

fn assert_redacted(diagnostic: String) {
    assert!(
        !diagnostic.contains(&format!("{:?}", secret())),
        "secret in {diagnostic}"
    );
    assert!(diagnostic.contains("<redacted>"));
}

// verifies: LOG-010
#[xmtp_common::test(unwrap_try = true)]
fn attachment_diagnostics_hide_sdk_remote_secret() {
    let remote = remote_attachment();
    assert_redacted(format!("{remote:?}"));
    assert_eq!(remote.secret, secret());
    assert_eq!(remote.content_length, Some(64));
    assert!(format!("{remote:?}").contains("attachment.txt"));
}

// verifies: LOG-010
#[xmtp_common::test(unwrap_try = true)]
fn attachment_diagnostics_hide_sdk_nested_secret() {
    let remote = remote_attachment();
    let multi = MultiRemoteAttachment {
        attachments: vec![remote.clone()],
    };
    assert_redacted(format!("{multi:?}"));
    assert_redacted(format!(
        "{:?}",
        StandardContent::RemoteAttachment(remote.clone())
    ));
    assert_redacted(format!(
        "{:?}",
        StandardContent::MultiRemoteAttachment(multi.clone())
    ));
    #[cfg(not(feature = "pure-only"))]
    {
        assert_redacted(format!(
            "{:?}",
            crate::MessageContent::RemoteAttachment(remote.clone())
        ));
        assert_redacted(format!(
            "{:?}",
            crate::MessageContent::MultiRemoteAttachment(multi.clone())
        ));
        assert_redacted(format!(
            "{:?}",
            crate::MessageBody::RemoteAttachment(remote.clone())
        ));
        assert_redacted(format!(
            "{:?}",
            crate::MessageBody::MultiRemoteAttachment(multi)
        ));
    }
    assert_eq!(remote.secret, secret());
}

// verifies: LOG-010
#[xmtp_common::test(unwrap_try = true)]
fn attachment_diagnostics_hide_core_keys_secret() {
    use xmtp_content_types::encryption::{EncryptedEncodedContent, EncryptionKeys};
    let keys = EncryptionKeys {
        secret: secret(),
        salt: vec![2; 32],
        nonce: vec![3; 12],
        digest: "digest".into(),
        length: 64,
    };
    assert_redacted(format!("{keys:?}"));
    assert_redacted(format!(
        "{:?}",
        EncryptedEncodedContent {
            ciphertext: vec![4; 64],
            keys: keys.clone()
        }
    ));
    assert_eq!(keys.secret, secret());
    assert_eq!(keys.length, 64);
}

// verifies: LOG-010
#[xmtp_common::test(unwrap_try = true)]
fn attachment_diagnostics_hide_core_encrypted_attachment_secret() {
    use xmtp_content_types::remote_attachment::EncryptedAttachment;
    let encrypted = EncryptedAttachment {
        payload: vec![4; 64],
        content_digest: "digest".into(),
        secret: secret(),
        salt: vec![2; 32],
        nonce: vec![3; 12],
        content_length: 64,
        filename: Some("attachment.txt".into()),
    };
    assert_redacted(format!("{encrypted:?}"));
    assert_redacted(format!("{:?}", vec![encrypted.clone()]));
    assert_eq!(encrypted.secret, secret());
    assert_eq!(encrypted.payload, vec![4; 64]);
}
