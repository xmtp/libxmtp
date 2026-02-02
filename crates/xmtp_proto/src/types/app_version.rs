use tonic::metadata::{Ascii, MetadataValue, errors::InvalidMetadataValue};

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

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;
    use tonic::metadata::MetadataValue;

    #[rstest]
    #[case("1.2.3")]
    #[case("2.0.1")]
    #[case("")]
    #[case("v1.0.0")]
    #[xmtp_common::test]
    async fn test_from_conversions(#[case] version: &str) {
        assert_eq!(AppVersion::from(version).to_string(), version);
        assert_eq!(AppVersion::from(version.to_string()).to_string(), version);
    }

    #[rstest]
    #[case("1.0.0", true)]
    #[case("", true)]
    #[case("1.0.0\u{1F600}", true)] // emoji - apparently allowed
    #[xmtp_common::test]
    async fn test_metadata_value_conversion(#[case] version: &str, #[case] should_succeed: bool) {
        assert_eq!(
            TryInto::<MetadataValue<Ascii>>::try_into(AppVersion::from(version).clone()).is_ok(),
            should_succeed
        );

        if should_succeed && version.is_ascii() {
            assert_eq!(
                TryInto::<MetadataValue<Ascii>>::try_into(AppVersion::from(version).clone())
                    .unwrap()
                    .to_str()
                    .unwrap(),
                version
            );
        }
    }

    #[rstest]
    #[case("1.0.0-alpha")]
    #[case("2.1.3-beta.1")]
    #[case("3.0.0-rc.2+build.123")]
    #[case("v1.2.3")]
    #[case("1.0.0+build.1")]
    #[xmtp_common::test]
    async fn test_complex_versions(#[case] version: &str) {
        assert_eq!(AppVersion::from(version).to_string(), version);
        assert!(
            TryInto::<MetadataValue<Ascii>>::try_into(&AppVersion::from(version)).is_ok(),
            "Version {} should be valid ASCII",
            version
        );
    }
}
