use openmls::{
    group::{MlsGroupJoinConfig, ProcessedWelcome, StagedWelcome, WireFormatPolicy},
    prelude::{
        BasicCredential, KeyPackageBundle, KeyPackageRef, MlsMessageBodyIn, MlsMessageIn, Welcome,
    },
};
use openmls_traits::storage::StorageProvider;
use tls_codec::{Deserialize, Serialize};
use xmtp_db::MlsProviderExt;
use xmtp_db::XmtpMlsStorageProvider;

use crate::{
    client::ClientError,
    groups::{GroupError, mls_ext::unwrap_welcome},
    identity::parse_credential,
};
use xmtp_configuration::MAX_PAST_EPOCHS;
use xmtp_db::{
    NotFound,
    sql_key_store::{KEY_PACKAGE_REFERENCES, KEY_PACKAGE_WRAPPER_PRIVATE_KEY},
};

use super::WrapperAlgorithm;

pub(crate) struct DecryptedWelcome {
    pub(crate) staged_welcome: StagedWelcome,
    pub(crate) added_by_inbox_id: String,
    pub(crate) added_by_installation_id: Vec<u8>,
}

impl DecryptedWelcome {
    /// Decrypt a welcome message using the specified [WrapperAlgorithm].
    ///
    /// This function will find the appropriate private key for the algorithm from the database and use it
    /// to decrypt. It will error if the private key cannot be found or decryption fails
    pub(crate) fn from_encrypted_bytes<P: MlsProviderExt>(
        provider: &P,
        hpke_public_key: &[u8],
        encrypted_welcome_bytes: &[u8],
        wrapper_ciphersuite: WrapperAlgorithm,
    ) -> Result<DecryptedWelcome, GroupError> {
        tracing::info!("Trying to decrypt welcome");
        let hash_ref = find_key_package_hash_ref(provider, hpke_public_key)?;
        let private_key = find_private_key(provider, &hash_ref, &wrapper_ciphersuite)?;
        let welcome_bytes =
            unwrap_welcome(encrypted_welcome_bytes, &private_key, wrapper_ciphersuite)?;

        let welcome = deserialize_welcome(&welcome_bytes)?;

        let join_config = build_group_join_config();

        let processed_welcome =
            ProcessedWelcome::new_from_welcome(provider, &join_config, welcome.clone())?;

        let psks = processed_welcome.psks();
        if !psks.is_empty() {
            tracing::error!("No PSK support for welcome");
            return Err(GroupError::NoPSKSupport);
        }
        let staged_welcome = processed_welcome.into_staged_welcome(provider, None)?;

        let added_by_node = staged_welcome.welcome_sender()?;

        let added_by_credential = BasicCredential::try_from(added_by_node.credential().clone())?;
        let added_by_inbox_id = parse_credential(added_by_credential.identity())?;
        let added_by_installation_id = added_by_node.signature_key().as_slice().to_vec();

        Ok(DecryptedWelcome {
            staged_welcome,
            added_by_inbox_id,
            added_by_installation_id,
        })
    }
}

pub(super) fn find_key_package_hash_ref(
    provider: &impl MlsProviderExt,
    hpke_public_key: &[u8],
) -> Result<KeyPackageRef, GroupError> {
    let serialized_hpke_public_key = hpke_public_key.tls_serialize_detached()?;

    Ok(provider
        .key_store()
        .read(KEY_PACKAGE_REFERENCES, &serialized_hpke_public_key)?
        .ok_or(NotFound::KeyPackageReference(serialized_hpke_public_key))?)
}

/// For Curve25519 keys, we can just get the private key from the key package bundle
/// For Post Quantum keys, we use look up the KEY_PACKAGE_WRAPPER_PRIVATE_KEY which is keyed
/// by the hash reference of the key package.
pub(super) fn find_private_key(
    provider: &impl MlsProviderExt,
    hash_ref: &KeyPackageRef,
    wrapper_ciphersuite: &WrapperAlgorithm,
) -> Result<Vec<u8>, GroupError> {
    match wrapper_ciphersuite {
        WrapperAlgorithm::Curve25519 => {
            let key_package: Option<KeyPackageBundle> =
                provider.key_store().key_package(hash_ref)?;
            Ok(key_package
                .map(|kp| kp.init_private_key().to_vec())
                .ok_or_else(|| NotFound::KeyPackage(hash_ref.as_slice().to_vec()))?)
        }
        WrapperAlgorithm::XWingMLKEM768Draft6 => {
            let serialized_hash_ref = bincode::serialize(hash_ref)
                .map_err(|_| GroupError::NotFound(NotFound::PostQuantumPrivateKey))?;
            let private_key = provider
                .key_store()
                .read(KEY_PACKAGE_WRAPPER_PRIVATE_KEY, &serialized_hash_ref)?;

            Ok(private_key.ok_or(NotFound::PostQuantumPrivateKey)?)
        }
    }
}

pub(crate) fn build_group_join_config() -> MlsGroupJoinConfig {
    MlsGroupJoinConfig::builder()
        .wire_format_policy(WireFormatPolicy::default())
        .max_past_epochs(MAX_PAST_EPOCHS)
        .use_ratchet_tree_extension(true)
        .build()
}

fn deserialize_welcome(welcome_bytes: &Vec<u8>) -> Result<Welcome, ClientError> {
    let welcome = MlsMessageIn::tls_deserialize(&mut welcome_bytes.as_slice())?;
    match welcome.extract() {
        MlsMessageBodyIn::Welcome(welcome) => Ok(welcome),
        _ => Err(ClientError::Generic(
            "unexpected message type in welcome".to_string(),
        )),
    }
}
