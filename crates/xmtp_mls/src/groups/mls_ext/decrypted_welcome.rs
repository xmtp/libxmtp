use crate::state_tx::state_write;
use openmls::{
    group::{MlsGroupJoinConfig, StagedWelcome, WireFormatPolicy},
    prelude::{
        BasicCredential, KeyPackageBundle, KeyPackageRef, MlsMessageBodyIn, MlsMessageIn, Welcome,
    },
};
use prost::Message;
use tls_codec::{Deserialize, Serialize};
use xmtp_db::TransactionOutcome;
use xmtp_db::XmtpMlsStorageProvider;
use xmtp_db::XmtpOpenMlsProviderRef;

use crate::{groups::GroupError, identity::parse_credential};
use xmtp_configuration::{MAX_PAST_EPOCHS, WELCOME_HPKE_LABEL};
use xmtp_db::{
    NotFound,
    sql_key_store::{KEY_PACKAGE_REFERENCES, KEY_PACKAGE_WRAPPER_PRIVATE_KEY},
};
use xmtp_id::key_package::WrapperAlgorithm;
use xmtp_mls_common::mls_ext::payload_encryption::{unwrap_payload_hpke, unwrap_payload_symmetric};
use xmtp_proto::{
    types::{
        DecryptedWelcomePointer, WelcomeMessage, WelcomeMessageType, WelcomeMessageV1,
        WelcomePointer,
    },
    xmtp::mls::message_contents::WelcomeMetadata,
};

/// A staged decode that must stay inside its current state transaction.
pub(crate) struct DecryptedWelcome {
    /// MLS state to install or discard before the writer is released.
    pub(crate) staged_welcome: StagedWelcome,
    pub(crate) added_by_inbox_id: String,
    pub(crate) added_by_installation_id: Vec<u8>,
    /// Authenticated join metadata. Non-Oneshot joins require an anchor.
    pub(crate) welcome_metadata: Option<WelcomeMetadata>,
}

/// Resolved network input. Every staging attempt reads private keys again.
pub(crate) struct ResolvedWelcome {
    /// Immutable pointer payload. No private keys or staged MLS state are cached.
    pointee: Option<WelcomeMessageV1>,
}

