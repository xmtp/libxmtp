use std::fmt;
use std::sync::{Mutex, MutexGuard};

use crate::{
    association::{AssociationError, Eip191Association},
    contact::Contact,
    types::Address,
    vmac_protos::ProtoWrapper,
    Signable,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use vodozemac::olm::{
    Account as OlmAccount, AccountPickle as OlmAccountPickle, IdentityKeys, InboundCreationResult,
    PreKeyMessage, Session as OlmSession, SessionConfig, SessionCreationError,
};
use vodozemac::Ed25519Signature;
use xmtp_cryptography::signature::SignatureError;
use xmtp_proto::xmtp::v3::message_contents::{
    installation_contact_bundle::Version, vmac_account_linked_key::Association as AssociationProto,
    InstallationContactBundle, VmacAccountLinkedKey, VmacInstallationLinkedKey,
    VmacInstallationPublicKeyBundleV1, VmacUnsignedPublicKey,
};

#[derive(Debug, Error)]
pub enum AccountError {
    #[error("session creation")]
    SessionCreation(#[from] SessionCreationError),
    #[error("generating new account")]
    BadGeneration(#[from] SignatureError),
    #[error("bad association")]
    BadAssocation(#[from] AssociationError),
    #[error("mutex poisoned error")]
    MutexPoisoned,
}

/// Holds an account and adds some serialization methods on top
pub struct VmacAccount {
    account: OlmAccount,
}

impl VmacAccount {
    /// Create a new instance
    pub fn new(account: OlmAccount) -> Self {
        Self { account }
    }

    pub fn generate() -> Self {
        let mut acc = OlmAccount::new();
        acc.generate_fallback_key();
        Self::new(acc)
    }

    pub fn get(&self) -> &OlmAccount {
        &self.account
    }

    pub fn get_mut(&mut self) -> &mut OlmAccount {
        &mut self.account
    }
}

impl Signable for VmacAccount {
    fn bytes_to_sign(&self) -> Vec<u8> {
        self.account.curve25519_key().to_vec()
    }
}

// Implement Serialize trait for VmacAccount
impl Serialize for VmacAccount {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        let pickle = self.account.pickle();
        pickle.serialize(serializer)
    }
}

// Implement Deserialize trait for VmacAccount
impl<'de> Deserialize<'de> for VmacAccount {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        let pickle: OlmAccountPickle = Deserialize::deserialize(deserializer)?;
        let account = OlmAccount::from_pickle(pickle);

        Ok(Self::new(account))
    }
}

#[derive(Serialize, Deserialize)]
pub struct Account {
    pub(crate) keys: Mutex<VmacAccount>,
    pub(crate) assoc: Eip191Association,
}

impl fmt::Debug for Account {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let keys = self.olm_account().map_err(|_| fmt::Error)?;
        let key_str = keys.get().curve25519_key().to_string();

        f.debug_struct("Point")
            .field("addr", &self.assoc.address())
            .field("key", &key_str)
            .finish()
    }
}

impl Account {
    pub fn new(keys: VmacAccount, assoc: Eip191Association) -> Self {
        // TODO: Validate Association on initialization

        Self {
            keys: Mutex::new(keys),
            assoc,
        }
    }

    pub fn generate(
        sf: impl Fn(Vec<u8>) -> Result<Eip191Association, AssociationError>,
    ) -> Result<Self, AccountError> {
        let keys = VmacAccount::generate();
        let bytes = keys.bytes_to_sign();
        let assoc = sf(bytes)?;
        Ok(Self::new(keys, assoc))
    }

    pub fn addr(&self) -> Address {
        self.assoc.address()
    }

    pub fn olm_account(&self) -> Result<MutexGuard<'_, VmacAccount>, AccountError> {
        self.keys.lock().map_err(|_| AccountError::MutexPoisoned)
    }

    pub fn identity_keys(&self) -> IdentityKeys {
        self.olm_account().unwrap().get().identity_keys()
    }

    pub fn contact(&self) -> Contact {
        let olm_account = self.olm_account().unwrap();
        let keys = olm_account.get();
        let identity_key = keys.curve25519_key();
        let fallback_key = keys.fallback_key().values().next().unwrap().to_owned();

        let identity_key_proto: ProtoWrapper<VmacUnsignedPublicKey> = identity_key.into();
        let fallback_key_proto: ProtoWrapper<VmacUnsignedPublicKey> = fallback_key.into();
        let identity_key = VmacAccountLinkedKey {
            key: Some(identity_key_proto.proto),
            association: Some(AssociationProto::Eip191(self.assoc.clone().into())),
        };
        let fallback_key = VmacInstallationLinkedKey {
            key: Some(fallback_key_proto.proto),
        };
        let contact = Contact::new(
            InstallationContactBundle {
                version: Some(Version::V1(VmacInstallationPublicKeyBundleV1 {
                    identity_key: Some(identity_key),
                    fallback_key: Some(fallback_key),
                })),
            },
            self.assoc.address(),
        );

        if let Err(e) = contact {
            panic!("Fatal: Client Owning Account has an invalid contact. Client cannot continue operating: {}", e);
        } else {
            contact.unwrap()
        }
    }

    pub(crate) fn sign(&self, message: &str) -> Ed25519Signature {
        self.olm_account().unwrap().get().sign(message)
    }

    pub fn create_outbound_session(&self, contact: &Contact) -> OlmSession {
        self.olm_account().unwrap().get().create_outbound_session(
            SessionConfig::version_2(),
            contact.vmac_identity_key(),
            contact.vmac_fallback_key(),
        )
    }

    pub fn create_inbound_session(
        &self,
        contact: &Contact,
        pre_key_message: PreKeyMessage,
    ) -> Result<InboundCreationResult, AccountError> {
        // TODO: Save the account keys to the store
        let mut olm_account = self.olm_account().unwrap();
        let keys = olm_account.get_mut();
        let res = keys.create_inbound_session(contact.vmac_identity_key(), &pre_key_message)?;

        Ok(res)
    }
}

