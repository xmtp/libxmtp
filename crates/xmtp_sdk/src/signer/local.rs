const PRIVATE_KEY_LENGTH: usize = 32;
const PRIVATE_KEY_HEX_LENGTH: usize = PRIVATE_KEY_LENGTH * 2;

/// An exportable Ethereum signing key. Only explicit export methods reveal its private key.
#[derive(uniffi::Object)]
pub struct LocalSigningKey(PrivateKeySigner);

impl std::fmt::Debug for LocalSigningKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("LocalSigningKey([REDACTED])")
    }
}

/// Generate an exportable local key without a backend or client database.
#[xmtp_macro::sdk_export(client_static)]
pub async fn generate_keys() -> Arc<LocalSigningKey> {
    Arc::new(LocalSigningKey(PrivateKeySigner::random()))
}

#[xmtp_macro::sdk_export]
impl LocalSigningKey {
    /// Restore a 32-byte secp256k1 private key. The input is copied.
    #[uniffi::constructor]
    pub async fn from_bytes(key: Vec<u8>) -> Result<Arc<Self>, XmtpError> {
        if key.len() != PRIVATE_KEY_LENGTH {
            return Err(XmtpError::invalid("invalid local signer private key"));
        }
        let signer = PrivateKeySigner::from_slice(&key)
            .map_err(|_| XmtpError::invalid("invalid local signer private key"))?;
        Ok(Arc::new(Self(signer)))
    }

    /// Restore a private key encoded as 0x followed by exactly 64 hexadecimal digits.
    #[uniffi::constructor]
    pub async fn from_hex(key: String) -> Result<Arc<Self>, XmtpError> {
        let encoded = key
            .strip_prefix("0x")
            .filter(|text| text.len() == PRIVATE_KEY_HEX_LENGTH)
            .ok_or_else(|| XmtpError::invalid("invalid local signer private key"))?;
        let bytes = hex::decode(encoded)
            .map_err(|_| XmtpError::invalid("invalid local signer private key"))?;
        Self::from_bytes(bytes).await
    }

    /// Export a copy of the private key. Store it securely; it controls the account.
    pub async fn to_bytes(&self) -> Vec<u8> {
        self.0.to_bytes().to_vec()
    }

    /// Export the private key as lowercase, 0x-prefixed hex. Store it securely.
    pub async fn to_hex(&self) -> String {
        format!("0x{}", hex::encode(self.0.to_bytes()))
    }

    pub async fn identity(&self) -> Result<PublicIdentity, XmtpError> {
        <Self as Signer>::identity(self)
            .await
            .map_err(|_| XmtpError::signer())
    }

    pub async fn kind(&self) -> Result<SignerKind, XmtpError> {
        <Self as Signer>::kind(self)
            .await
            .map_err(|_| XmtpError::signer())
    }

    pub async fn sign(&self, request: SigningRequest) -> Result<Signature, XmtpError> {
        <Self as Signer>::sign(self, request)
            .await
            .map_err(|_| XmtpError::signer())
    }
}

#[cfg(test)]
#[path = "local_tests.rs"]
mod local_tests;