impl DecryptedWelcome {
    /// Decrypt a welcome message using the specified [WrapperAlgorithm].
    ///
    /// This function will find the appropriate private key for the algorithm from the database and use it
    /// to decrypt. It will error if the private key cannot be found or decryption fails
    fn welcome_from_proto_v1(
        provider: &impl XmtpMlsStorageProvider,
        welcome: &WelcomeMessage,
        welcome_v1: &WelcomeMessageV1,
    ) -> Result<(openmls::messages::Welcome, Option<WelcomeMetadata>), GroupError> {
        let WelcomeMessageV1 {
            installation_key: _,
            hpke_public_key,
            wrapper_algorithm,
            data,
            welcome_metadata,
        } = welcome_v1;
        tracing::debug!(welcome_id = %welcome.cursor, "Trying to decrypt welcome");
        let wrapper_ciphersuite = WrapperAlgorithm::try_from(*wrapper_algorithm)?;
        let hash_ref = find_key_package_hash_ref(provider, hpke_public_key)?;
        let private_key = find_private_key(provider, &hash_ref, &wrapper_ciphersuite)?;

        let (welcome_bytes, welcome_metadata_bytes) = unwrap_payload_hpke(
            data,
            welcome_metadata,
            &private_key,
            wrapper_ciphersuite,
            WELCOME_HPKE_LABEL,
        )?;
        let welcome = deserialize_welcome(&welcome_bytes)?;

        let welcome_metadata = Some(welcome_metadata_bytes.as_slice())
            .filter(|bytes| !bytes.is_empty())
            .map(deserialize_welcome_metadata)
            .transpose()?;
        Ok((welcome, welcome_metadata))
    }
    fn welcome_from_decrypted_welcome_pointer(
        decrypted_welcome_pointer: &DecryptedWelcomePointer,
        v1: &WelcomeMessageV1,
    ) -> Result<(openmls::messages::Welcome, Option<WelcomeMetadata>), GroupError> {
        let aead_type = match decrypted_welcome_pointer.aead_type {
            xmtp_proto::xmtp::mls::message_contents::WelcomePointeeEncryptionAeadType::Chacha20Poly1305 => {
                openmls::prelude::AeadType::ChaCha20Poly1305
            }
            xmtp_proto::xmtp::mls::message_contents::WelcomePointeeEncryptionAeadType::Unspecified => {
                return Err(xmtp_proto::ConversionError::InvalidValue {
                    item: "WelcomePointer::V1.aead_type",
                    expected: "ChaCha20Poly1305",
                    got: "Unspecified".into(),
                }
                .into());
            }
        };

        let decrypted_welcome_data = unwrap_payload_symmetric(
            v1.data.as_slice(),
            aead_type,
            &decrypted_welcome_pointer.encryption_key,
            &decrypted_welcome_pointer.data_nonce,
        )?;
        let decrypted_welcome_metadata = unwrap_payload_symmetric(
            v1.welcome_metadata.as_slice(),
            aead_type,
            &decrypted_welcome_pointer.encryption_key,
            &decrypted_welcome_pointer.welcome_metadata_nonce,
        )?;
        let welcome = deserialize_welcome(&decrypted_welcome_data)?;
        let welcome_metadata = Some(decrypted_welcome_metadata.as_slice())
            .filter(|data| !data.is_empty())
            .map(deserialize_welcome_metadata)
            .transpose()?;

        Ok((welcome, welcome_metadata))
    }
    fn stage(
        welcome: Welcome,
        welcome_metadata: Option<WelcomeMetadata>,
        mls_storage: &impl XmtpMlsStorageProvider,
    ) -> Result<Self, GroupError> {
        let join_config = build_group_join_config();

        let provider = XmtpOpenMlsProviderRef::new(mls_storage);
        let builder = StagedWelcome::build_from_welcome(&provider, &join_config, welcome.clone())?;
        let processed_welcome = builder.processed_welcome();

        let psks = processed_welcome.psks();
        if !psks.is_empty() {
            tracing::error!("No PSK support for welcome");
            return Err(GroupError::NoPSKSupport);
        }
        let staged_welcome = builder
            .replace_old_group()
            .skip_lifetime_validation()
            .build()?;

        let added_by_node = staged_welcome.welcome_sender()?;

        let added_by_credential = BasicCredential::try_from(added_by_node.credential().clone())?;
        let added_by_inbox_id = parse_credential(added_by_credential.identity())?;
        let added_by_installation_id = added_by_node.signature_key().as_slice().to_vec();

        Ok(DecryptedWelcome {
            staged_welcome,
            added_by_inbox_id,
            added_by_installation_id,
            welcome_metadata,
        })
    }
}

impl ResolvedWelcome {
    /// Use an inline Welcome without a network lookup.
    pub(crate) fn inline(welcome: &WelcomeMessage) -> Option<Self> {
        matches!(welcome.variant, WelcomeMessageType::V1(_)).then_some(Self { pointee: None })
    }

    /// Resolve pointer data without holding the database writer during network I/O.
    pub(crate) async fn resolve(
        welcome: &WelcomeMessage,
        context: &impl crate::context::XmtpSharedContext,
    ) -> Result<Self, GroupError> {
        let pointer = match &welcome.variant {
            WelcomeMessageType::V1(_) => return Ok(Self { pointee: None }),
            WelcomeMessageType::WelcomePointer(pointer) => {
                state_write(context.mls_storage(), |tx| {
                    let storage = tx.storage();
                    decrypt_welcome_pointer(&storage, pointer).map(TransactionOutcome::Continue)
                })?
                .into_continued()
            }
        };
        let pointee =
            super::super::welcome_pointer::resolve_welcome_pointer(&pointer, context).await?;
        if let Some(pointee) = pointee {
            return Ok(Self {
                pointee: Some(pointee),
            });
        }
        Err(GroupError::WelcomeDataNotFound(hex::encode(
            pointer.destination.as_slice(),
        )))
    }

