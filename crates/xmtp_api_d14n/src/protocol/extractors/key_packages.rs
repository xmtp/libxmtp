use xmtp_proto::ConversionError;

use crate::protocol::Extractor;
use crate::protocol::traits::EnvelopeVisitor;
use xmtp_proto::xmtp::mls::api::v1::UploadKeyPackageRequest;
use xmtp_proto::xmtp::mls::api::v1::fetch_key_packages_response::KeyPackage;

/// Key Packages Extractor
/// This Extractor can be applied to multiple envelopes without losing state
#[derive(Default, Clone)]
pub struct KeyPackagesExtractor {
    key_packages: Vec<KeyPackage>,
}

impl Extractor for KeyPackagesExtractor {
    type Output = Vec<KeyPackage>;

    fn get(self) -> Self::Output {
        self.key_packages
    }
}

impl KeyPackagesExtractor {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn get(self) -> Vec<KeyPackage> {
        self.key_packages
    }
}

impl EnvelopeVisitor<'_> for KeyPackagesExtractor {
    type Error = ConversionError;

    fn visit_upload_key_package(
        &mut self,
        req: &UploadKeyPackageRequest,
    ) -> Result<(), Self::Error> {
        let key_package = req.key_package.as_ref().ok_or(ConversionError::Missing {
            item: "key_package",
            r#type: "OriginatorEnvelope",
        })?;
        self.key_packages.push(KeyPackage {
            key_package_tls_serialized: key_package.key_package_tls_serialized.clone(),
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::extractors::test_utils::*;
    use crate::protocol::{EnvelopeError, ProtocolEnvelope};

    #[xmtp_common::test]
    fn test_extract_kp() {
        let kp = xmtp_common::rand_vec::<32>();
        let envelope = TestEnvelopeBuilder::new()
            .with_key_package_custom(kp.clone())
            .build();
        let mut extractor = KeyPackagesExtractor::new();
        envelope.accept(&mut extractor).unwrap();
        let extracted_kp = extractor.get();
        assert_eq!(kp, extracted_kp[0].key_package_tls_serialized);
    }

    #[xmtp_common::test]
    fn extractor_errors_when_missing() {
        let envelope = TestEnvelopeBuilder::new()
            .with_invalid_key_package()
            .build();
        let mut extractor = KeyPackagesExtractor::new();
        let err = envelope.accept(&mut extractor).unwrap_err();
        assert!(matches!(
            err,
            EnvelopeError::Conversion(ConversionError::Missing { .. })
        ));
    }
}
