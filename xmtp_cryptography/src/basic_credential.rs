use ed25519_dalek::SigningKey;
use k256::schnorr::CryptoRngCore;
use openmls_basic_credential::SignatureKeyPair;
use openmls_traits::signatures::Signer;
use openmls_traits::{signatures, types::SignatureScheme};
use serde::de::Error;
use std::io::BufReader;
use tls_codec::SecretTlsVecU8;
use zeroize::Zeroizing;

/// Wrapper for [`signatures::SignerError`] that implements [`std::fmt::Display`]
#[derive(thiserror::Error, Debug)]
pub struct SignerError {
    inner: signatures::SignerError,
}

impl From<signatures::SignerError> for SignerError {
    fn from(err: signatures::SignerError) -> SignerError {
        SignerError { inner: err }
    }
}

impl std::fmt::Display for SignerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use signatures::SignerError::*;
        match self.inner {
            SigningError => write!(f, "signing error"),
            InvalidSignature => write!(f, "invalid signature"),
            CryptoError(c) => write!(f, "{}", c),
        }
    }
}

mod private {

    /// A rudimentary form of specialization
    /// this allows implementing CredentialSigning
    /// on `XmtpInstallationCredential` in foreign crates.
    /// A `private::NotSpecialized` trait may only be defined in `xmtp_cryptography`.
    /// Since it is not defined, implementations in their own crates are preferred.
    pub struct NotSpecialized;
}

/// Sign with some public/private keypair credential
pub trait CredentialSign<SP = private::NotSpecialized> {
    /// the hashed context this credential signature takes place in
    type Error;

    fn credential_sign<T: SigningContextProvider>(
        &self,
        text: impl AsRef<str>,
    ) -> Result<Vec<u8>, Self::Error>;
}

pub trait SigningContextProvider {
    fn context() -> &'static [u8];
}

/// Verify a credential signature with its public key
pub trait CredentialVerify<SP = private::NotSpecialized> {
    type Error;

    fn credential_verify<T: SigningContextProvider>(
        &self,
        signature_text: impl AsRef<str>,
        signature_bytes: &[u8; 64],
    ) -> Result<(), Self::Error>;
}

/// The credential for an XMTP Installation
/// an XMTP Installation often refers to one specific device,
/// and is an ed25519 key
// Boxing the inner value avoids creating large enums if an enum stores multiple installation
// credentials
#[derive(Debug, Clone)]
pub struct XmtpInstallationCredential(Box<SigningKey>);

impl Default for XmtpInstallationCredential {
    fn default() -> Self {
        Self(Box::new(SigningKey::generate(&mut crate::utils::rng())))
    }
}

impl XmtpInstallationCredential {
    /// Create a new [`XmtpInstallationCredential`] with [`rand_chacha::ChaCha20Rng`]
    pub fn new() -> Self {
        Self(Box::new(SigningKey::generate(&mut crate::utils::rng())))
    }

    /// Create a new [`XmtpInstallationCredential`] with custom RNG
    pub fn with_rng<R: CryptoRngCore + ?Sized>(rng: &mut R) -> Self {
        Self(Box::new(SigningKey::generate(rng)))
    }

    /// Get a reference to the public [`ed25519_dalek::VerifyingKey`]
    /// Can be used to verify signatures
    pub fn verifying_key(&self) -> ed25519_dalek::VerifyingKey {
        self.0.verifying_key()
    }

    /// View the public [`ed25519_dalek::VerifyingKey`] as constant-sized bytes
    pub fn public_bytes(&self) -> &[u8; 32] {
        self.0.as_ref().as_ref().as_bytes()
    }

    /// View the public [`ed25519_dalek::VerifyingKey`] as a slice
    pub fn public_slice(&self) -> &[u8] {
        self.0.as_ref().as_ref().as_ref()
    }

    /// get the scheme, prefer the public [`Signer::signature_scheme`]
    fn scheme(&self) -> SignatureScheme {
        SignatureScheme::ED25519
    }

