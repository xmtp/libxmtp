use libsecp256k1::{
    sign as secp_sign, Error as SecpError, Message, SecretKey, Signature as SecpSignature,
};
use prost::Message as ProstMessage;
use xmtp_crypto::hashes::sha256;
use xmtp_proto::xmtp::message_contents::{
    private_preferences_payload::Version as PrivatePreferencesVersion, signature::EcdsaCompact,
    signature::Union, Ciphertext, PrivatePreferencesPayload, Signature, SignedPayload,
};

use crate::encryption::encrypt_to_ciphertext;

pub fn encrypt_message(
    private_key: &[u8], // secp256k1 private key
    message: &[u8],     // any byte array
) -> Result<Vec<u8>, String> {
    let signed_payload = build_signed_payload(private_key, message)?;
    let message_bytes = signed_payload.encode_to_vec();
    // TODO: Figure out if we should include additonal data
    let ciphertext = encrypt_to_ciphertext(private_key, &message_bytes, None)?;
    let ecies_message = PrivatePreferencesPayload {
        version: Some(PrivatePreferencesVersion::V1(ciphertext)),
    };

    Ok(ecies_message.encode_to_vec())
}

pub fn decrypt_message(
    public_key: &[u8],  // secp256k1 public key
    private_key: &[u8], // secp256k1 private key
    message: &[u8],     // message encrypted with `encrypt_message`
) -> Result<Vec<u8>, String> {
    let ciphertext = get_ciphertext(message)?;

    let signed_payload_bytes = decrypt_bytes(private_key, &ciphertext)?;
    let signed_payload = SignedPayload::decode(signed_payload_bytes.as_slice())
        .map_err(|e| format!("error decoding signed payload: {:?}", e))?;

    let message_bytes = signed_payload.payload;

    let public_key = libsecp256k1::PublicKey::parse_slice(public_key, None)
        .map_err(|e| format!("error parsing public key: {:?}", e))?;

    let message_hash = Message::parse_slice(sha256(message_bytes.as_slice()).as_slice())
        .map_err(|e| format!("error parsing message: {:?}", e))?;

    let message_signature = match signed_payload.signature {
        Some(signature) => signature,
        None => return Err("no signature found".to_string()),
    };

    let raw_sig = match message_signature.union {
        Some(Union::EcdsaCompact(sig)) => sig.bytes,
        None => return Err("no signature bytes found".to_string()),
        _ => return Err("invalid signature type".to_string()),
    };

    let sized_sig_bytes = raw_sig
        .as_slice()
        .try_into()
        .map_err(|e| format!("signature error: {:?}", e))?;

    let signature =
        SecpSignature::parse_standard(sized_sig_bytes).map_err(|e| format!("{:?}", e))?;

    let is_valid = libsecp256k1::verify(&message_hash, &signature, &public_key);

    if !is_valid {
        return Err("signature is invalid".to_string());
    }

    Ok(message_bytes)
}

fn get_ciphertext(message: &[u8]) -> Result<Ciphertext, String> {
    let ecies_message =
        PrivatePreferencesPayload::decode(message).map_err(|e| format!("{:?}", e))?;
    let ciphertext = match ecies_message.version {
        Some(PrivatePreferencesVersion::V1(ciphertext)) => ciphertext,
        None => return Err("no ciphertext found".to_string()),
    };

    Ok(ciphertext)
}

fn build_signed_payload(private_key: &[u8], message: &[u8]) -> Result<SignedPayload, String> {
    let signature = sign(private_key, message).map_err(|e| format!("{:?}", e))?;
    Ok(SignedPayload {
        payload: message.to_vec(),
        signature: Some(signature),
    })
}

fn sign(private_key: &[u8], message: &[u8]) -> Result<Signature, SecpError> {
    let sec = SecretKey::parse_slice(private_key)?;
    let hash = sha256(message);
    let msg = Message::parse_slice(hash.as_slice())?;

    let (sig, recovery) = secp_sign(&msg, &sec);

    let recovery_u32 = recovery.serialize() as u32;

    return Ok(Signature {
        union: Some(Union::EcdsaCompact(EcdsaCompact {
            bytes: sig.serialize().to_vec(),
            recovery: recovery_u32,
        })),
    });
}

#[cfg(test)]
mod test {
    use ecies::utils::generate_keypair;

    use super::*;

    #[test]
    fn test_round_trip() {
        let (private_key, pub_key) = generate_keypair();
        let message = "hello world".as_bytes().to_vec();

        let encrypted =
            encrypt_message(&pub_key.serialize(), &private_key.serialize(), &message).unwrap();
        assert!(encrypted.len() > 0);

        let decrypted =
            decrypt_message(&pub_key.serialize(), &private_key.serialize(), &encrypted).unwrap();

        assert_eq!(message, decrypted);
    }

    #[test]
    fn decrypt_fails_with_incorrect_pub_key() {
        let (private_key, pub_key) = generate_keypair();
        let message = "hello world".as_bytes().to_vec();

        let encrypted =
            encrypt_message(&pub_key.serialize(), &private_key.serialize(), &message).unwrap();
        assert!(encrypted.len() > 0);
        let (_, other_pub_key) = generate_keypair();

        let decrypt_result = decrypt_message(
            &other_pub_key.serialize(),
            &private_key.serialize(),
            &encrypted,
        );

        assert_eq!(decrypt_result.is_err(), true);
    }

    #[test]
    fn decrypt_fails_with_incorrect_private_key() {
        let (private_key, pub_key) = generate_keypair();
        let message = "hello world".as_bytes().to_vec();

        let encrypted =
            encrypt_message(&pub_key.serialize(), &private_key.serialize(), &message).unwrap();
        assert!(encrypted.len() > 0);
        let (other_private_key, _) = generate_keypair();

        let decrypt_result = decrypt_message(
            &pub_key.serialize(),
            &other_private_key.serialize(),
            &encrypted,
        );

        assert_eq!(decrypt_result.is_err(), true);
    }
}
