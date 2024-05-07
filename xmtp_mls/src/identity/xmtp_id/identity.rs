use std::array::TryFromSliceError;

use ed25519_dalek::SigningKey;
use openmls::{
    credentials::{errors::BasicCredentialError, BasicCredential},
    prelude::Credential as OpenMlsCredential,
};
use openmls_basic_credential::SignatureKeyPair;
use openmls_traits::types::CryptoError;
use prost::Message;
use rand::Rng;
use sha2::{Digest, Sha512};
use thiserror::Error;
use xmtp_id::{
    associations::{
        builder::{SignatureRequest, SignatureRequestBuilder, SignatureRequestError},
        InstallationKeySignature, LegacyDelegatedSignature, MemberIdentifier,
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
    api::{ApiClientWrapper, GetIdentityUpdatesV2Filter, WrappedApiError},
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

impl Identity {
    fn is_ready(&self) -> bool {
        self.signature_request.is_none()
    }

    pub(crate) async fn create_to_be_signed<ApiClient: XmtpMlsClient + XmtpIdentityClient>(
        inbox_id: InboxId,
        account_address: String,
        api_client: &ApiClientWrapper<ApiClient>,
    ) -> Result<Self, IdentityError> {
        let signature_keys = SignatureKeyPair::new(CIPHERSUITE.signature_algorithm())?;
        let installation_public_key = signature_keys.public();
        let member_identifier: MemberIdentifier = account_address.into();

        let mut builder = SignatureRequestBuilder::new(inbox_id.clone());

        if !Self::is_existing_inbox(inbox_id.clone(), api_client).await? {
            builder = builder.create_inbox(member_identifier.clone(), rand::thread_rng().gen());
        }

        let mut signature_request = builder
            .add_association(installation_public_key.to_vec().into(), member_identifier)
            .build();

        // We can pre-sign the request with an installation key signature, since we have access to the key
        signature_request
            .add_signature(Box::new(
                Self::sign_with_installation_key(
                    signature_request.signature_text(),
                    sized_installation_key(signature_keys.private())?,
                )
                .await?,
            ))
            .await?;

        let identity = Self {
            inbox_id: inbox_id.clone(),
            installation_keys: signature_keys,
            credential: create_credential(inbox_id)?,
            signature_request: Some(signature_request),
        };

        Ok(identity)
    }

    pub(crate) async fn create_from_legacy<ApiClient: XmtpMlsClient + XmtpIdentityClient>(
        inbox_id: InboxId,
        account_address: String, // TODO: we can derive account_address from the private_key, it can be removed.
        legacy_signed_private_key: Vec<u8>,
        api_client: &ApiClientWrapper<ApiClient>,
    ) -> Result<Self, IdentityError> {
        if Self::is_existing_inbox(inbox_id.clone(), api_client).await? {
            return Err(IdentityError::LegacyKeyReuse);
        }
        let signature_keys = SignatureKeyPair::new(CIPHERSUITE.signature_algorithm())?;
        let installation_public_key = signature_keys.public();
        let member_identifier: MemberIdentifier = account_address.into();
        let builder = SignatureRequestBuilder::new(inbox_id.clone());
        let mut signature_request = builder
            .create_inbox(member_identifier.clone(), 0)
            .add_association(installation_public_key.to_vec().into(), member_identifier)
            .build();

        // We can pre-sign the request with an installation key signature, since we have access to the key
        signature_request
            .add_signature(Box::new(
                Self::sign_with_installation_key(
                    signature_request.signature_text(),
                    sized_installation_key(signature_keys.private())?,
                )
                .await?,
            ))
            .await?;

        signature_request
            .add_signature(Box::new(
                Self::sign_with_legacy_key(
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

    async fn is_existing_inbox<ApiClient: XmtpMlsClient + XmtpIdentityClient>(
        inbox_id: InboxId,
        api_client: &ApiClientWrapper<ApiClient>,
    ) -> Result<bool, IdentityError> {
        let identity_updates = api_client
            .get_identity_updates_v2(vec![GetIdentityUpdatesV2Filter {
                inbox_id: inbox_id.to_string(),
                sequence_id: None,
            }])
            .await?;

        let inbox_updates = identity_updates.get(&inbox_id);

        Ok(inbox_updates.is_some())
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

    pub fn credential(&self) -> OpenMlsCredential {
        self.credential.clone()
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
            &signature_text.as_bytes(), // message
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
}

fn sized_installation_key(installation_key: &[u8]) -> Result<&[u8; 32], IdentityError> {
    Ok(installation_key
        .try_into()
        .map_err(|e: TryFromSliceError| IdentityError::InstallationKey(e.to_string()))?)
}

fn create_credential(inbox_id: InboxId) -> Result<OpenMlsCredential, IdentityError> {
    let cred = MlsCredential { inbox_id };
    let mut credential_bytes = Vec::new();
    cred.encode(&mut credential_bytes);

    Ok(BasicCredential::new(credential_bytes)?.into())
}
