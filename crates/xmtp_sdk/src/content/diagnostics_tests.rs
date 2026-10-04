use super::*;

fn secret() -> Vec<u8> {
    (101..133).collect()
}

fn remote_attachment() -> RemoteAttachment {
    RemoteAttachment {
        url: "https://example.invalid/attachment?token=attachment-access-token".into(),
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
        !diagnostic.contains("attachment-access-token"),
        "attachment URL in {diagnostic}"
    );
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
    assert_eq!(
        remote.url,
        "https://example.invalid/attachment?token=attachment-access-token"
    );
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

// verifies: LOG-010
#[xmtp_common::test(unwrap_try = true)]
fn encoded_content_diagnostics_hide_secret_parameter() {
    let canary = "encoded-content-decryption-secret";
    let content = EncodedContent {
        r#type: ContentTypeId {
            authority_id: "xmtp.org".into(),
            type_id: "remoteStaticAttachment".into(),
            version_major: 1,
            version_minor: 0,
        },
        parameters: HashMap::from([
            ("secret".into(), canary.into()),
            ("filename".into(), "attachment.txt".into()),
        ]),
        fallback: Some("attachment".into()),
        content: vec![1, 2, 3],
    };
    let diagnostics = [
        ("direct", format!("{content:?}")),
        ("nested", format!("{:?}", vec![content.clone()])),
    ];
    let leaked: Vec<_> = diagnostics
        .iter()
        .filter(|(_, value)| value.contains(canary))
        .map(|(scope, _)| *scope)
        .collect();
    assert!(
        leaked.is_empty(),
        "EncodedContent secret exposed in {leaked:?}"
    );
    for (_, diagnostic) in diagnostics {
        assert!(diagnostic.contains("<redacted>"));
        assert!(diagnostic.contains("attachment.txt"));
    }
    assert_eq!(content.parameters["secret"], canary);
    assert_eq!(content.content, vec![1, 2, 3]);
}

#[xmtp_common::test(unwrap_try = true)]
fn encoded_content_diagnostics_hide_signed_attachment_url() {
    let remote = remote_attachment();
    let encoded = encode_standard(StandardContent::RemoteAttachment(remote.clone()))?;
    assert_eq!(encoded.content, remote.url.as_bytes());
    let nested = StandardContent::Reply {
        reference: "ab".repeat(32).try_into()?,
        reference_inbox_id: None,
        content: encoded.clone(),
    };
    let token_bytes = b"attachment-access-token"
        .iter()
        .map(u8::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    let diagnostics = [
        ("direct", format!("{encoded:?}")),
        ("reply", format!("{nested:?}")),
    ];
    let leaked: Vec<_> = diagnostics
        .iter()
        .filter(|(_, diagnostic)| {
            diagnostic.contains("attachment-access-token") || diagnostic.contains(&token_bytes)
        })
        .map(|(scope, _)| *scope)
        .collect();
    assert!(
        leaked.is_empty(),
        "signed URL exposed in {leaked:?} diagnostics"
    );
    for (_, diagnostic) in diagnostics {
        assert!(diagnostic.contains(&format!("content_bytes: {}", encoded.content.len())));
    }
    let StandardContent::RemoteAttachment(decoded) = decode_standard(encoded)? else {
        panic!("remote attachment did not round trip");
    };
    assert_eq!(decoded.url, remote.url);
    assert_eq!(decoded.secret, remote.secret);
}
