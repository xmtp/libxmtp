#![allow(unused)]

use crate::builder::ClientBuilder;
use ethers::signers::LocalWallet;
use parking_lot::Mutex;
use passkey::{
    authenticator::{Authenticator, UserCheck, UserValidationMethod},
    client::{Client, DefaultClientData},
    types::{ctap2::*, rand::random_vec, webauthn::*, Bytes, Passkey},
};
use public_suffix::PublicSuffixList;
use std::{ops::Deref, sync::Arc};
use url::Url;
use xmtp_cryptography::{signature::SignatureError, utils::generate_local_wallet};
use xmtp_db::XmtpOpenMlsProvider;
use xmtp_id::{
    associations::{
        ident,
        unverified::{UnverifiedPasskeySignature, UnverifiedSignature},
        Identifier,
    },
    InboxOwner,
};

use super::FullXmtpClient;

/// A test client wrapper that auto-exposes all of the usual component access boilerplate.
/// Makes testing easier and less repetetive.
#[allow(dead_code)]
pub(crate) struct Tester<Owner> {
    pub owner: Owner,
    pub client: FullXmtpClient,
    pub provider: Arc<XmtpOpenMlsProvider>,
}

impl Tester<LocalWallet> {
    pub(crate) async fn new() -> Self {
        let wallet = generate_local_wallet();
        Self::new_from_owner(wallet).await
    }
}

impl Tester<PasskeyUser> {
    pub(crate) async fn new_passkey() -> Self {
        let passkey_user = PasskeyUser::new().await;
        Self::new_from_owner(passkey_user).await
    }
}

#[allow(dead_code)]
impl<Owner> Tester<Owner>
where
    Owner: InboxOwner + Clone + 'static,
{
    pub(crate) async fn clone(&self) -> Self {
        Self::new_from_owner(self.owner.clone()).await
    }

    pub(crate) async fn new_from_owner(owner: Owner) -> Self {
        let client = ClientBuilder::new_test_client(&owner).await;
        let provider = client.mls_provider().unwrap();

        Self {
            owner,
            client,
            provider: Arc::new(provider),
        }
    }
}

impl<Owner> Deref for Tester<Owner>
where
    Owner: InboxOwner,
{
    type Target = FullXmtpClient;

    fn deref(&self) -> &Self::Target {
        &self.client
    }
}

pub type PasskeyCredential = PublicKeyCredential<AuthenticatorAttestationResponse>;
pub type PasskeyClient = Client<Option<Passkey>, PkUserValidationMethod, PublicSuffixList>;

#[derive(Clone)]
pub struct PasskeyUser {
    origin: Url,
    pk_cred: Arc<PasskeyCredential>,
    pk_client: Arc<Mutex<PasskeyClient>>,
}

impl InboxOwner for PasskeyUser {
    fn sign(&self, text: &str) -> Result<UnverifiedSignature, SignatureError> {
        let text = text.as_bytes().to_vec();
        let sign_request = CredentialRequestOptions {
            public_key: PublicKeyCredentialRequestOptions {
                challenge: Bytes::from(text),
                timeout: None,
                rp_id: Some(String::from(self.origin.domain().unwrap())),
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
        let cred = futures_executor::block_on(cred).unwrap();
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
        xmtp_id::associations::Identifier,
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
        let mut pk_client = Client::new(pk_auth);

        let request = CredentialCreationOptions {
            public_key: PublicKeyCredentialCreationOptions {
                rp: PublicKeyCredentialRpEntity {
                    id: None, // Leaving the ID as None means use the effective domain
                    name: origin.domain().unwrap().into(),
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
            .unwrap();

        Self {
            pk_client: Arc::new(Mutex::new(pk_client)),
            pk_cred: Arc::new(pk_cred),
            origin,
        }
    }

    fn public_key(&self) -> Vec<u8> {
        self.pk_cred.response.public_key.as_ref().unwrap()[26..].to_vec()
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
        _credential: Option<&'a Passkey>,
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
