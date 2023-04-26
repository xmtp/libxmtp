use crate::{account::VmacAccount, persistence::Persistence};
use serde_json;
use vodozemac::olm::Account;

#[derive(Clone, Copy)]
pub enum Network {
    Local(&'static str),
    Dev,
    Prod,
}

impl Default for Network {
    fn default() -> Self {
        Network::Dev
    }
}

pub struct Client<P>
where
    P: Persistence,
{
    pub network: Network,
    pub persistence: P,
}

impl<P: Persistence> Client<P> {
    pub fn write_to_persistence(&mut self, s: String, b: &[u8]) -> Result<(), String> {
        self.persistence.write(s, b)
    }

    pub fn get_or_create_account(
        mut persistence: P,
        wallet_address: String,
    ) -> Result<VmacAccount, String> {
        let key = get_account_storage_key(wallet_address);
        let existing = persistence.read(key.clone());
        match existing {
            Ok(Some(data)) => {
                let data_string = std::str::from_utf8(&data).map_err(|e| format!("{}", e))?;
                let account: VmacAccount =
                    serde_json::from_str(data_string).map_err(|e| format!("{}", e))?;
                Ok(account)
            }
            Ok(None) => {
                let account = VmacAccount::generate();
                let data = serde_json::to_string(&account).map_err(|e| format!("{}", e))?;
                persistence.write(key, data.as_bytes())?;

                Ok(account)
            }
            Err(e) => Err(format!("Failed to read from persistence: {}", e)),
        }
    }

    pub fn read_from_persistence(&self, s: String) -> Result<Option<Vec<u8>>, String> {
        self.persistence.read(s)
    }
}

pub fn get_account_storage_key(wallet_address: String) -> String {
    format!("account_{}", wallet_address)
}
