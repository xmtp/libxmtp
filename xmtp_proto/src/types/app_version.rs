use tonic::metadata::{errors::InvalidMetadataValue, Ascii, MetadataValue};

/// An Inbox-App specific version
#[derive(Clone, PartialEq, Eq)]
pub struct AppVersion(String);

impl From<String> for AppVersion {
    fn from(value: String) -> Self {
        AppVersion(value)
    }
}

impl From<&str> for AppVersion {
    fn from(value: &str) -> Self {
        AppVersion(value.to_string())
    }
}

impl From<&String> for AppVersion {
    fn from(value: &String) -> Self {
        AppVersion(value.to_string())
    }
}

impl Default for AppVersion {
    fn default() -> Self {
        Self("0.0.0".to_string())
    }
}

impl TryFrom<AppVersion> for MetadataValue<Ascii> {
    type Error = InvalidMetadataValue;

    fn try_from(value: AppVersion) -> Result<Self, Self::Error> {
        MetadataValue::try_from(value.0)
    }
}

impl TryFrom<&AppVersion> for MetadataValue<Ascii> {
    type Error = InvalidMetadataValue;

    fn try_from(value: &AppVersion) -> Result<Self, Self::Error> {
        MetadataValue::try_from(&value.0)
    }
}

impl std::fmt::Display for AppVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::fmt::Debug for AppVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("AppVersion").field(&self.0).finish()
    }
}

impl PartialEq<String> for AppVersion {
    fn eq(&self, other: &String) -> bool {
        self.0 == *other
    }
}

impl PartialEq<AppVersion> for String {
    fn eq(&self, other: &AppVersion) -> bool {
        other.0 == *self
    }
}
