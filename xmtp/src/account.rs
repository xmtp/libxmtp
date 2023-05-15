use std::vec;

use serde::{Deserialize, Serialize};
use vodozemac::olm::{Account as OlmAccount, AccountPickle as OlmAccountPickle};

use crate::Signable;

type Address = String;

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub enum AssociationFormat {
    Eip191,
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub enum AssociationType {
    Static,
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct Association {
    pub addr: Address,
    pub key_bytes: String,
    pub text: String,
    pub format: AssociationFormat,
    pub proof: Vec<u8>,
    pub proof_type: AssociationType,
}

impl Association {
    pub fn test() -> Self {
        Self {
            addr: "ADDR".to_string(),
            key_bytes: "KEY_BYTES".to_string(),
            text: "TEXT".to_string(),
            format: AssociationFormat::Eip191,
            proof: vec![0, 2, 3, 5, 67],
            proof_type: AssociationType::Static,
        }
    }
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
        Self { keys, assoc }
    }

    pub fn generate(sf: impl Fn(Vec<u8>) -> Association + 'static) -> Self {
        let keys = VmacAccount::generate();
        let bytes = keys.bytes_to_sign();
        Self::new(keys, sf(bytes))
    }

    pub fn addr(&self) -> Address {
        self.assoc.addr.clone()
    }
}

pub struct AccountCreator {
    key: VmacAccount,
}

impl AccountCreator {
    pub fn new() -> Self {
        Self {
            key: VmacAccount::generate(),
        }
    }

    pub fn finalize_key(self, _sig: Vec<u8>) -> Account {
        Account::new(self.key, Association::test())
    }
}

impl Signable for AccountCreator {
    fn bytes_to_sign(&self) -> Vec<u8> {
        self.key.bytes_to_sign()
    }
}

pub fn test_wallet_signer(_: Vec<u8>) -> Association {
    Association::test()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::{account::Association, Signable};

    use super::{test_wallet_signer, Account, AccountCreator};

    #[test]
    fn account_serialize() {
        let account = Account::generate(test_wallet_signer);
        let serialized_account = json!(account).to_string();
        let serialized_account_other = json!(account).to_string();

        assert_eq!(serialized_account, serialized_account_other);

        let recovered_account: Account = serde_json::from_str(&serialized_account).unwrap();
        assert_eq!(account.addr(), recovered_account.addr());
    }

    #[test]
    fn account_generate() {
        let ac = AccountCreator::new();
        let _ = ac.bytes_to_sign();
        let account = ac.finalize_key(vec![11, 22, 33]);

        assert_eq!(account.assoc, Association::test())
    }
}
