use xmtp_cryptography::hash::sha256_bytes;
use xmtp_proto::xmtp::{
    message_contents::{
        ciphertext::{Aes256gcmHkdfsha256, Union},
        Ciphertext,
    },
    mls::message_contents::EncodedContent,
};
use xmtp_v2::encryption::{decrypt, encrypt, hkdf};

use crate::{bytes_to_encoded_content, encoded_content_to_bytes};

pub const ENCODED_CONTENT_ENCRYPTION_KEY_SALT: &[u8] = b"XMTP_ENCODED_CONTENT_ENCRYPTION";

pub struct EncryptedEncodedContent {
    pub content_digest: String,
    pub secret: Vec<u8>,
    pub salt: Vec<u8>,
    pub nonce: Vec<u8>,
    pub payload: Vec<u8>,
    pub content_length: Option<u32>,
    pub filename: Option<String>,
}

pub fn encrypt_encoded_content(
    private_key: &[u8],
    public_key: &[u8],
    encoded_content: EncodedContent,
) -> Result<EncryptedEncodedContent, String> {
    let ciphertext: Ciphertext = encrypt_to_ciphertext(
        private_key,
        &encoded_content_to_bytes(encoded_content),
        public_key,
    )?;

    // Get the payload from the ciphertext
    let payload = match &ciphertext.union {
        Some(Union::Aes256GcmHkdfSha256(aes)) => &aes.payload,
        _ => return Err("Invalid ciphertext format".to_string()),
    };

    let nonce = match &ciphertext.union {
        Some(Union::Aes256GcmHkdfSha256(aes)) => &aes.gcm_nonce,
        _ => return Err("Invalid ciphertext format".to_string()),
    };

    let salt = match &ciphertext.union {
        Some(Union::Aes256GcmHkdfSha256(aes)) => &aes.hkdf_salt,
        _ => return Err("Invalid ciphertext format".to_string()),
    };

    // Calculate digest and convert to hex string
    let digest = sha256_bytes(payload);
    let content_digest = hex::encode(digest);

    Ok(EncryptedEncodedContent {
        content_digest,
        secret: private_key.to_vec(),
        salt: salt.clone(),
        nonce: nonce.clone(),
        payload: payload.clone(),
        content_length: None,
        filename: None,
    })
}

pub fn decrypt_encoded_content(
    private_key: &[u8],
    public_key: &[u8],
    encrypted_encoded_content: EncryptedEncodedContent,
) -> Result<EncodedContent, String> {
    println!(
        "GOT HERE!! => encrypted_encoded_content: {:?}",
        encrypted_encoded_content.secret
    );

    let ciphertext = Ciphertext {
        union: Some(Union::Aes256GcmHkdfSha256(Aes256gcmHkdfsha256 {
            hkdf_salt: encrypted_encoded_content.salt,
            gcm_nonce: encrypted_encoded_content.nonce,
            payload: encrypted_encoded_content.payload,
        })),
    };

    println!("GOT HERE!! => ciphertext: {:?}", ciphertext);

    let decrypted = decrypt_ciphertext(private_key, ciphertext, public_key)?;

    println!("GOT HERE!! => decrypted: {:?}", decrypted);

    Ok(bytes_to_encoded_content(decrypted))
}

fn derive_encryption_key(private_key: &[u8]) -> Result<[u8; 32], String> {
    let derived_key = hkdf(private_key, ENCODED_CONTENT_ENCRYPTION_KEY_SALT)?;

    Ok(derived_key)
}

pub fn encrypt_to_ciphertext(
    private_key: &[u8],
    message: &[u8],
    additional_data: &[u8],
) -> Result<Ciphertext, String> {
    let secret_key = derive_encryption_key(private_key)?;
    let raw_ciphertext = encrypt(message, &secret_key, Some(additional_data))?;
    println!("GOT HERE!! => encrypt action MESSAGE: {:?}", message);

    Ok(Ciphertext {
        union: Some(Union::Aes256GcmHkdfSha256(Aes256gcmHkdfsha256 {
            hkdf_salt: raw_ciphertext.hkdf_salt,
            gcm_nonce: raw_ciphertext.gcm_nonce,
            payload: raw_ciphertext.payload,
        })),
    })
}

pub fn decrypt_ciphertext(
    private_key: &[u8],
    ciphertext: Ciphertext,
    additional_data: &[u8],
) -> Result<Vec<u8>, String> {
    let encryption_key = derive_encryption_key(private_key)?;
    let unwrapped = unwrap_ciphertext(ciphertext)?;

    println!("GOT HERE!! => unwrapped payload: {:?}", unwrapped.payload);
    println!(
        "GOT HERE!! => unwrapped hkdf_salt: {:?}",
        unwrapped.hkdf_salt
    );
    println!(
        "GOT HERE!! => unwrapped gcm_nonce: {:?}",
        unwrapped.gcm_nonce
    );
    println!(
        "GOT HERE!! => unwrapped encryption_key: {:?}",
        encryption_key
    );
    println!(
        "GOT HERE!! => unwrapped additional_data: {:?}",
        additional_data
    );
    let decrypted = decrypt(
        unwrapped.payload.as_slice(),
        unwrapped.hkdf_salt.as_slice(),
        unwrapped.gcm_nonce.as_slice(),
        &encryption_key,
        Some(additional_data),
    )?;
    println!("GOT HERE!! => decrypted: {:?}", decrypted);
    Ok(decrypted)
}

fn unwrap_ciphertext(ciphertext: Ciphertext) -> Result<Aes256gcmHkdfsha256, String> {
    match ciphertext.union {
        Some(Union::Aes256GcmHkdfSha256(data)) => Ok(data),
        _ => Err("unrecognized format".to_string()),
    }
}