    pub fn with_context<'k, 'v>(
        &'k self,
        context: &'v [u8],
    ) -> Result<ed25519_dalek::Context<'k, 'v, SigningKey>, ed25519_dalek::SignatureError> {
        self.0.with_context(context)
    }

    /// Internal helper function to safely create a credential from its raw parts
    /// private and public must be exactly 32 bytes large.
    fn from_raw(private: &[u8], public: &[u8]) -> Result<Self, ed25519_dalek::SignatureError> {
        let keypair = Zeroizing::new({
            let mut keypair = [0u8; 64];
            keypair[0..32].copy_from_slice(private);
            keypair[32..].copy_from_slice(public);
            keypair
        });

        let signing_key = SigningKey::from_keypair_bytes(&keypair)?;
        Ok(Self(Box::new(signing_key)))
    }

    /// Alias for [`ed25519_dalek::SigningKey::from_bytes`]
    pub fn from_bytes(bytes: &[u8; 32]) -> Result<Self, ed25519_dalek::SignatureError> {
        let key = SigningKey::from_bytes(bytes);
        Ok(Self(Box::new(key)))
    }

    /// private key for this credential
    #[cfg(feature = "exposed-keys")]
    pub fn private_bytes(&self) -> [u8; 32] {
        self.0.to_bytes()
    }
}

/// The signer here must maintain compatability with `SignatureKeyPair`
impl Signer for XmtpInstallationCredential {
    fn sign(&self, payload: &[u8]) -> Result<Vec<u8>, signatures::SignerError> {
        SignatureKeyPair::from(self).sign(payload)
    }

    fn signature_scheme(&self) -> SignatureScheme {
        self.scheme()
    }
}

// The signer here must maintain compatability with `SignatureKeyPair`
impl Signer for &XmtpInstallationCredential {
    fn sign(&self, payload: &[u8]) -> Result<Vec<u8>, signatures::SignerError> {
        SignatureKeyPair::from(*self).sign(payload)
    }

    fn signature_scheme(&self) -> SignatureScheme {
        self.scheme()
    }
}

impl tls_codec::Deserialize for XmtpInstallationCredential {
    fn tls_deserialize<R: std::io::Read>(bytes: &mut R) -> Result<Self, tls_codec::Error>
    where
        Self: Sized,
    {
        // a bufreader consumes its input, unlike just a `Read` instance.
        let mut buf = BufReader::new(bytes);
        let private = SecretTlsVecU8::tls_deserialize(&mut buf)?;
        let public = SecretTlsVecU8::tls_deserialize(&mut buf)?;
        let scheme = SignatureScheme::tls_deserialize(&mut buf)?;
        if scheme != SignatureScheme::ED25519 {
            return Err(tls_codec::Error::DecodingError(
                "XMTP InstallationCredential must be Ed25519".into(),
            ));
        }

        Self::from_raw(private.as_slice(), public.as_slice())
            .map_err(|e| tls_codec::Error::DecodingError(e.to_string()))
    }
}

impl From<XmtpInstallationCredential> for SignatureKeyPair {
    fn from(key: XmtpInstallationCredential) -> SignatureKeyPair {
        SignatureKeyPair::from_raw(
            key.signature_scheme(),
            key.0.to_bytes().into(),
            key.0.verifying_key().to_bytes().into(),
        )
    }
}

impl<'a> From<&'a XmtpInstallationCredential> for SignatureKeyPair {
    fn from(key: &'a XmtpInstallationCredential) -> SignatureKeyPair {
        SignatureKeyPair::from_raw(
            key.signature_scheme(),
            key.0.to_bytes().into(),
            key.0.verifying_key().to_bytes().into(),
        )
    }
}

impl From<SigningKey> for XmtpInstallationCredential {
    fn from(signing_key: SigningKey) -> Self {
        Self(Box::new(signing_key))
    }
}

impl<'a> From<&'a SigningKey> for XmtpInstallationCredential {
    fn from(signing_key: &'a SigningKey) -> Self {
        Self(Box::new(signing_key.clone()))
    }
}

impl tls_codec::Serialize for XmtpInstallationCredential {
    fn tls_serialize<W: std::io::Write>(&self, writer: &mut W) -> Result<usize, tls_codec::Error> {
        SignatureKeyPair::from(self).tls_serialize(writer)
    }
}

