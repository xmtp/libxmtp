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
    fn write(&mut self, key: String, value: &[u8]) -> Result<(), String> {
        let value = String::from_utf8(value.to_vec()).unwrap();
        let key = format!("xmtp_{}", key);
        self.storage()
            .set_item(&key, &value)
            .expect("Failed to write to local storage");
        Ok(())
    }

    fn read(&self, key: String) -> Result<Option<Vec<u8>>, String> {
        let key = format!("xmtp_{}", key);
        let value = self
            .storage()
            .get_item(&key)
            .expect("Failed to read from local storage");
        if value.is_none() {
            return Ok(None);
        }
        let value = value.unwrap();
        Ok(Some(value.as_bytes().to_vec()))
    }
}
