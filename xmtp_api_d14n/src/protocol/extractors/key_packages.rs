use xmtp_proto::ConversionError;

use crate::protocol::Extractor;
use crate::protocol::traits::EnvelopeVisitor;
use xmtp_proto::xmtp::mls::api::v1::UploadKeyPackageRequest;
use xmtp_proto::xmtp::mls::api::v1::fetch_key_packages_response::KeyPackage;
use xmtp_proto::xmtp::xmtpv4::envelopes::ClientEnvelope;

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

    fn visit_client(&mut self, e: &ClientEnvelope) -> Result<(), Self::Error> {
        tracing::debug!("client: {:?}", e);
        Ok(())
    }

    fn visit_none(&mut self) -> Result<(), Self::Error> {
        // TODO: Handle empty key package response (when key package is None)
        Ok(())
    }

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