#[cfg(test)]
pub(crate) mod tests {

    use crate::association::AssociationError;

    use super::{Account, Eip191Association};
    use ethers::core::rand::thread_rng;
    use ethers::signers::{LocalWallet, Signer};
    use ethers_core::types::{Address as EthAddress, Signature};
    use ethers_core::utils::hex;
    use serde_json::json;

    pub fn test_wallet_signer(pub_key: Vec<u8>) -> Result<Eip191Association, AssociationError> {
        Eip191Association::test(pub_key)
    }

    #[test]
    fn account_serialize() {
        let account = Account::generate(test_wallet_signer).unwrap();
        let serialized_account = json!(account).to_string();
        let serialized_account_other = json!(account).to_string();

        assert_eq!(serialized_account, serialized_account_other);

        let recovered_account: Account = serde_json::from_str(&serialized_account).unwrap();
        assert_eq!(account.addr(), recovered_account.addr());
    }

    async fn generate_random_signature(msg: &str) -> (String, Vec<u8>) {
        let wallet = LocalWallet::new(&mut thread_rng());
        let signature = wallet.sign_message(msg).await.unwrap();
        (
            hex::encode(wallet.address().to_fixed_bytes()),
            signature.to_vec(),
        )
    }

    #[tokio::test]
    async fn local_sign() {
        let msg = "hello";

        let (addr, bytes) = generate_random_signature(msg).await;
        let (other_addr, _) = generate_random_signature(msg).await;

        let signature = Signature::try_from(bytes.as_slice()).unwrap();
        let wallet_addr = hex::decode(addr).unwrap();
        let other_wallet_addr = hex::decode(other_addr).unwrap();

        assert!(signature
            .verify(msg, EthAddress::from_slice(&wallet_addr))
            .is_ok());
        assert!(signature
            .verify(msg, EthAddress::from_slice(&other_wallet_addr))
            .is_err());
    }
}
