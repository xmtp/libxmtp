use base64::{engine::general_purpose, Engine as _};
use xmtp::persistence::Persistence;

pub struct LocalStoragePersistence {}

impl LocalStoragePersistence {
    pub fn new() -> Self {
        LocalStoragePersistence {}
    }

    fn storage(&self) -> web_sys::Storage {
        web_sys::window()
            .expect("Global Window not found - are you running in a browser?")
            .local_storage()
            .expect("Local Storage not found - are you running in a browser?")
            .expect("Window.localStorage not found - are you running in a browser?")
    }
}

impl Default for LocalStoragePersistence {
    fn default() -> Self {
        Self::new()
    }
}

impl Persistence for LocalStoragePersistence {
    fn write(&mut self, key: &str, value: &[u8]) -> Result<(), String> {
        let value = general_purpose::STANDARD.encode(value);
        self.storage()
            .set_item(key, &value)
            .expect("Failed to write to local storage");
        Ok(())
    }

    fn read(&self, key: &str) -> Result<Option<Vec<u8>>, String> {
        let value = self
            .storage()
            .get_item(key)
            .expect("Failed to read from local storage");
        if value.is_none() {
            return Ok(None);
        }
        let value = value.unwrap();
        let value = general_purpose::STANDARD.decode(value).unwrap();
        Ok(Some(value))
    }
}
