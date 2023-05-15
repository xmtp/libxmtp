use crate::{
    association::{Association, AssociationText},
    types::Address,
    Signable,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use vodozemac::olm::{Account as OlmAccount, AccountPickle as OlmAccountPickle};
use xmtp_crypto::signature::{RecoverableSignature, SignatureError};

#[derive(Debug, Error)]
pub enum AccountError {
    #[error("bad signature")]
    BadSignature(#[from] SignatureError),
    #[error("Association text mismatch")]
    TextMismatch,
    #[error("unknown association error")]
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
        Self::new(OlmAccount::new())
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
    keys: VmacAccount,
    assoc: Association,
}

impl Account {
    pub fn new(keys: VmacAccount, assoc: Association) -> Self {
        // TODO: Validate Association on initialization

        Self { keys, assoc }
    }

    pub fn generate(sf: impl Fn(Vec<u8>) -> Association + 'static) -> Self {
        let keys = VmacAccount::generate();
        let bytes = keys.bytes_to_sign();
        Self::new(keys, sf(bytes))
    }

    pub fn addr(&self) -> Address {
        self.assoc.address()
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

    pub fn finalize(self, signature: Vec<u8>) -> Account {
        Account::new(
            self.key,
            Association {
                text: self.assoc_text,
                signature: RecoverableSignature::Eip191Signature(signature),
            },
        )
    }
}

impl Signable for AccountCreator {
    fn bytes_to_sign(&self) -> Vec<u8> {
        self.key.bytes_to_sign()
    }
}

#[cfg(test)]
mod tests {

    use ethers::core::rand::thread_rng;
    use ethers::signers::{LocalWallet, Signer};
    use ethers_core::types::{Address as EthAddress, Signature};
    use ethers_core::utils::hex;
    use serde_json::json;
    use xmtp_crypto::utils::rng;
    use crate::Signable;
    use super::{Account, AccountCreator, Association};

    pub fn test_wallet_signer(_: Vec<u8>) -> Association {
        Association::test().expect("Test Association failed to generate")
    }

    #[test]
    fn account_serialize() {
        let account = Account::generate(test_wallet_signer);
        let serialized_account = json!(account).to_string();
        let serialized_account_other = json!(account).to_string();

        assert_eq!(serialized_account, serialized_account_other);

        let recovered_account: Account = serde_json::from_str(&serialized_account).unwrap();
        assert_eq!(account.addr(), recovered_account.addr());
    }

    #[tokio::test]
    async fn account_generate() {
        let wallet = LocalWallet::new(&mut rng());
        let addr = wallet.address().to_string();

        let ac = AccountCreator::new(addr);
        let key_bytes = ac.bytes_to_sign();
        let msg = ac.text_to_sign();
        let sig = wallet
            .sign_message(msg)
            .await
            .expect("Bad Signature in test");
        let account = ac.finalize(sig.to_vec());

        // Ensure Account is valid
        assert_eq!(true, account.assoc.is_valid(&key_bytes).is_ok())
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
        // println!("Verified signature produced by {:?}!", wallet.address());
    }
}
