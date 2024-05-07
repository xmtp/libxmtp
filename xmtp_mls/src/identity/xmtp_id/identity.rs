use std::array::TryFromSliceError;

use ed25519_dalek::SigningKey;
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

    pub(crate) async fn create_to_be_signed<ApiClient: XmtpMlsClient + XmtpIdentityClient>(
        address: String,
        api_client: &ApiClientWrapper<ApiClient>,
    ) -> Result<Self, IdentityError> {
        // check if address is already associated with an inbox_id, generate a new inbox_id if not.
        let inbox_ids = api_client.get_inbox_ids(vec![address.clone()]).await?;
        let mut is_new_inbox = false;
        let mut nonce = 0;
        let inbox_id;
        if let Some(id) = inbox_ids.get(&address) {
            inbox_id = id.clone();
        } else {
            is_new_inbox = true;
            nonce = rand::random::<u64>();
            inbox_id = generate_inbox_id(&address, &nonce);
        }

        let signature_keys = SignatureKeyPair::new(CIPHERSUITE.signature_algorithm())?;
        let installation_public_key = signature_keys.public();
        let member_identifier: MemberIdentifier = address.into();

        let mut builder = SignatureRequestBuilder::new(inbox_id.clone());
        if is_new_inbox {
            builder = builder.create_inbox(member_identifier.clone(), nonce);
        }

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

    // Create new inbox using legacy key, will error if the address is already associated with an inbox
    pub(crate) async fn create_from_legacy<ApiClient: XmtpMlsClient + XmtpIdentityClient>(
        address: String,
        legacy_signed_private_key: Vec<u8>,
        api_client: &ApiClientWrapper<ApiClient>,
    ) -> Result<Self, IdentityError> {
        let inbox_ids = api_client.get_inbox_ids(vec![address.clone()]).await?;
        if inbox_ids.contains_key(&address) {
            return Err(IdentityError::LegacyKeyReuse);
        }
        let nonce = 0;
        let inbox_id = generate_inbox_id(&address, &nonce);

        let signature_keys = SignatureKeyPair::new(CIPHERSUITE.signature_algorithm())?;
        let installation_public_key = signature_keys.public();
        let member_identifier: MemberIdentifier = address.into();
        let builder = SignatureRequestBuilder::new(inbox_id.clone());
        let mut signature_request = builder
            .create_inbox(member_identifier.clone(), nonce)
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

#[allow(dead_code)]
fn sized_installation_key(installation_key: &[u8]) -> Result<&[u8; 32], IdentityError> {
    installation_key
        .try_into()
        .map_err(|e: TryFromSliceError| IdentityError::InstallationKey(e.to_string()))
}

#[allow(dead_code)]
fn create_credential(inbox_id: InboxId) -> Result<OpenMlsCredential, IdentityError> {
    let cred = MlsCredential { inbox_id };
    let mut credential_bytes = Vec::new();
    let _ = cred.encode(&mut credential_bytes);

    Ok(BasicCredential::new(credential_bytes)?.into())
}