    /// Decode against private keys read under the current write transaction.
    pub(crate) fn stage(
        &self,
        welcome: &WelcomeMessage,
        storage: &impl XmtpMlsStorageProvider,
    ) -> Result<DecryptedWelcome, GroupError> {
        let (welcome, metadata) = match &welcome.variant {
            WelcomeMessageType::V1(v1) => {
                DecryptedWelcome::welcome_from_proto_v1(storage, welcome, v1)?
            }
            WelcomeMessageType::WelcomePointer(pointer) => {
                let pointer = decrypt_welcome_pointer(storage, pointer)?;
                DecryptedWelcome::welcome_from_decrypted_welcome_pointer(
                    &pointer,
                    self.pointee
                        .as_ref()
                        .ok_or(GroupError::UninitializedResult)?,
                )?
            }
        };
        DecryptedWelcome::stage(welcome, metadata, storage)
    }
}

pub(super) fn find_key_package_hash_ref(
    provider: &impl XmtpMlsStorageProvider,
    hpke_public_key: &[u8],
) -> Result<KeyPackageRef, GroupError> {
    let serialized_hpke_public_key = hpke_public_key.tls_serialize_detached()?;

    Ok(provider
        .read(KEY_PACKAGE_REFERENCES, &serialized_hpke_public_key)?
        .ok_or(NotFound::KeyPackageReference(serialized_hpke_public_key))?)
}

/// For Curve25519 keys, we can just get the private key from the key package bundle
/// For Post Quantum keys, we use look up the KEY_PACKAGE_WRAPPER_PRIVATE_KEY which is keyed
/// by the hash reference of the key package.
pub(super) fn find_private_key(
    provider: &impl XmtpMlsStorageProvider,
    hash_ref: &KeyPackageRef,
    wrapper_ciphersuite: &WrapperAlgorithm,
) -> Result<Vec<u8>, GroupError> {
    match wrapper_ciphersuite {
        WrapperAlgorithm::Curve25519 => {
            let key_package: Option<KeyPackageBundle> = provider.key_package(hash_ref)?;
            Ok(key_package
                .map(|kp| kp.init_private_key().to_vec())
                .ok_or_else(|| NotFound::KeyPackage(hash_ref.as_slice().to_vec()))?)
        }
        WrapperAlgorithm::XWingMLKEM768Draft6 => {
            let serialized_hash_ref = bincode::serialize(hash_ref)
                .map_err(|_| GroupError::NotFound(NotFound::PostQuantumPrivateKey))?;
            let private_key =
                provider.read(KEY_PACKAGE_WRAPPER_PRIVATE_KEY, &serialized_hash_ref)?;

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

fn deserialize_welcome(welcome_bytes: &Vec<u8>) -> Result<Welcome, GroupError> {
    if welcome_bytes
        .get(..2)
        .is_some_and(|version| version != [0, 1])
    {
        return Err(openmls::prelude::WelcomeError::UnsupportedMlsVersion.into());
    }
    let welcome = MlsMessageIn::tls_deserialize(&mut welcome_bytes.as_slice())?;
    match welcome.extract() {
        MlsMessageBodyIn::Welcome(welcome) => Ok(welcome),
        _ => Err(openmls::prelude::WelcomeError::NotAWelcomeMessage.into()),
    }
}

fn deserialize_welcome_metadata(metadata_bytes: &[u8]) -> Result<WelcomeMetadata, GroupError> {
    let metadata =
        WelcomeMetadata::decode(metadata_bytes).map_err(|_| GroupError::InvalidWelcomeMetadata)?;
    Ok(metadata)
}

pub(crate) fn decrypt_welcome_pointer(
    provider: &impl XmtpMlsStorageProvider,
    welcome_pointer: &WelcomePointer,
) -> Result<DecryptedWelcomePointer, GroupError> {
    tracing::debug!("Trying to decrypt welcome pointer");
    let hash_ref = find_key_package_hash_ref(provider, &welcome_pointer.hpke_public_key)?;
    let wrapper_algorithm = WrapperAlgorithm::try_from(welcome_pointer.wrapper_algorithm)?;
    let private_key = find_private_key(provider, &hash_ref, &wrapper_algorithm)?;

    let welcome_bytes = unwrap_payload_hpke(
        &welcome_pointer.welcome_pointer,
        &[],
        &private_key,
        wrapper_algorithm,
        WELCOME_HPKE_LABEL,
    )?;

    Ok(DecryptedWelcomePointer::decode(welcome_bytes.0.as_slice())?)
}
