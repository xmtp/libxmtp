// This defines the flutter API to libxmtp.
//
// The contents are processed by `flutter_rust_bridge` to generate
// the corresponding `bridge_generated.rs` and `bridge_generated.dart`.
// See .flutter_rust_bridge.yaml

#[derive(Debug)]
pub enum XmtpError {
    GenericError(String),
}

pub fn generate_private_preferences_topic_identifier(
    private_key_bytes: Vec<u8>,
) -> Result<String, XmtpError> {
    xmtp_user_preferences::topic::generate_private_preferences_topic_identifier(
        private_key_bytes.as_slice(),
    )
    .map_err(|e| XmtpError::GenericError(e))
}

pub fn user_preferences_encrypt(
    public_key: Vec<u8>,
    private_key: Vec<u8>,
    message: Vec<u8>,
) -> Result<Vec<u8>, XmtpError> {
    xmtp_user_preferences::encrypt_message(
        public_key.as_slice(),
        private_key.as_slice(),
        message.as_slice(),
    )
    .map_err(|e| XmtpError::GenericError(e))
}

pub fn user_preferences_decrypt(
    public_key: Vec<u8>,
    private_key: Vec<u8>,
    encrypted_message: Vec<u8>,
) -> Result<Vec<u8>, XmtpError> {
    xmtp_user_preferences::decrypt_message(
        public_key.as_slice(),
        private_key.as_slice(),
        encrypted_message.as_slice(),
    )
    .map_err(|e| XmtpError::GenericError(e))
}