impl tls_codec::Size for XmtpInstallationCredential {
    fn tls_serialized_len(&self) -> usize {
        SignatureKeyPair::from(self).tls_serialized_len()
    }
}

impl serde::Serialize for XmtpInstallationCredential {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        SignatureKeyPair::from(self).serialize(serializer)
    }
}

impl<'de> serde::Deserialize<'de> for XmtpInstallationCredential {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize, zeroize::ZeroizeOnDrop)]
        struct SignatureKeyPairRemote {
            private: Vec<u8>,
            public: Vec<u8>,
            #[allow(dead_code)]
            #[zeroize(skip)]
            _signature_scheme: SignatureScheme,
        }

        let SignatureKeyPairRemote {
            ref private,
            ref public,
            ..
        } = SignatureKeyPairRemote::deserialize(deserializer)?;

        Self::from_raw(private.as_slice(), public.as_slice())
            .map_err(|e| <D as serde::Deserializer<'_>>::Error::custom(format!("{}", e)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tls_codec::{Deserialize as _, Serialize as _};

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn test_is_binary_compatible_with_mls_deser() {
        // XmtpInstallationCredential needs to be binary-compatible/tls-codec/serde compatible with
        // `SignatureKeyPair` from xmtp_basic_credential
        let keypair = SignatureKeyPair::new(SignatureScheme::ED25519).unwrap();
        let mut serialized: Vec<u8> = Vec::new();

        keypair.tls_serialize(&mut serialized).unwrap();
        let x_kp = XmtpInstallationCredential::tls_deserialize(&mut serialized.as_slice()).unwrap();
        assert_eq!(keypair.private(), &x_kp.0.to_bytes());
        assert_eq!(keypair.public(), &x_kp.0.verifying_key().to_bytes());
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn test_is_binary_compatible_with_mls_ser() {
        let keypair = XmtpInstallationCredential::new();
        let mut serialized: Vec<u8> = Vec::new();

        keypair.tls_serialize(&mut serialized).unwrap();
        let mls_kp = SignatureKeyPair::tls_deserialize(&mut serialized.as_slice()).unwrap();
        assert_eq!(mls_kp.private(), &keypair.0.to_bytes());
        assert_eq!(mls_kp.public(), &keypair.0.verifying_key().to_bytes());
        assert_eq!(mls_kp.signature_scheme(), keypair.scheme());
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn test_is_binary_compatible_with_mls_deser_serde() {
        // XmtpInstallationCredential needs to be serde compatible with
        // `SignatureKeyPair` from xmtp_basic_credential
        let keypair = SignatureKeyPair::new(SignatureScheme::ED25519).unwrap();
        let serialized: Vec<u8> = bincode::serialize(&keypair).unwrap();

        let x_kp: XmtpInstallationCredential = bincode::deserialize(serialized.as_slice()).unwrap();
        assert_eq!(keypair.private(), &x_kp.0.to_bytes());
        assert_eq!(keypair.public(), &x_kp.0.verifying_key().to_bytes());
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn test_is_binary_compatible_with_mls_ser_serde() {
        let keypair = XmtpInstallationCredential::new();
        let serialized: Vec<u8> = bincode::serialize(&keypair).unwrap();

        let mls_kp: SignatureKeyPair = bincode::deserialize(serialized.as_slice()).unwrap();
        assert_eq!(mls_kp.private(), &keypair.0.to_bytes());
        assert_eq!(mls_kp.public(), &keypair.0.verifying_key().to_bytes());
        assert_eq!(mls_kp.signature_scheme(), keypair.scheme());
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn secret_key_can_not_be_exposed() {
        let keypair = XmtpInstallationCredential::new();
        let secret = keypair.0.as_ref();

        assert_ne!(keypair.public_bytes(), secret.as_bytes());
        assert_ne!(keypair.public_slice(), secret.as_bytes());
        assert_ne!(keypair.verifying_key().as_bytes(), &secret.to_bytes());
    }
}
