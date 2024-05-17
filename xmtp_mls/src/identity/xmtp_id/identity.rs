use std::array::TryFromSliceError;

use ed25519_dalek::SigningKey;
use ethers::signers::{LocalWallet, WalletError};
use openmls::{
    credentials::{errors::BasicCredentialError, BasicCredential},
    prelude::Credential as OpenMlsCredential,
};
use openmls_basic_credential::SignatureKeyPair;
use openmls_traits::types::CryptoError;
use prost::Message;
use sha2::{Digest, Sha512};
use thiserror::Error;
use xmtp_id::{
    associations::{
        builder::{SignatureRequest, SignatureRequestBuilder, SignatureRequestError},
        generate_inbox_id, InstallationKeySignature, LegacyDelegatedSignature, MemberIdentifier,
        RecoverableEcdsaSignature,
    },
    constants::INSTALLATION_KEY_SIGNATURE_CONTEXT,
    InboxId,
};
use xmtp_proto::{
    api_client::{XmtpIdentityClient, XmtpMlsClient},
    xmtp::{
        identity::MlsCredential,
        message_contents::{signed_private_key, SignedPrivateKey as LegacySignedPrivateKeyProto},
    },
};
use xmtp_v2::k256_helper;

use crate::{
    api::{ApiClientWrapper, WrappedApiError},
    configuration::CIPHERSUITE,
    InboxOwner,
};

