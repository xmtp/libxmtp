use super::*;
use alloy::signers::SignerSync;

const KEY: &str = "0x0000000000000000000000000000000000000000000000000000000000000001";

// verifies: IDENT-088, IDENT-089
#[xmtp_common::test(unwrap_try = true)]
async fn local_key_restores_identity_and_personal_message_signatures() {
    let key = LocalSigningKey::from_hex(KEY.to_owned().into()).await?;
    assert_eq!(
        key.identity().await?.identifier,
        "0x7e5f4552091a69125d5dfcb7b8c2659029395bdf"
    );
    assert!(matches!(key.kind().await?, SignerKind::Eoa));
    let mut bytes = export_bytes(&key).await;
    assert_eq!(bytes.len(), 32);
    assert_eq!(bytes[31], 1);
    let restored = LocalSigningKey::from_bytes(bytes.clone().into()).await?;
    bytes.fill(0);
    assert_eq!(export_hex(&key).await, KEY);
    assert_eq!(export_hex(&restored).await, KEY);
    let text = "XMTP local signing key round trip";
    let expected = PrivateKeySigner::from_slice(&export_bytes(&key).await)?
        .sign_message_sync(text.as_bytes())?;
    for signer in [key, restored] {
        let Signature::Ecdsa(actual) = signer.sign(SigningRequest { text: text.into() }).await?
        else {
            panic!("expected ECDSA")
        };
        assert_eq!(actual, expected.as_bytes());
    }
}

// verifies: IDENT-089
#[xmtp_common::test(unwrap_try = true)]
async fn local_key_rejects_invalid_scalars_and_encodings_without_echoing_them() {
    for bytes in [
        vec![],
        vec![1; 31],
        vec![1; 33],
        vec![0; 32],
        vec![255; 32],
        hex::decode("fffffffffffffffffffffffffffffffebaaedce6af48a03bbfd25e8cd0364141")?,
    ] {
        assert!(matches!(
            LocalSigningKey::from_bytes(bytes.into()).await,
            Err(XmtpError::InvalidInput(_))
        ));
    }
    for text in [
        "".to_owned(),
        KEY[2..].to_owned(),
        KEY.replace("0x", "0X"),
        format!("{KEY}00"),
        format!(" {KEY}"),
        format!("0x{}", "g".repeat(64)),
        format!("0x{}", "0".repeat(64)),
    ] {
        let error = LocalSigningKey::from_hex(text.into()).await.unwrap_err();
        assert!(matches!(error, XmtpError::InvalidInput(_)));
        assert_eq!(
            error.to_string(),
            XmtpError::invalid("invalid local signer private key").to_string()
        );
    }
    let upper = "0x".to_owned() + &"ab".repeat(32);
    let key =
        LocalSigningKey::from_hex(upper.to_uppercase().replacen("0X", "0x", 1).into()).await?;
    assert_eq!(export_hex(&key).await, upper);
}

#[xmtp_common::test(unwrap_try = true)]
async fn local_key_generation_is_exportable_and_debug_is_redacted() {
    let first = generate_keys().await;
    let second = generate_keys().await;
    assert_ne!(export_bytes(&first).await, export_bytes(&second).await);
    let restored = LocalSigningKey::from_hex(export_hex(&first).await.into()).await?;
    assert_eq!(
        first.identity().await?.identifier,
        restored.identity().await?.identifier
    );
    assert_eq!(format!("{first:?}"), "LocalSigningKey([REDACTED])");
}

async fn export_bytes(key: &LocalSigningKey) -> Vec<u8> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        key.to_bytes().to_vec()
    }
    #[cfg(target_arch = "wasm32")]
    {
        key.to_bytes().await.to_vec()
    }
}

async fn export_hex(key: &LocalSigningKey) -> String {
    #[cfg(not(target_arch = "wasm32"))]
    {
        key.to_hex()
    }
    #[cfg(target_arch = "wasm32")]
    {
        key.to_hex().await
    }
}
