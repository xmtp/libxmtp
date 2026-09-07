use super::{WelcomePointersExtension, WrapperAlgorithm, WrapperEncryptionExtension};
use openmls::{
    credentials::{Credential, CredentialWithKey},
    extensions::{
        ApplicationIdExtension, Extension, ExtensionType, Extensions, LastResortExtension,
    },
    key_packages::{KeyPackage, KeyPackageBundle, Lifetime},
    messages::proposals::ProposalType,
    prelude::{Capabilities, HpkeKeyPair, LeafNode},
};
use openmls_traits::OpenMlsProvider;
use xmtp_configuration::{
    CIPHERSUITE, GROUP_MEMBERSHIP_EXTENSION_ID, GROUP_PERMISSIONS_EXTENSION_ID,
    MUTABLE_METADATA_EXTENSION_ID, WELCOME_POINTEE_ENCRYPTION_AEAD_TYPES_EXTENSION_ID,
    WELCOME_WRAPPER_ENCRYPTION_EXTENSION_ID,
};
use xmtp_cryptography::{
    GeneratePostQuantumKeyError, XmtpInstallationCredential, generate_post_quantum_key,
};

#[derive(Debug, thiserror::Error)]
pub enum KeyPackageConstructionError {
    #[error(transparent)]
    Generation(#[from] openmls::key_packages::errors::KeyPackageNewError),
    #[error(transparent)]
    InvalidExtension(#[from] openmls::prelude::InvalidExtensionError),
    #[error(transparent)]
    Encode(#[from] prost::EncodeError),
    #[error(transparent)]
    PostQuantum(#[from] GeneratePostQuantumKeyError),
}

/// Options for construction. These do not change admission policy.
pub struct KeyPackageOptions {
    pub include_post_quantum: bool,
    pub welcome_pointers: bool,
    pub app_data_dictionary: bool,
    pub lifetime: Option<Lifetime>,
}
impl Default for KeyPackageOptions {
    fn default() -> Self {
        Self {
            include_post_quantum: false,
            welcome_pointers: true,
            app_data_dictionary: true,
            lifetime: None,
        }
    }
}
pub struct GeneratedKeyPackage {
    pub bundle: KeyPackageBundle,
    pub post_quantum_keypair: Option<HpkeKeyPair>,
}
pub fn build_post_quantum_public_key_extension(
    public_key: &[u8],
) -> Result<Extension, prost::EncodeError> {
    WrapperEncryptionExtension::new(WrapperAlgorithm::XWingMLKEM768Draft6, public_key.to_vec())
        .try_into()
}

/// Build a package with the supplied provider. Client bookkeeping is separate.
pub fn build_key_package(
    inbox_id: &str,
    credential: Credential,
    installation_keys: &XmtpInstallationCredential,
    provider: &impl OpenMlsProvider,
    options: KeyPackageOptions,
) -> Result<GeneratedKeyPackage, KeyPackageConstructionError> {
    let last_resort = Extension::LastResort(LastResortExtension::default());
    let welcome_pointee_encryption_aead_types =
        WelcomePointersExtension::available_types().try_into()?;
    let mut extensions = vec![last_resort, welcome_pointee_encryption_aead_types];
    if !options.welcome_pointers {
        extensions.pop();
    }
    let mut post_quantum_keypair = None;
    if options.include_post_quantum {
        let keypair = generate_post_quantum_key()?;
        extensions.push(build_post_quantum_public_key_extension(&keypair.public)?);
        post_quantum_keypair = Some(keypair);
    }
    let key_package_extensions = Extensions::from_vec(extensions)?;

    let application_id = Extension::ApplicationId(ApplicationIdExtension::new(inbox_id.as_bytes()));
    let leaf_node_extensions = Extensions::<LeafNode>::single(application_id)?;

    let mut capability_extensions = vec![
        ExtensionType::LastResort,
        ExtensionType::ApplicationId,
        ExtensionType::ImmutableMetadata,
        // Default capabilities let clients join groups that use AppDataUpdate.
        ExtensionType::AppDataDictionary,
        ExtensionType::Unknown(GROUP_PERMISSIONS_EXTENSION_ID),
        ExtensionType::Unknown(MUTABLE_METADATA_EXTENSION_ID),
        ExtensionType::Unknown(GROUP_MEMBERSHIP_EXTENSION_ID),
        ExtensionType::Unknown(WELCOME_WRAPPER_ENCRYPTION_EXTENSION_ID),
        ExtensionType::Unknown(WELCOME_POINTEE_ENCRYPTION_AEAD_TYPES_EXTENSION_ID),
    ];
    // Fixtures can model clients that do not support AppDataDictionary.
    if !options.app_data_dictionary {
        capability_extensions.retain(|e| *e != ExtensionType::AppDataDictionary);
    }
    // Defaults preserve both proposal capabilities advertised by clients.
    let capabilities = Capabilities::new(
        None,
        Some(&[CIPHERSUITE]),
        Some(&capability_extensions),
        Some(&[
            ProposalType::GroupContextExtensions,
            ProposalType::AppDataUpdate,
        ]),
        None,
    );

    let kp_builder = KeyPackage::builder()
        .leaf_node_capabilities(capabilities)
        .leaf_node_extensions(leaf_node_extensions)
        .key_package_extensions(key_package_extensions);

    let kp_builder = if let Some(lifetime) = options.lifetime {
        kp_builder.key_package_lifetime(lifetime)
    } else {
        kp_builder
    };

    let kp = kp_builder.build(
        CIPHERSUITE,
        provider,
        installation_keys,
        CredentialWithKey {
            credential,
            signature_key: installation_keys.public_slice().into(),
        },
    )?;

    Ok(GeneratedKeyPackage {
        bundle: kp,
        post_quantum_keypair,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key_package::{VerifiedKeyPackageV2, create_credential};
    use openmls::prelude::tls_codec::Serialize;
    use openmls_rust_crypto::OpenMlsRustCrypto;

    #[xmtp_common::test(unwrap_try = true)]
    fn generated_package_preserves_options_and_verifies() {
        for include_post_quantum in [false, true] {
            for capabilities in [false, true] {
                let provider = OpenMlsRustCrypto::default();
                let key = XmtpInstallationCredential::new();
                let generated = build_key_package(
                    "inbox",
                    create_credential("inbox"),
                    &key,
                    &provider,
                    KeyPackageOptions {
                        include_post_quantum,
                        welcome_pointers: capabilities,
                        app_data_dictionary: capabilities,
                        lifetime: Some(Lifetime::new(3600)),
                    },
                )?;
                let bytes = generated.bundle.key_package().tls_serialize_detached()?;
                let verified = VerifiedKeyPackageV2::from_bytes(provider.crypto(), &bytes)?;
                assert_eq!(verified.credential.inbox_id, "inbox");
                assert_eq!(verified.installation_public_key, key.public_slice());
                let leaf = verified.inner.leaf_node();
                assert_eq!(
                    leaf.capabilities()
                        .extensions()
                        .contains(&ExtensionType::AppDataDictionary),
                    capabilities
                );
                assert!(
                    leaf.capabilities()
                        .proposals()
                        .contains(&ProposalType::AppDataUpdate)
                );
                assert!(
                    leaf.capabilities()
                        .proposals()
                        .contains(&ProposalType::GroupContextExtensions)
                );
                let pointer = verified
                    .inner
                    .extensions()
                    .unknown(WELCOME_POINTEE_ENCRYPTION_AEAD_TYPES_EXTENSION_ID);
                assert_eq!(pointer.is_some(), capabilities);
                let wrapper = verified.wrapper_encryption()?;
                assert_eq!(wrapper.is_some(), include_post_quantum);
                if let Some(pair) = generated.post_quantum_keypair {
                    assert_eq!(wrapper.unwrap().pub_key_bytes, pair.public);
                    assert!(!pair.private.is_empty());
                }
            }
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn credential_shape_is_not_a_key_package_admission_rule() {
        let provider = OpenMlsRustCrypto::default();
        let key = XmtpInstallationCredential::new();
        for inbox in ["", "not-a-hex-inbox"] {
            let generated = build_key_package(
                inbox,
                create_credential(inbox),
                &key,
                &provider,
                Default::default(),
            )?;
            let bytes = generated.bundle.key_package().tls_serialize_detached()?;
            assert_eq!(
                VerifiedKeyPackageV2::from_bytes(provider.crypto(), &bytes)?
                    .credential
                    .inbox_id,
                inbox
            );
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn configured_expired_lifetime_still_fails_verification() {
        let provider = OpenMlsRustCrypto::default();
        let key = XmtpInstallationCredential::new();
        let generated = build_key_package(
            "inbox",
            create_credential("inbox"),
            &key,
            &provider,
            KeyPackageOptions {
                lifetime: Some(Lifetime::init(1, 2)),
                ..Default::default()
            },
        )?;
        let bytes = generated.bundle.key_package().tls_serialize_detached()?;
        assert!(matches!(
            VerifiedKeyPackageV2::from_bytes(provider.crypto(), &bytes),
            Err(crate::key_package::KeyPackageVerificationError::MlsValidation(_))
        ));
    }
}
