// Borrowed wholesale from openmls cli
use std::collections::HashMap;

use ethers::signers::LocalWallet;
use openmls::prelude::{config::CryptoConfig, *};
use openmls_basic_credential::SignatureKeyPair;
use openmls_traits::OpenMlsProvider;
use prost::Message;
use xmtp_proto::xmtp::v3::message_contents::Eip191Association as Eip191AssociationProto;

use crate::{
    association::{AssociationText, Eip191Association},
    owner::InboxOwner,
};

use super::{openmls_rust_persistent_crypto::OpenMlsRustPersistentCrypto, serialize_any_hashmap};

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Identity {
    #[serde(
        serialize_with = "serialize_any_hashmap::serialize_hashmap",
        deserialize_with = "serialize_any_hashmap::deserialize_hashmap"
    )]
    pub(crate) kp: HashMap<Vec<u8>, KeyPackage>,
    pub(crate) credential_with_key: CredentialWithKey,
    pub(crate) signer: SignatureKeyPair,
}

impl Identity {
    pub(crate) fn new(
        ciphersuite: Ciphersuite,
        crypto: &OpenMlsRustPersistentCrypto,
        wallet: LocalWallet,
    ) -> Self {
        let signature_keys = SignatureKeyPair::new(ciphersuite.signature_algorithm()).unwrap();
        let id = Identity::get_identity(signature_keys.clone(), wallet);
        let credential = Credential::new(id, CredentialType::Basic).unwrap();
        let credential_with_key = CredentialWithKey {
            credential,
            signature_key: signature_keys.to_public_vec().into(),
        };
        signature_keys.store(crypto.key_store()).unwrap();

        let key_package = KeyPackage::builder()
            .build(
                CryptoConfig {
                    ciphersuite,
                    version: ProtocolVersion::default(),
                },
                crypto,
                &signature_keys,
                credential_with_key.clone(),
            )
            .unwrap();

        Self {
            kp: HashMap::from([(
                key_package
                    .hash_ref(crypto.crypto())
                    .unwrap()
                    .as_slice()
                    .to_vec(),
                key_package,
            )]),
            credential_with_key,
            signer: signature_keys,
        }
    }

    fn get_identity(signature_key_pair: SignatureKeyPair, wallet: LocalWallet) -> Vec<u8> {
        let pub_key = signature_key_pair.public();
        let wallet_address = wallet.get_address();
        let association_text = AssociationText::new_static(wallet_address, pub_key.to_vec());
        let signature = wallet
            .sign(&association_text.text())
            .expect("failed to sign");

        let association =
            Eip191Association::new(pub_key, association_text, signature).expect("bad signature");
        let association_proto: Eip191AssociationProto = association.into();
        let mut buf = Vec::new();
        association_proto
            .encode(&mut buf)
            .expect("failed to serialize");
        buf
    }

    /// Create an additional key package using the credential_with_key/signer bound to this identity
    pub fn add_key_package(
        &mut self,
        ciphersuite: Ciphersuite,
        crypto: &OpenMlsRustPersistentCrypto,
    ) -> KeyPackage {
        let key_package = KeyPackage::builder()
            .build(
                CryptoConfig {
                    ciphersuite,
                    version: ProtocolVersion::default(),
                },
                crypto,
                &self.signer,
                self.credential_with_key.clone(),
            )
            .unwrap();

        self.kp.insert(
            key_package
                .hash_ref(crypto.crypto())
                .unwrap()
                .as_slice()
                .to_vec(),
            key_package.clone(),
        );
        key_package
    }

    /// Get the plain identity as byte vector.
    pub fn identity(&self) -> &[u8] {
        self.credential_with_key.credential.identity()
    }
}

pub fn identity_to_wallet_address(identity: &[u8], pub_key: &[u8]) -> String {
    let proto_value = Eip191AssociationProto::decode(identity).expect("failed to deserialize");
    let association = Eip191Association::from_proto_with_expected_address(
        pub_key,
        proto_value.clone(),
        proto_value.wallet_address,
    )
    .expect("failed to validate identity signature");

    association.address()
}
