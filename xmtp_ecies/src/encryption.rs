use xmtp_crypto::encryption::encrypt;
use xmtp_proto::xmtp::message_contents::{
    ciphertext::{Aes256gcmHkdfsha256, Union},
    Ciphertext,
};

pub fn encrypt_to_ciphertext(
    private_key: &[u8],
    message: &[u8],
    additional_data: Option<&[u8]>,
) -> Result<Ciphertext, String> {
    let raw_ciphertext = encrypt(message, private_key, additional_data)?;
    let ciphertext = Ciphertext {
        union: Some(Union::Aes256GcmHkdfSha256(Aes256gcmHkdfsha256 {
            hkdf_salt: raw_ciphertext.hkdf_salt,
            gcm_nonce: raw_ciphertext.gcm_nonce,
            payload: raw_ciphertext.payload,
        })),
    };

    Ok(ciphertext)
}
