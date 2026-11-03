//! Building and storing an installation's key packages.

use derive_builder::Builder;
use openmls::{key_packages::KeyPackage, prelude::Credential as OpenMlsCredential};
#[cfg(any(test, feature = "test-hooks"))]
use std::sync::atomic::Ordering;
use xmtp_cryptography::XmtpInstallationCredential;
use xmtp_db::MlsProviderExt;
use xmtp_mls_core::identity::{IdentityError, store_key_package_references};

/// A new key package and, if requested, its post-quantum public key.
pub struct NewKeyPackageResult {
    /// The key package.
    pub key_package: KeyPackage,
    /// The post-quantum public key, if one was generated.
    pub pq_pub_key: Option<Vec<u8>>,
}

/// Whether key packages built in its scope advertise welcome pointees, with a
/// count of the packages that read it so a test can tell that it ran.
#[cfg(any(test, feature = "test-hooks"))]
#[doc(hidden)]
#[derive(Clone, Debug)]
pub struct WelcomePointersHook {
    enabled: bool,
    reads: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

#[cfg(any(test, feature = "test-hooks"))]
impl WelcomePointersHook {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            reads: Default::default(),
        }
    }

    /// Record a package build that reads this hook, and return its setting.
    pub fn read(&self) -> bool {
        self.reads.fetch_add(1, Ordering::SeqCst);
        self.enabled
    }

    /// How many package builds read this hook.
    pub fn reads(&self) -> usize {
        self.reads.load(Ordering::SeqCst)
    }
}

#[cfg(any(test, feature = "test-hooks"))]
tokio::task_local! {
    pub static ENABLE_WELCOME_POINTERS: WelcomePointersHook;
}

#[derive(Builder, Debug)]
#[builder(build_fn(error = "IdentityError", name = "inner_build", private))]
pub struct XmtpKeyPackage {
    #[builder(setter(into))]
    inbox_id: String,
    #[builder(setter(into))]
    credential: OpenMlsCredential,
    #[builder(setter(into))]
    installation_keys: XmtpInstallationCredential,
}

impl XmtpKeyPackage {
    /// Starts building a key package.
    pub fn builder() -> XmtpKeyPackageBuilder {
        XmtpKeyPackageBuilder::default()
    }
}

impl XmtpKeyPackageBuilder {
    /// Builds the key package and stores its private keys through `provider`.
    pub fn build(
        &mut self,
        provider: &impl MlsProviderExt,
        include_post_quantum: bool,
    ) -> Result<NewKeyPackageResult, IdentityError> {
        let this = self.inner_build()?;
        #[allow(unused_mut)]
        let mut options = xmtp_id::key_package::KeyPackageOptions {
            include_post_quantum,
            ..Default::default()
        };
        #[cfg(any(test, feature = "test-hooks"))]
        {
            options.welcome_pointers = ENABLE_WELCOME_POINTERS
                .try_with(WelcomePointersHook::read)
                .unwrap_or(true);
            options.lifetime =
                Some(crate::identity_lifecycle::test_mocks::maybe_mock_package_lifetime());
        }
        let generated = xmtp_id::key_package::build_key_package(
            &this.inbox_id,
            this.credential,
            &this.installation_keys,
            provider,
            options,
        )
        .map_err(|error| match error {
            xmtp_id::key_package::KeyPackageConstructionError::Generation(e) => {
                IdentityError::KeyPackageGenerationError(e)
            }
            xmtp_id::key_package::KeyPackageConstructionError::InvalidExtension(e) => {
                IdentityError::InvalidExtension(e)
            }
            xmtp_id::key_package::KeyPackageConstructionError::Encode(e) => {
                IdentityError::CredentialSerialization(e)
            }
            xmtp_id::key_package::KeyPackageConstructionError::PostQuantum(e) => {
                IdentityError::GeneratePostQuantumKey(e)
            }
        })?;
        store_key_package_references(
            provider,
            generated.bundle.key_package(),
            &generated.post_quantum_keypair,
        )?;
        Ok(NewKeyPackageResult {
            key_package: generated.bundle.key_package().clone(),
            pq_pub_key: generated.post_quantum_keypair.map(|kp| kp.public),
        })
    }
}