#[derive(Debug, Error)]
pub enum IdentityError {
    #[error(transparent)]
    CredentialSerialization(#[from] prost::EncodeError),
    #[error(transparent)]
    Decode(#[from] prost::DecodeError),
    #[error(transparent)]
    ApiError(#[from] WrappedApiError),
    #[error(transparent)]
    SignatureRequestBuilder(#[from] SignatureRequestError),
    #[error(transparent)]
    BasicCredential(#[from] BasicCredentialError),
    #[error("Legacy key re-use")]
    LegacyKeyReuse,
    #[error("Installation key {0}")]
    InstallationKey(String),
    #[error("Malformed legacy key: {0}")]
    MalformedLegacyKey(String),
    #[error("Legacy signature: {0}")]
    LegacySignature(String),
    #[error(transparent)]
    Crypto(#[from] CryptoError),
    #[error("legacy key does not match address")]
    LegacyKeyMismatch,
    #[error(transparent)]
    WalletError(#[from] WalletError),
}

#[derive(Debug, Clone)]
pub struct Identity {
    pub(crate) inbox_id: InboxId,
    pub(crate) installation_keys: SignatureKeyPair,
    pub(crate) credential: OpenMlsCredential,
    pub(crate) signature_request: Option<SignatureRequest>,
}

#[allow(dead_code)]
impl Identity {
    fn is_ready(&self) -> bool {
        self.signature_request.is_none()
    }

    /// Create a new [Identity] instance.
    ///
    /// If the address is already associated with an inbox_id, the existing inbox_id will be used.
    /// Users will be required to sign with their wallet, and the legacy is ignored even if it's provided.
    ///
    /// If the address is NOT associated with an inbox_id, a new inbox_id will be generated.
    /// Prioritize legacy key if provided, otherwise use wallet to sign.
    ///
    ///
    pub(crate) async fn new<ApiClient: XmtpMlsClient + XmtpIdentityClient>(
        address: String,
        legacy_signed_private_key: Option<Vec<u8>>,
        api_client: &ApiClientWrapper<ApiClient>,
    ) -> Result<Self, IdentityError> {
        // check if address is already associated with an inbox_id
        let inbox_ids = api_client.get_inbox_ids(vec![address.clone()]).await?;
        let associated_inbox_id = inbox_ids.get(&address);
        let signature_keys = SignatureKeyPair::new(CIPHERSUITE.signature_algorithm())?;
        let installation_public_key = signature_keys.public();
        let member_identifier: MemberIdentifier = address.clone().into();

        if let Some(associated_inbox_id) = associated_inbox_id {
            // If an inbox is associated, we just need to associate the installation key
            // Only wallet is allowed to sign the installation key
            let builder = SignatureRequestBuilder::new(associated_inbox_id.clone());
            let mut signature_request = builder
                .add_association(installation_public_key.to_vec().into(), member_identifier)
                .build();

            signature_request
                .add_signature(Box::new(
                    sign_with_installation_key(
                        signature_request.signature_text(),
                        sized_installation_key(signature_keys.private())?,
                    )
                    .await?,
                ))
                .await?;

            let identity = Self {
                inbox_id: associated_inbox_id.clone(),
                installation_keys: signature_keys,
                credential: create_credential(associated_inbox_id.clone())?,
                signature_request: Some(signature_request),
            };

            Ok(identity)
        } else if let Some(legacy_signed_private_key) = legacy_signed_private_key {
            // sanity check if address matches the one derived from legacy_signed_private_key
            let legacy_key_address = legacy_key_to_address(legacy_signed_private_key.clone())?;
            if address != legacy_key_address {
                return Err(IdentityError::LegacyKeyMismatch);
            }

            let nonce = 0;
            let inbox_id = generate_inbox_id(&address, &nonce);
            let mut builder = SignatureRequestBuilder::new(inbox_id.clone());
            builder = builder.create_inbox(member_identifier.clone(), nonce);
            let mut signature_request = builder
                .add_association(installation_public_key.to_vec().into(), member_identifier)
                .build();

            signature_request
                .add_signature(Box::new(
                    sign_with_installation_key(
                        signature_request.signature_text(),
                        sized_installation_key(signature_keys.private())?,
                    )
                    .await?,
                ))
                .await?;
            signature_request
                .add_signature(Box::new(
                    sign_with_legacy_key(
                        signature_request.signature_text(),
                        legacy_signed_private_key,
                    )
                    .await?,
                ))
                .await?;
            let identity_update = signature_request.build_identity_update()?;
            api_client.publish_identity_update(identity_update).await?;

            let identity = Self {
                inbox_id: inbox_id.clone(),
                installation_keys: signature_keys,
                credential: create_credential(inbox_id)?,
                signature_request: None,
            };

            Ok(identity)
        } else {
            let nonce = rand::random::<u64>();
            let inbox_id = generate_inbox_id(&address, &nonce);
            let mut builder = SignatureRequestBuilder::new(inbox_id.clone());
            builder = builder.create_inbox(member_identifier.clone(), nonce);

            let mut signature_request = builder
                .add_association(installation_public_key.to_vec().into(), member_identifier)
                .build();

            // We can pre-sign the request with an installation key signature, since we have access to the key
            signature_request
                .add_signature(Box::new(
                    sign_with_installation_key(
                        signature_request.signature_text(),
                        sized_installation_key(signature_keys.private())?,
                    )
                    .await?,
                ))
                .await?;

            let identity = Self {
                inbox_id: inbox_id.clone(),
                installation_keys: signature_keys,
                credential: create_credential(inbox_id.clone())?,
                signature_request: Some(signature_request),
            };

            Ok(identity)
        }
    }

    pub fn credential(&self) -> OpenMlsCredential {
        self.credential.clone()
    }
}

async fn sign_with_installation_key(
    signature_text: String,
    installation_private_key: &[u8; 32],
) -> Result<InstallationKeySignature, IdentityError> {
    let signing_key: SigningKey = SigningKey::from_bytes(installation_private_key);
    let verifying_key = signing_key.verifying_key();
    let mut prehashed: Sha512 = Sha512::new();
    prehashed.update(signature_text.clone());
    let sig = signing_key
        .sign_prehashed(prehashed, Some(INSTALLATION_KEY_SIGNATURE_CONTEXT))
        .unwrap();

    let installation_key_sig = InstallationKeySignature::new(
        signature_text.clone(),
        sig.to_vec(),
        verifying_key.as_bytes().to_vec(),
    );

    Ok(installation_key_sig)
}

/// Convert a legacy signed private key(secp256k1) to an address.
fn legacy_key_to_address(legacy_signed_private_key: Vec<u8>) -> Result<String, IdentityError> {
    let legacy_signed_private_key_proto =
        LegacySignedPrivateKeyProto::decode(legacy_signed_private_key.as_slice())?;
    let signed_private_key::Union::Secp256k1(secp256k1) = legacy_signed_private_key_proto
        .union
        .ok_or(IdentityError::MalformedLegacyKey(
            "Missing secp256k1.union field".to_string(),
        ))?;
    let legacy_private_key = secp256k1.bytes;
    let wallet: LocalWallet = LocalWallet::from_bytes(&legacy_private_key)?;
    Ok(wallet.get_address())
}

async fn sign_with_legacy_key(
    signature_text: String,
    legacy_signed_private_key: Vec<u8>,
) -> Result<LegacyDelegatedSignature, IdentityError> {
    let legacy_signed_private_key_proto =
        LegacySignedPrivateKeyProto::decode(legacy_signed_private_key.as_slice())?;
    let signed_private_key::Union::Secp256k1(secp256k1) = legacy_signed_private_key_proto
        .union
        .ok_or(IdentityError::MalformedLegacyKey(
            "Missing secp256k1.union field".to_string(),
        ))?;
    let legacy_private_key = secp256k1.bytes;
    let (mut delegating_signature, recovery_id) = k256_helper::sign_sha256(
        &legacy_private_key, // secret_key
        // TODO: Verify this will create a verifiable signature
        signature_text.as_bytes(), // message
    )
    .map_err(IdentityError::LegacySignature)?;
    delegating_signature.push(recovery_id); // TODO: normalize recovery ID if necessary

    let legacy_signed_public_key_proto =
        legacy_signed_private_key_proto
            .public_key
            .ok_or(IdentityError::MalformedLegacyKey(
                "Missing public_key field".to_string(),
            ))?;

    let recoverable_sig = RecoverableEcdsaSignature::new(signature_text, delegating_signature);

    Ok(LegacyDelegatedSignature::new(
        recoverable_sig,
        legacy_signed_public_key_proto,
    ))
}

fn sized_installation_key(installation_key: &[u8]) -> Result<&[u8; 32], IdentityError> {
    installation_key
        .try_into()
        .map_err(|e: TryFromSliceError| IdentityError::InstallationKey(e.to_string()))
}

fn create_credential(inbox_id: InboxId) -> Result<OpenMlsCredential, IdentityError> {
    let cred = MlsCredential { inbox_id };
    let mut credential_bytes = Vec::new();
    let _ = cred.encode(&mut credential_bytes);

    Ok(BasicCredential::new(credential_bytes).into())
}
