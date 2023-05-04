use base64::{engine::general_purpose, Engine as _};
use thiserror::Error;
use wasm_bindgen::JsValue;
use xmtp::persistence::Persistence;

#[derive(Error, Debug)]
pub enum LocalStoragePersistenceError {
    #[error("Failed to read/write from local storage")]
    ReadWriteError { native_error: JsValue },

    #[error("Failed to deserialize from local storage")]
    DeserializationError(#[from] base64::DecodeError),
}

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
    type Error = LocalStoragePersistenceError;

    fn write(&mut self, key: &str, value: &[u8]) -> Result<(), Self::Error> {
        let value = general_purpose::STANDARD.encode(value);
        self.storage()
            .set_item(key, &value)
            .map_err(|native_error| LocalStoragePersistenceError::ReadWriteError { native_error })
    }

    fn read(&self, key: &str) -> Result<Option<Vec<u8>>, Self::Error> {
        let value = self.storage().get_item(key).map_err(|native_error| {
            LocalStoragePersistenceError::ReadWriteError { native_error }
        })?;
        if value.is_none() {
            return Ok(None);
        }
        let value = value.unwrap();
        let value = general_purpose::STANDARD.decode(value)?;
        Ok(Some(value))
    }
}
