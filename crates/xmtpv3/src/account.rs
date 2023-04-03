use anyhow::Result;
use vodozemac::olm::{Account, AccountPickle};

use serde::{Deserialize, Serialize};

// Public identity for a voodoo account
// TODO: STARTINGTASK: Implement this correctly so it
// doesn't just serialize the entire account (which includes sensitive key material)
// NOTE: We only store
#[derive(Serialize, Deserialize)]
pub struct VoodooPublicIdentity {
    pub pickle: AccountPickle,
}

// Crappy stand-ins for a real serialization/deserialization implementation
impl VoodooPublicIdentity {
    // Create a new VoodooInstance
    pub fn new(account: &Account) -> Self {
        Self {
            pickle: account.pickle(),
        }
    }

    pub fn get_account(&self) -> Result<Account> {
        // Serialize and deserialize as a copy
        let json = serde_json::to_string(&self)?;
        let copy: Self = serde_json::from_str(&json)?;
        Ok(Account::from_pickle(copy.pickle))
    }

    pub fn from_json(json: &str) -> Self {
        serde_json::from_str(json).unwrap()
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(&self).unwrap()
    }
}
