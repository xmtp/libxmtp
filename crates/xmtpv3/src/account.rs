use anyhow::Result;
use vodozemac::{
    olm::{Account, AccountPickle},
    Curve25519PublicKey,
};

use serde::{Deserialize, Serialize};

// Public bundle for a voodoo account
// Served from the client for now but should eventually be served by the server
// The one_time_key is hardcoded to the fallback key for now but should eventually rotate
// with each request to the server
#[derive(Serialize, Deserialize)]
pub struct VoodooContactBundlePickle {
    identity_key: Curve25519PublicKey,
    one_time_key: Curve25519PublicKey,
}

impl VoodooContactBundlePickle {
    // Create a new VoodooInstance
    pub fn new(account: &Account) -> Self {
        Self {
            identity_key: account.curve25519_key(),
            one_time_key: *account
                .fallback_key()
                .values()
                .next()
                .expect("Expecting an unpublished fallback key on the account for now"),
        }
    }

    pub fn identity_key(&self) -> Curve25519PublicKey {
        self.identity_key
    }

    pub fn one_time_key(&self) -> Curve25519PublicKey {
        self.one_time_key
    }

    pub fn from_json(json: &str) -> Self {
        serde_json::from_str(json).unwrap()
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(&self).unwrap()
    }
}

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
