//! Post-commit actions: the Welcomes a commit sends and the installations they reach.

use super::*;

#[derive(Debug, Clone)]
pub enum PostCommitAction {
    SendWelcomes(SendWelcomesAction),
}

#[derive(Debug)]
pub struct MembershipDiffWithKeyPackages {
    pub new_installations: Vec<Installation>,
    pub new_key_packages: Vec<KeyPackage>,
    pub removed_installations: HashSet<Vec<u8>>,
    pub failed_installations: Vec<Vec<u8>>,
}

impl MembershipDiffWithKeyPackages {
    pub fn new(
        new_installations: Vec<Installation>,
        new_key_packages: Vec<KeyPackage>,
        removed_installations: HashSet<Vec<u8>>,
        failed_installations: Vec<Vec<u8>>,
    ) -> MembershipDiffWithKeyPackages {
        MembershipDiffWithKeyPackages {
            new_installations,
            new_key_packages,
            removed_installations,
            failed_installations,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Installation {
    pub(crate) installation_key: Vec<u8>,
    pub(crate) hpke_public_key: Vec<u8>,
    pub(crate) welcome_wrapper_algorithm: WrapperAlgorithm,
    pub(crate) welcome_pointee_encryption_aead_types: WelcomePointersExtension,
}

impl Installation {
    /// The addressing fields verification already decoded and checked, so a
    /// sender can always wrap a Welcome for the result.
    pub fn from_verified_key_package(key_package: &VerifiedKeyPackageV2) -> Self {
        Self {
            installation_key: key_package.installation_id(),
            hpke_public_key: key_package.wrapper.pub_key_bytes.clone(),
            welcome_wrapper_algorithm: key_package.wrapper.algorithm,
            welcome_pointee_encryption_aead_types: key_package.pointee_aead.clone(),
        }
    }

    /// The installation can open a welcome pointer: it advertises a
    /// ChaCha20-Poly1305 pointee, and its wrapper is one the pointer wire format
    /// can name. Pointers are always wrapped post-quantum.
    // implements: JOIN-091
    pub(crate) fn pointer_capable(&self) -> bool {
        self.welcome_pointee_encryption_aead_types.compatible()
            && WelcomePointerWrapperAlgorithm::try_from(self.welcome_wrapper_algorithm).is_ok()
    }
}

impl From<Installation> for InstallationProto {
    fn from(installation: Installation) -> Self {
        Self {
            installation_key: installation.installation_key,
            hpke_public_key: installation.hpke_public_key,
            welcome_wrapper_algorithm: installation.welcome_wrapper_algorithm.into(),
            welcome_pointee_encryption_aead_types: Some(
                installation.welcome_pointee_encryption_aead_types.into(),
            ),
        }
    }
}

impl TryFrom<InstallationProto> for Installation {
    type Error = ConversionError;
    fn try_from(installation: InstallationProto) -> Result<Self, Self::Error> {
        Ok(Self {
            installation_key: installation.installation_key,
            hpke_public_key: installation.hpke_public_key,
            welcome_wrapper_algorithm: installation.welcome_wrapper_algorithm.try_into()?,
            welcome_pointee_encryption_aead_types: installation
                .welcome_pointee_encryption_aead_types
                .map(Into::into)
                .unwrap_or_else(WelcomePointersExtension::empty),
        })
    }
}

#[derive(Debug, Clone)]
pub struct SendWelcomesAction {
    pub installations: Vec<Installation>,
    pub welcome_message: Vec<u8>,
}

impl SendWelcomesAction {
    pub fn new(installations: Vec<Installation>, welcome_message: Vec<u8>) -> Self {
        Self {
            installations,
            welcome_message,
        }
    }

    pub(crate) fn to_bytes(&self) -> Vec<u8> {
        PostCommitActionProto {
            kind: Some(PostCommitActionKind::SendWelcomes(SendWelcomesProto {
                installations: self
                    .installations
                    .clone()
                    .into_iter()
                    .map(|i| i.into())
                    .collect(),
                welcome_message: self.welcome_message.clone(),
            })),
        }
        .encode_to_vec()
    }
}

impl PostCommitAction {
    pub(crate) fn to_bytes(&self) -> Vec<u8> {
        match self {
            PostCommitAction::SendWelcomes(action) => action.to_bytes(),
        }
    }

    pub(crate) fn from_bytes(data: &[u8]) -> Result<Self, IntentError> {
        let decoded = PostCommitActionProto::decode(data)?
            .kind
            .ok_or(IntentError::MissingPostCommit)?;
        match decoded {
            PostCommitActionKind::SendWelcomes(proto) => {
                Ok(Self::SendWelcomes(SendWelcomesAction::new(
                    proto
                        .installations
                        .into_iter()
                        .map(|i| i.try_into())
                        .collect::<Result<_, _>>()?,
                    proto.welcome_message,
                )))
            }
        }
    }

    pub(crate) fn from_welcome(
        welcome: MlsMessageOut,
        installations: Vec<Installation>,
    ) -> Result<Self, IntentError> {
        let welcome_bytes = welcome.tls_serialize_detached()?;

        Ok(Self::SendWelcomes(SendWelcomesAction::new(
            installations,
            welcome_bytes,
        )))
    }
}

impl TryFrom<Vec<u8>> for PostCommitAction {
    type Error = IntentError;

    fn try_from(data: Vec<u8>) -> Result<Self, Self::Error> {
        PostCommitAction::from_bytes(data.as_slice())
    }
}
