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
        let key = Zeroizing::new(key);
        Self::parse_bytes(&key)
    }

    /// Restore a private key encoded as 0x followed by exactly 64 hexadecimal digits.
    #[uniffi::constructor]
    pub async fn from_hex(key: String) -> Result<Arc<Self>, XmtpError> {
        let key = Zeroizing::new(key);
        let encoded = key
            .strip_prefix("0x")
            .filter(|text| text.len() == PRIVATE_KEY_HEX_LENGTH)
            .ok_or_else(|| XmtpError::invalid("invalid local signer private key"))?;
        // Accepts either hex case.
        let mut bytes = Zeroizing::new([0u8; PRIVATE_KEY_LENGTH]);
        hex::decode_to_slice(encoded, bytes.as_mut_slice())
            .map_err(|_| XmtpError::invalid("invalid local signer private key"))?;
        Self::parse_bytes(bytes.as_slice())
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

impl LocalSigningKey {
    // Parse below the exported boundary so each failed SDK call logs only once.
    // implements: IDENT-089
    fn parse_bytes(key: &[u8]) -> Result<Arc<Self>, XmtpError> {
        if key.len() != PRIVATE_KEY_LENGTH {
            return Err(XmtpError::invalid("invalid local signer private key"));
        }
        let signer = PrivateKeySigner::from_slice(key)
            .map_err(|_| XmtpError::invalid("invalid local signer private key"))?;
        Ok(Arc::new(Self(signer)))
    }

    /// The one copy of the private key that export makes. alloy has no
    /// borrowing accessor for the scalar.
    fn private_key(&self) -> Zeroizing<[u8; PRIVATE_KEY_LENGTH]> {
        // zeroize: the copy is held in the returned `Zeroizing`
        #[allow(clippy::disallowed_methods)]
        Zeroizing::new(self.0.to_bytes().0)
    }

    // implements: IDENT-088
    fn private_key_bytes(&self) -> Vec<u8> {
        self.private_key().to_vec()
    }

    /// The returned `String` is the only heap copy: digits are written
    /// straight into it.
    // implements: IDENT-088
    fn private_key_hex(&self) -> String {
        const DIGITS: &[u8; 16] = b"0123456789abcdef";
        let mut hex = String::with_capacity(2 + PRIVATE_KEY_HEX_LENGTH);
        hex.push_str("0x");
        for byte in self.private_key().iter() {
            hex.push(char::from(DIGITS[usize::from(byte >> 4)]));
            hex.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
        }
        hex
    }
}

#[xmtp_macro::sdk_export(native_only)]
impl LocalSigningKey {
    /// Export a copy of the private key. Store it securely; it controls the account.
    #[sdk(immutable)]
    pub fn to_bytes(&self) -> Vec<u8> {
        self.private_key_bytes()
    }

    /// Export the private key as lowercase, 0x-prefixed hex. Store it securely.
    #[sdk(immutable)]
    pub fn to_hex(&self) -> String {
        self.private_key_hex()
    }
}

#[xmtp_macro::sdk_export(wasm_only)]
impl LocalSigningKey {
    /// Export a copy of the private key. Store it securely; it controls the account.
    pub async fn to_bytes(&self) -> Vec<u8> {
        self.private_key_bytes()
    }

    /// Export the private key as lowercase, 0x-prefixed hex. Store it securely.
    pub async fn to_hex(&self) -> String {
        self.private_key_hex()
    }
}

#[cfg(test)]
#[path = "local_tests.rs"]
mod local_tests;
