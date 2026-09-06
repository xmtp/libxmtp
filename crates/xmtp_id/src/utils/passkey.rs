use crate::{
    InboxOwner,
    associations::{
        Identifier, ident,
        unverified::{UnverifiedPasskeySignature, UnverifiedSignature},
    },
};
use parking_lot::Mutex;
use passkey::{
    authenticator::{Authenticator, UiHint, UserCheck, UserValidationMethod},
    client::{Client as PasskeyClient, DefaultClientData},
    types::{Bytes, Passkey, ctap2::*, rand::random_vec, webauthn::*},
};
use public_suffix::PublicSuffixList;
use std::sync::Arc;
use url::Url;
use xmtp_cryptography::signature::SignatureError;
pub type PKCredential = PublicKeyCredential<AuthenticatorAttestationResponse>;
pub type PKClient = PasskeyClient<Option<Passkey>, PkUserValidationMethod, PublicSuffixList, ()>;

#[derive(Clone)]
pub struct PasskeyUser {
    origin: Url,
    pk_cred: Arc<PKCredential>,
    pk_client: Arc<Mutex<PKClient>>,
}

impl InboxOwner for PasskeyUser {
    fn sign(&self, text: &str) -> Result<UnverifiedSignature, SignatureError> {
        let text = text.as_bytes().to_vec();
        let sign_request = CredentialRequestOptions {
            public_key: PublicKeyCredentialRequestOptions {
                challenge: Bytes::from(text),
                timeout: None,
                rp_id: Some(String::from(
                    self.origin.domain().expect("fixture origin has a domain"),
                )),
                allow_credentials: None,
                user_verification: UserVerificationRequirement::default(),
                hints: None,
                attestation: AttestationConveyancePreference::None,
                attestation_formats: None,
                extensions: None,
            },
        };

        let mut pk_client = self.pk_client.lock();

        let cred = pk_client.authenticate(self.origin.clone(), sign_request, DefaultClientData);
        let cred = futures_executor::block_on(cred).expect("fixture authentication succeeds");
        let resp = cred.response;

        let signature = resp.signature.to_vec();

        Ok(UnverifiedSignature::Passkey(UnverifiedPasskeySignature {
            public_key: self.public_key(),
            signature,
            authenticator_data: resp.authenticator_data.to_vec(),
            client_data_json: resp.client_data_json.to_vec(),
        }))
    }

    fn get_identifier(
        &self,
    ) -> Result<
        crate::associations::Identifier,
        xmtp_cryptography::signature::IdentifierValidationError,
    > {
        Ok(Identifier::Passkey(ident::Passkey {
            key: self.public_key(),
            relying_party: None,
        }))
    }
}

impl PasskeyUser {
    pub async fn new() -> Self {
        let origin = url::Url::parse("https://xmtp.chat").expect("Should parse");
        let parameters_from_rp = PublicKeyCredentialParameters {
            ty: PublicKeyCredentialType::PublicKey,
            alg: coset::iana::Algorithm::ES256,
        };
        let pk_user_entity = PublicKeyCredentialUserEntity {
            id: random_vec(32).into(),
            display_name: "Alex Passkey".into(),
            name: "apk@example.org".into(),
        };
        let pk_auth_store: Option<Passkey> = None;
        let pk_aaguid = Aaguid::new_empty();
        let pk_user_validation_method = PkUserValidationMethod {};
        let pk_auth = Authenticator::new(pk_aaguid, pk_auth_store, pk_user_validation_method);
        let mut pk_client = PasskeyClient::new(pk_auth);

        let request = CredentialCreationOptions {
            public_key: PublicKeyCredentialCreationOptions {
                rp: PublicKeyCredentialRpEntity {
                    id: None, // Leaving the ID as None means use the effective domain
                    name: origin.domain().expect("fixture origin has a domain").into(),
                },
                user: pk_user_entity,
                // We're not passing a challenge here because we don't care about the credential and the user_entity behind it (for now).
                // It's guaranteed to be unique, and that's good enough for us.
                // All we care about is if that unique credential signs below.
                challenge: Bytes::from(vec![]),
                pub_key_cred_params: vec![parameters_from_rp],
                timeout: None,
                exclude_credentials: None,
                authenticator_selection: None,
                hints: None,
                attestation: AttestationConveyancePreference::None,
                attestation_formats: None,
                extensions: None,
            },
        };

        // Now create the credential.
        let pk_cred = pk_client
            .register(origin.clone(), request, DefaultClientData)
            .await
            .expect("fixture registration succeeds");

        Self {
            pk_client: Arc::new(Mutex::new(pk_client)),
            pk_cred: Arc::new(pk_cred),
            origin,
        }
    }

    fn public_key(&self) -> Vec<u8> {
        self.pk_cred
            .response
            .public_key
            .as_ref()
            .expect("fixture registration returns a key")[26..]
            .to_vec()
    }

    pub fn identifier(&self) -> Identifier {
        Identifier::Passkey(ident::Passkey {
            key: self.public_key(),
            relying_party: self.origin.domain().map(str::to_string),
        })
    }
}

pub struct PkUserValidationMethod {}
#[async_trait::async_trait]
impl UserValidationMethod for PkUserValidationMethod {
    type PasskeyItem = Passkey;
    async fn check_user<'a>(
        &self,
        _hint: UiHint<'a, Passkey>,
        presence: bool,
        verification: bool,
    ) -> Result<UserCheck, Ctap2Error> {
        Ok(UserCheck {
            presence,
            verification,
        })
    }

    fn is_verification_enabled(&self) -> Option<bool> {
        Some(true)
    }

    fn is_presence_enabled(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::associations::test_utils::MockSmartContractSignatureVerifier;

    #[xmtp_common::test(unwrap_try = true)]
    async fn passkey_fixture_binds_the_signed_challenge() {
        let user = PasskeyUser::new().await;
        let verifier = MockSmartContractSignatureVerifier::new(false);
        let signature = user.sign("expected challenge")?;
        signature
            .to_verified("expected challenge", &verifier)
            .await?;
        assert!(
            signature
                .to_verified("different challenge", &verifier)
                .await
                .is_err()
        );
    }
}
