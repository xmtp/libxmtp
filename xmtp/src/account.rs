use crate::{
    association::{Association, AssociationError, AssociationText},
    contact::Contact,
    session::Session,
    types::Address,
    vmac_protos::ProtoWrapper,
    Signable,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use vodozemac::olm::{Account as OlmAccount, AccountPickle as OlmAccountPickle, IdentityKeys};
use xmtp_cryptography::signature::{RecoverableSignature, SignatureError};
use xmtp_proto::xmtp::v3::message_contents::{
    VmacAccountLinkedKey, VmacContactBundle, VmacDeviceLinkedKey, VmacUnsignedPublicKey,
};

#[derive(Debug, Error)]
pub enum AccountError {
    #[error("generating new account")]
    BadGeneration(#[from] SignatureError),
    #[error("bad association")]
    BadAssocation(#[from] AssociationError),
    #[error("unknown error")]
    Unknown,
}

pub struct VmacAccount {
    account: OlmAccount,
}

// Struct that holds an account and adds some serialization methods on top
impl VmacAccount {
    // Create a new instance
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
    pub(crate) keys: VmacAccount,
    pub(crate) assoc: Association,
}

impl Account {
    pub fn new(keys: VmacAccount, assoc: Association) -> Self {
        // TODO: Validate Association on initialization

        Self { keys, assoc }
    }

    pub fn generate(
        sf: impl Fn(Vec<u8>) -> Result<Association, AssociationError>,
    ) -> Result<Self, AccountError> {
        let keys = VmacAccount::generate();
        let bytes = keys.bytes_to_sign();

        let assoc = sf(bytes)?;
        Ok(Self::new(keys, assoc))
    }

    pub fn addr(&self) -> Address {
        self.assoc.address()
    }

    pub fn contact(&self) -> Contact {
        let identity_key = self.keys.get().curve25519_key();
        let fallback_key = self
            .keys
            .get()
            .fallback_key()
            .values()
            .next()
            .unwrap()
            .to_owned();

        let identity_key_proto: ProtoWrapper<VmacUnsignedPublicKey> = identity_key.into();
        let fallback_key_proto: ProtoWrapper<VmacUnsignedPublicKey> = fallback_key.into();
        let identity_key = VmacAccountLinkedKey {
            key: Some(identity_key_proto.proto),
        };
        let fallback_key = VmacDeviceLinkedKey {
            key: Some(fallback_key_proto.proto),
        };
        // TODO: Add associations here
        Contact::new(VmacContactBundle {
            identity_key: Some(identity_key),
            prekey: Some(fallback_key),
        })
    }

    pub fn create_outbound_session(&self, contact: Contact) -> Session {
        let vmac_session = self.keys.get().create_outbound_session(
            SessionConfig::version_2(),
            contact.identity_key(),
            contact.fallback_key(),
        );

        Session::new(vmac_session)
    }

    pub fn get_keys(&self) -> IdentityKeys {
        self.keys.account.identity_keys()
    }
}

pub struct AccountCreator {
    key: VmacAccount,
    assoc_text: AssociationText,
}

impl AccountCreator {
    pub fn new(addr: Address) -> Self {
        let key = VmacAccount::generate();
        let key_bytes = key.bytes_to_sign();
        Self {
            key,
            assoc_text: AssociationText::new_static(addr, key_bytes),
        }
    }

    pub fn text_to_sign(&self) -> String {
        self.assoc_text.text()
    }

    pub fn finalize(self, signature: Vec<u8>) -> Result<Account, AccountError> {
        let assoc = Association::new(
            &self.key.bytes_to_sign(),
            self.assoc_text,
            RecoverableSignature::Eip191Signature(signature),
        )
        .map_err(AccountError::BadAssocation)?;
        Ok(Account::new(self.key, assoc))
    }
}

impl Signable for AccountCreator {
    fn bytes_to_sign(&self) -> Vec<u8> {
        self.key.bytes_to_sign()
    }
}

#[cfg(test)]
mod tests {

    use crate::association::AssociationError;

    use super::{Account, AccountCreator, Association};
    use ethers::core::rand::thread_rng;
    use ethers::signers::{LocalWallet, Signer};
    use ethers_core::types::{Address as EthAddress, Signature};
    use ethers_core::utils::hex;
    use serde_json::json;
    use xmtp_cryptography::{signature::h160addr_to_string, utils::rng};

    pub fn test_wallet_signer(_: Vec<u8>) -> Result<Association, AssociationError> {
        Association::test()
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

    #[tokio::test]
    async fn account_generate() {
        let wallet = LocalWallet::new(&mut rng());
        let addr = h160addr_to_string(wallet.address());

        let ac = AccountCreator::new(addr);
        let msg = ac.text_to_sign();
        let sig = wallet
            .sign_message(msg)
            .await
            .expect("Bad Signature in test");
        assert!(ac.finalize(sig.to_vec()).is_ok());
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
