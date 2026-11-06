use super::{WelcomePointersExtension, WrapperAlgorithm, WrapperEncryptionExtension};
use openmls::{
    credentials::{Credential, CredentialWithKey},
    extensions::{ApplicationIdExtension, Extension, ExtensionType, Extensions},
    key_packages::{KeyPackage, KeyPackageBundle, Lifetime},
    messages::proposals::ProposalType,
    prelude::{Capabilities, HpkeKeyPair, LeafNode},
};
use openmls_traits::OpenMlsProvider;
use xmtp_configuration::{
    WELCOME_POINTEE_ENCRYPTION_AEAD_TYPES_EXTENSION_ID, WELCOME_WRAPPER_ENCRYPTION_EXTENSION_ID,
};
use xmtp_cryptography::configuration::CIPHERSUITE;
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
    pub lifetime: Option<Lifetime>,
}
impl Default for KeyPackageOptions {
    fn default() -> Self {
        Self {
            include_post_quantum: false,
            welcome_pointers: true,
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
// implements: JOIN-072, JOIN-003
pub fn build_key_package(
    inbox_id: &str,
    credential: Credential,
    installation_keys: &XmtpInstallationCredential,
    provider: &impl OpenMlsProvider,
    options: KeyPackageOptions,
) -> Result<GeneratedKeyPackage, KeyPackageConstructionError> {
    let mut extensions = vec![];
    if options.welcome_pointers {
        extensions.push(WelcomePointersExtension::available_types().try_into()?);
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

    // No `LastResort`: no key package carries that extension, and groups do
    // not require it.
    let capability_extensions = vec![
        ExtensionType::ApplicationId,
        // Clients can join groups that use AppDataUpdate. The last-resort
        // mark is in the key package's AppDataDictionary extension too.
        ExtensionType::AppDataDictionary,
        ExtensionType::Unknown(WELCOME_WRAPPER_ENCRYPTION_EXTENSION_ID),
        ExtensionType::Unknown(WELCOME_POINTEE_ENCRYPTION_AEAD_TYPES_EXTENSION_ID),
    ];
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

    // Every key package is last resort: the installation keeps it for every
    // Welcome addressed to it. `mark_as_last_resort` adds the
    // `last_resort_key_package` component (0x0004) with empty data to the key
    // package's AppDataDictionary, as upstream OpenMLS does, and no legacy
    // `last_resort` extension. Its capabilities must list AppDataDictionary.
    let kp_builder = KeyPackage::builder()
        .leaf_node_capabilities(capabilities)
        .leaf_node_extensions(leaf_node_extensions)
        .key_package_extensions(key_package_extensions)
        .mark_as_last_resort();

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
    use openmls::extensions::LastResortExtension;
    use openmls::prelude::tls_codec::Serialize;
    use openmls_rust_crypto::OpenMlsRustCrypto;

    /// The extension type the xmtp OpenMLS fork names `ImmutableMetadata`.
    /// Upstream OpenMLS reports it as `Unknown(0xf000)`.
    const FORK_IMMUTABLE_METADATA_EXTENSION_ID: u16 = 0xf000;

    #[xmtp_common::test(unwrap_try = true)]
    // verifies: JOIN-004
    fn generated_package_preserves_options_and_verifies() {
        for include_post_quantum in [false, true] {
            for welcome_pointers in [false, true] {
                let provider = OpenMlsRustCrypto::default();
                let key = XmtpInstallationCredential::new();
                let generated = build_key_package(
                    "inbox",
                    create_credential("inbox"),
                    &key,
                    &provider,
                    KeyPackageOptions {
                        include_post_quantum,
                        welcome_pointers,
                        lifetime: Some(Lifetime::new(3600)),
                    },
                )?;
                let bytes = generated.bundle.key_package().tls_serialize_detached()?;
                let verified = VerifiedKeyPackageV2::from_bytes(provider.crypto(), &bytes)?;
                assert_eq!(verified.credential.inbox_id, "inbox");
                assert_eq!(verified.installation_public_key, key.public_slice());
                let leaf = verified.inner.leaf_node();
                assert!(
                    leaf.capabilities()
                        .extensions()
                        .contains(&ExtensionType::AppDataDictionary)
                );
                for legacy_extension in [
                    xmtp_configuration::GROUP_PERMISSIONS_EXTENSION_ID,
                    xmtp_configuration::MUTABLE_METADATA_EXTENSION_ID,
                    xmtp_configuration::GROUP_MEMBERSHIP_EXTENSION_ID,
                ] {
                    assert!(
                        !leaf
                            .capabilities()
                            .extensions()
                            .contains(&ExtensionType::Unknown(legacy_extension)),
                        "key package advertises removed legacy extension {legacy_extension:?}",
                    );
                }
                assert!(
                    !leaf
                        .capabilities()
                        .extensions()
                        .iter()
                        .any(|ext| u16::from(*ext) == FORK_IMMUTABLE_METADATA_EXTENSION_ID),
                    "key package advertises the fork's ImmutableMetadata extension",
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
                assert_eq!(pointer.is_some(), welcome_pointers);
                let wrapper = verified.wrapper_encryption()?;
                assert_eq!(wrapper.is_some(), include_post_quantum);
                if let Some(pair) = generated.post_quantum_keypair {
                    assert_eq!(wrapper.unwrap().pub_key_bytes, pair.public);
                    assert!(!pair.private.is_empty());
                }
            }
        }
    }

    /// The draft-10 `last_resort_key_package` application component.
    /// libxmtp and upstream OpenMLS (#2133) mark a last-resort key package
    /// with it, with empty data, and reject it with any other data.
    const LAST_RESORT_COMPONENT: u16 = 0x0004;

    /// A generated key package is last resort through the component alone:
    /// its AppDataDictionary holds 0x0004 with empty data, it has no legacy
    /// `last_resort` extension, and its capabilities do not list one.
    #[xmtp_common::test(unwrap_try = true)]
    // verifies: JOIN-003
    fn generated_package_is_marked_last_resort_by_the_component() {
        for welcome_pointers in [false, true] {
            let provider = OpenMlsRustCrypto::default();
            let key = XmtpInstallationCredential::new();
            let generated = build_key_package(
                "inbox",
                create_credential("inbox"),
                &key,
                &provider,
                KeyPackageOptions {
                    welcome_pointers,
                    ..Default::default()
                },
            )?;
            let bytes = generated.bundle.key_package().tls_serialize_detached()?;
            let package = VerifiedKeyPackageV2::from_bytes(provider.crypto(), &bytes)?.inner;
            let extensions = package.extensions();
            assert_eq!(
                extensions
                    .app_data_dictionary()
                    .and_then(|extension| extension.dictionary().get(&LAST_RESORT_COMPONENT)),
                Some(&[][..])
            );
            assert!(
                !extensions
                    .iter()
                    .any(|extension| extension.extension_type() == ExtensionType::LastResort),
                "the key package carries the legacy last_resort extension"
            );
            assert!(
                !package
                    .leaf_node()
                    .capabilities()
                    .extensions()
                    .contains(&ExtensionType::LastResort)
            );
            assert!(package.last_resort());
        }
    }

    /// An application component that is not the last-resort mark.
    const OTHER_COMPONENT: u16 = 0x8001;

    /// A key package whose `app_data_dictionary` holds the last-resort
    /// component with `data`. Its capabilities advertise AppDataDictionary, as
    /// upstream's builder sets them for that component. `legacy` also adds
    /// the `last_resort` extension.
    fn key_package_with_last_resort_component(
        data: Vec<u8>,
        legacy: bool,
    ) -> (OpenMlsRustCrypto, Vec<u8>) {
        key_package_with_components(&[(LAST_RESORT_COMPONENT, data)], legacy)
    }

    /// A key package whose `app_data_dictionary` holds `components`, as
    /// [`key_package_with_last_resort_component`] builds it.
    fn key_package_with_components(
        components: &[(u16, Vec<u8>)],
        legacy: bool,
    ) -> (OpenMlsRustCrypto, Vec<u8>) {
        use openmls::extensions::{AppDataDictionary, AppDataDictionaryExtension};
        let mut dictionary = AppDataDictionary::new();
        for (component, data) in components {
            let _ = dictionary.insert(*component, data.clone());
        }
        let mut extensions = vec![Extension::AppDataDictionary(
            AppDataDictionaryExtension::new(dictionary),
        )];
        if legacy {
            extensions.push(Extension::LastResort(LastResortExtension::default()));
        }
        key_package_with_extensions(extensions)
    }

    /// A key package with `extensions`, signed by a fresh installation key.
    /// Its capabilities advertise every extension type these tests use.
    fn key_package_with_extensions(extensions: Vec<Extension>) -> (OpenMlsRustCrypto, Vec<u8>) {
        let provider = OpenMlsRustCrypto::default();
        let key = XmtpInstallationCredential::new();
        let capabilities = Capabilities::new(
            None,
            Some(&[CIPHERSUITE]),
            Some(&[
                ExtensionType::LastResort,
                ExtensionType::ApplicationId,
                ExtensionType::AppDataDictionary,
                ExtensionType::Unknown(WELCOME_WRAPPER_ENCRYPTION_EXTENSION_ID),
                ExtensionType::Unknown(WELCOME_POINTEE_ENCRYPTION_AEAD_TYPES_EXTENSION_ID),
            ]),
            None,
            None,
        );
        let bundle = KeyPackage::builder()
            .leaf_node_capabilities(capabilities)
            .leaf_node_extensions(
                Extensions::single(Extension::ApplicationId(ApplicationIdExtension::new(
                    b"inbox",
                )))
                .unwrap(),
            )
            .key_package_extensions(Extensions::from_vec(extensions).unwrap())
            .build(
                CIPHERSUITE,
                &provider,
                &key,
                CredentialWithKey {
                    credential: create_credential("inbox"),
                    signature_key: key.public_slice().into(),
                },
            )
            .unwrap();
        let bytes = bundle.key_package().tls_serialize_detached().unwrap();
        (provider, bytes)
    }

    /// Upstream OpenMLS rejects a key package whose last-resort component has
    /// data, so rc4 must reject it too. Otherwise an rc4 member could Add it
    /// and future members would reject that commit: the group would fork.
    #[xmtp_common::test(unwrap_try = true)]
    // verifies: JOIN-082
    fn last_resort_component_with_data_fails_verification() {
        for legacy in [false, true] {
            let (provider, bytes) = key_package_with_last_resort_component(vec![1], legacy);
            assert!(
                matches!(
                    VerifiedKeyPackageV2::from_bytes(provider.crypto(), &bytes),
                    Err(
                        crate::key_package::KeyPackageVerificationError::MlsValidation(
                            openmls::prelude::KeyPackageVerifyError::MalformedLastResortComponent
                        )
                    )
                ),
                "a last-resort component with data verified (legacy extension: {legacy})"
            );
        }
    }

    /// The component with empty data, libxmtp's own mark and upstream
    /// OpenMLS's, verifies, with or without the legacy extension next to it.
    #[xmtp_common::test(unwrap_try = true)]
    // verifies: JOIN-082
    fn upstream_last_resort_component_verifies() {
        for legacy in [false, true] {
            let (provider, bytes) = key_package_with_last_resort_component(vec![], legacy);
            let verified = VerifiedKeyPackageV2::from_bytes(provider.crypto(), &bytes)?;
            assert_eq!(verified.credential.inbox_id, "inbox");
        }
    }

    /// The check is on the last-resort mark only: data in another component
    /// is accepted, as upstream accepts it, next to an empty mark and the
    /// legacy extension. A stricter check would reject commits that future
    /// members accept, and the group would fork the other way.
    #[xmtp_common::test(unwrap_try = true)]
    // verifies: JOIN-082
    fn other_components_with_data_verify() {
        let (provider, bytes) = key_package_with_components(
            &[
                (OTHER_COMPONENT, vec![1, 2, 3]),
                (LAST_RESORT_COMPONENT, vec![]),
            ],
            true,
        );
        let verified = VerifiedKeyPackageV2::from_bytes(provider.crypto(), &bytes)?;
        assert_eq!(verified.credential.inbox_id, "inbox");
    }

    #[xmtp_common::test(unwrap_try = true)]
    // verifies: JOIN-008
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
            Err(
                crate::key_package::KeyPackageVerificationError::MlsValidation(
                    openmls::prelude::KeyPackageVerifyError::LifetimeError(_)
                )
            )
        ));
    }

    fn wrapper_extension(algorithm: i32, pub_key: Vec<u8>) -> Extension {
        use prost::Message;
        let wrapper = xmtp_proto::xmtp::mls::message_contents::WelcomeWrapperEncryption {
            pub_key,
            algorithm,
        };
        Extension::Unknown(
            WELCOME_WRAPPER_ENCRYPTION_EXTENSION_ID,
            openmls::prelude::UnknownExtension(wrapper.encode_to_vec()),
        )
    }

    fn pointee_aead_extension(supported_aead_types: Vec<i32>) -> Extension {
        use prost::Message;
        let pointee =
            xmtp_proto::xmtp::mls::message_contents::WelcomePointeeEncryptionAeadTypesExtension {
                supported_aead_types,
            };
        Extension::Unknown(
            WELCOME_POINTEE_ENCRYPTION_AEAD_TYPES_EXTENSION_ID,
            openmls::prelude::UnknownExtension(pointee.encode_to_vec()),
        )
    }

    /// A package no sender can wrap a Welcome for fails verification, so the
    /// backend refuses it and a sender records its installation as failed:
    /// an extension that does not decode, a wrapper algorithm a key package
    /// cannot advertise, a key of the wrong length, or a key that no HPKE
    /// sender setup accepts.
    #[xmtp_common::test(unwrap_try = true)]
    // verifies: JOIN-090
    fn unusable_addressing_extensions_fail_verification() {
        use crate::key_package::KeyPackageVerificationError;
        use xmtp_proto::ConversionError;
        let reason = |error: &ConversionError| match error {
            ConversionError::Decode(_) => "undecodable",
            ConversionError::InvalidValue { .. } => "algorithm",
            ConversionError::InvalidLength { .. } => "length",
            ConversionError::InvalidPublicKey { .. } => "setup",
            _ => "other",
        };
        let curve25519: i32 = WrapperAlgorithm::Curve25519.into();
        let xwing: i32 = WrapperAlgorithm::XWingMLKEM768Draft6.into();
        let undecodable =
            |id| Extension::Unknown(id, openmls::prelude::UnknownExtension(vec![0xff]));
        for (extension, expected) in [
            (
                undecodable(WELCOME_POINTEE_ENCRYPTION_AEAD_TYPES_EXTENSION_ID),
                "undecodable",
            ),
            (
                undecodable(WELCOME_WRAPPER_ENCRYPTION_EXTENSION_ID),
                "undecodable",
            ),
            (wrapper_extension(0, vec![9; 32]), "algorithm"),
            (wrapper_extension(99, vec![9; 32]), "algorithm"),
            (wrapper_extension(curve25519, vec![9; 31]), "length"),
            (wrapper_extension(curve25519, vec![9; 33]), "length"),
            (wrapper_extension(xwing, vec![9; 1215]), "length"),
            (wrapper_extension(xwing, vec![9; 1217]), "length"),
            // A low-order point, and a key outside the ML-KEM modulus.
            (wrapper_extension(curve25519, vec![0; 32]), "setup"),
            (wrapper_extension(xwing, vec![0xff; 1216]), "setup"),
        ] {
            let case = format!("{extension:?}");
            let (provider, bytes) = key_package_with_extensions(vec![extension]);
            match VerifiedKeyPackageV2::from_bytes(provider.crypto(), &bytes) {
                Err(KeyPackageVerificationError::ConversionError(error)) => {
                    assert_eq!(reason(&error), expected, "{case}: {error:?}")
                }
                other => panic!("{case}: expected a conversion error, got {other:?}"),
            }
        }
    }

    /// A pointee AEAD value that is unspecified or that this client does not
    /// define counts as absent, and the package still verifies with the
    /// values it does define. Rejecting it would make an installation that
    /// advertises a later value unaddable by this client.
    #[xmtp_common::test(unwrap_try = true)]
    // verifies: JOIN-090
    fn undefined_pointee_aead_values_are_ignored() {
        use openmls::prelude::AeadType;
        let chacha20_poly1305 = 3;
        for (advertised, expected) in [
            (vec![i32::MAX], vec![]),
            (
                vec![0, i32::MAX, chacha20_poly1305],
                vec![AeadType::ChaCha20Poly1305],
            ),
        ] {
            let (provider, bytes) =
                key_package_with_extensions(vec![pointee_aead_extension(advertised.clone())]);
            let verified = VerifiedKeyPackageV2::from_bytes(provider.crypto(), &bytes)?;
            assert_eq!(
                verified.pointee_aead.supported_aead_types, expected,
                "{advertised:?}"
            );
        }
    }
}
