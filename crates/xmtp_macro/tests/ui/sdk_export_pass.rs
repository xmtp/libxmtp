uniffi::setup_scaffolding!();

#[xmtp_macro::callback_error]
#[derive(Debug, thiserror::Error, uniffi::Error)]
enum CallbackError {
    #[error("callback failed")]
    Failed,
}

impl From<uniffi::UnexpectedUniFFICallbackError> for CallbackError {
    fn from(_: uniffi::UnexpectedUniFFICallbackError) -> Self {
        Self::Failed
    }
}

#[derive(uniffi::Object)]
struct SyncApi;

#[xmtp_macro::sdk_export]
impl SyncApi {
    #[uniffi::constructor]
    pub fn new() -> Self {
        Self
    }

    pub fn value(&self) -> Result<u32, CallbackError> {
        Ok(42)
    }
}

#[derive(uniffi::Object)]
struct AsyncApi;

#[xmtp_macro::sdk_export]
impl AsyncApi {
    #[uniffi::constructor]
    pub fn new() -> Self {
        Self
    }

    pub async fn value(&self) -> Result<u32, CallbackError> {
        Ok(42)
    }
}

#[xmtp_macro::sdk_export]
pub fn answer() -> u32 {
    42
}

fn main() {
    assert_eq!(SyncApi::new().value().unwrap(), answer());
    let _ = AsyncApi::new();
}
