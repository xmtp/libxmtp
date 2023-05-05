use serde::{Deserialize, Serialize};
use vodozemac::olm::{Account, AccountPickle};

pub struct VmacAccount {
    pub account: Account,
}

// Struct that holds an account and adds some serialization methods on top
impl VmacAccount {
    // Create a new instance
    pub fn new(account: Account) -> Self {
        Self { account }
    }

    pub fn generate() -> Self {
        let mut acc = Account::new();
        acc.generate_fallback_key();
        Self::new(acc) 
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
        let pickle: AccountPickle = Deserialize::deserialize(deserializer)?;
        let account = Account::from_pickle(pickle);
        Ok(Self::new(account))
    }
}
