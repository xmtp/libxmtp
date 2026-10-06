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

    /// A value that never changes for the object's lifetime.
    #[sdk(immutable)]
    pub fn fixed(&self) -> u32 {
        42
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

// The markers travel as doc attributes, so the stock UniFFI derives accept them.
#[xmtp_macro::sdk_export]
#[derive(Clone, Copy, Debug, uniffi::Enum)]
pub enum EventKind {
    #[sdk(kind = "conversation.joined")]
    ConversationJoined,
    #[sdk(kind = "lagged")]
    Lagged,
}

#[xmtp_macro::sdk_export(native_only)]
#[derive(Clone, uniffi::Enum)]
pub enum NotificationChannel {
    Apns {
        #[sdk(redact)]
        token: String,
    },
}

#[xmtp_macro::sdk_export]
#[derive(Clone, uniffi::Record)]
pub struct EncodedContent {
    #[sdk(shown)]
    pub fallback: Option<String>,
    #[sdk(redact = "secret")]
    pub parameters: std::collections::HashMap<String, String>,
    #[cfg(not(target_arch = "wasm32"))]
    #[sdk(redact)]
    pub key: Option<Vec<u8>>,
}

#[derive(uniffi::Object)]
struct Probe;

#[xmtp_macro::sdk_export]
impl Probe {
    /// A live read the browser worker makes itself. @xmtp-worker
    pub fn entered(&self) -> bool {
        false
    }
}

fn main() {
    assert_eq!(SyncApi::new().value().unwrap(), answer());
    assert_eq!(SyncApi::new().fixed(), answer());
    let _ = AsyncApi::new();
    let _ = EventKind::Lagged;
    let _ = NotificationChannel::Apns {
        token: String::new(),
    };
    assert!(!Probe.entered());
    let content = EncodedContent {
        fallback: None,
        parameters: Default::default(),
        key: None,
    };
    assert!(content.parameters.is_empty() && content.fallback.is_none());
}
