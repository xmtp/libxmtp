use openmls::{
    group::{MlsGroupJoinConfig, StagedWelcome, WireFormatPolicy},
    prelude::{
        BasicCredential, KeyPackageBundle, KeyPackageRef, MlsMessageBodyIn, MlsMessageIn, Welcome,
    },
};
use prost::Message;
use tls_codec::{Deserialize, Serialize};
use xmtp_db::XmtpMlsStorageProvider;
use xmtp_db::XmtpOpenMlsProviderRef;

use super::WrapperAlgorithm;
use crate::{
    client::ClientError,
    groups::{
        GroupError,
        mls_ext::{unwrap_welcome, unwrap_welcome_symmetric},
        welcome_sync::ResumableWelcomeMessage,
    },
    identity::parse_credential,
};
use xmtp_configuration::MAX_PAST_EPOCHS;
use xmtp_db::{
    NotFound,
    sql_key_store::{KEY_PACKAGE_REFERENCES, KEY_PACKAGE_WRAPPER_PRIVATE_KEY},
};
use xmtp_proto::mls_v1::WelcomeMetadata;

pub(crate) struct DecryptedWelcome {
    pub(crate) staged_welcome: StagedWelcome,
    pub(crate) added_by_inbox_id: String,
    pub(crate) added_by_installation_id: Vec<u8>,
    pub(crate) welcome_metadata: Option<WelcomeMetadata>,
}

impl DecryptedWelcome {
    /// Decrypt a welcome message using the specified [WrapperAlgorithm].
    ///
    /// This function will find the appropriate private key for the algorithm from the database and use it
    /// to decrypt. It will error if the private key cannot be found or decryption fails
    fn welcome_from_proto_v1(
        provider: &impl XmtpMlsStorageProvider,
        proto: &xmtp_proto::mls_v1::welcome_message::V1,
    ) -> Result<(openmls::messages::Welcome, Option<WelcomeMetadata>), GroupError> {
        let xmtp_proto::mls_v1::welcome_message::V1 {
            id,
            data,
            hpke_public_key,
            wrapper_algorithm,
            welcome_metadata,
            ..
        } = proto;
        tracing::info!(id, "Trying to decrypt welcome");
        let wrapper_ciphersuite = WrapperAlgorithm::try_from(*wrapper_algorithm)?;
        let hash_ref = find_key_package_hash_ref(provider, hpke_public_key)?;
        let private_key = find_private_key(provider, &hash_ref, &wrapper_ciphersuite)?;

        let (welcome_bytes, welcome_metadata_bytes) =
            unwrap_welcome(data, welcome_metadata, &private_key, wrapper_ciphersuite)?;
        let welcome = deserialize_welcome(&welcome_bytes)?;

        let welcome_metadata = if welcome_metadata_bytes.is_empty() {
            tracing::debug!("Welcome Metadata is empty; proceeding without metadata.");
            None
        } else {
            deserialize_welcome_metadata(&welcome_metadata_bytes)
                .map_err(|e| {
                    tracing::debug!(?e, "Failed to deserialize welcome metadata; ignoring.")
                })
                .ok()
        };
        Ok((welcome, welcome_metadata))
    }
    async fn welcome_from_proto_welcome_pointer(
        welcome_pointer: &xmtp_proto::xmtp::mls::message_contents::WelcomePointer,
        context: &impl crate::context::XmtpSharedContext,
    ) -> Result<Option<(openmls::messages::Welcome, Option<WelcomeMetadata>)>, GroupError> {
        let v1 = super::super::welcome_pointer::resolve_welcome_pointer(welcome_pointer, context)
            .await?;
        let Some(v1) = v1 else {
            // Unable to resolve welcome pointer
            return Ok(None);
        };
        let welcome_pointer_v1 = match &welcome_pointer.version {
            Some(xmtp_proto::xmtp::mls::message_contents::welcome_pointer::Version::V1(v1)) => v1,
            None => {
                return Err(xmtp_proto::ConversionError::InvalidValue {
                    item: "WelcomePointer.version",
                    expected: "V1",
                    got: "None".into(),
                }
                .into());
            }
        };
        let aead_type =
            xmtp_proto::xmtp::mls::message_contents::WelcomePointeeEncryptionAeadType::try_from(
                welcome_pointer_v1.aead_type,
            )
            .map_err(|_| xmtp_proto::ConversionError::InvalidValue {
                item: "WelcomePointer::V1.aead_type",
                expected: "ChaCha20Poly1305 (3)",
                got: welcome_pointer_v1.aead_type.to_string(),
            })?;
        let aead_type = match aead_type {
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

        #[allow(clippy::get_first)]
        let nonces = [
            welcome_pointer_v1
                .nonces
                .get(0)
                .ok_or_else(|| xmtp_proto::ConversionError::InvalidValue {
                    item: "WelcomePointer::V1.nonces",
                    expected: "2 nonces",
                    got: "0 nonces".into(),
                })?
                .as_slice(),
            welcome_pointer_v1
                .nonces
                .get(1)
                .ok_or_else(|| xmtp_proto::ConversionError::InvalidValue {
                    item: "WelcomePointer::V1.nonces",
                    expected: "2 nonces",
                    got: "1 nonce".into(),
                })?
                .as_slice(),
        ];

        let decrypted_welcome = unwrap_welcome_symmetric(
            [v1.data.as_slice(), v1.welcome_metadata.as_slice()],
            aead_type,
            &welcome_pointer_v1.encryption_key,
            nonces,
        )?;
        let welcome = deserialize_welcome(&decrypted_welcome[0])?;
        let welcome_metadata = Some(decrypted_welcome[1].as_slice())
            .filter(|data| !data.is_empty())
            .map(deserialize_welcome_metadata)
            .transpose()?;

        Ok(Some((welcome, welcome_metadata)))
    }
    pub(crate) async fn from_welcome_proto(
        welcome: &ResumableWelcomeMessage,
        provider: &impl XmtpMlsStorageProvider,
        context: &impl crate::context::XmtpSharedContext,
    ) -> Result<Self, GroupError> {
        use xmtp_common::r#const::{NS_IN_DAY, NS_IN_HOUR, NS_IN_MIN};
        let (welcome, welcome_metadata) = match welcome {
            ResumableWelcomeMessage::WelcomeMessage(proto) => match proto {
                xmtp_proto::mls_v1::welcome_message::Version::V1(v1) => {
                    Self::welcome_from_proto_v1(provider, v1)?
                }
                xmtp_proto::mls_v1::welcome_message::Version::WelcomePointer(w) => {
                    let welcome_pointer = decrypt_welcome_pointer(provider, w)?;
                    let maybe_welcome =
                        Self::welcome_from_proto_welcome_pointer(&welcome_pointer, context).await?;
                    match maybe_welcome {
                        Some(welcome) => welcome,
                        None => {
                            let destination = match &welcome_pointer.version {
                                Some(xmtp_proto::xmtp::mls::message_contents::welcome_pointer::Version::V1(v1)) => {
                                    hex::encode(v1.destination.as_slice())
                                }
                                None => {
                                    return Err(xmtp_proto::ConversionError::InvalidValue { item: "WelcomePointer.version", expected: "V1", got: "None".into() }.into());
                                }
                            };
                            let now = xmtp_common::time::now_ns();
                            #[allow(clippy::unwrap_used)]
                            let task = xmtp_db::tasks::NewTaskBuilder::default()
                                .originating_message_id(Some(proto.id() as i64))
                                // use created_ns from the welcome so we can reuse it when reprocessing
                                .created_at_ns(proto.created_ns() as i64)
                                .expires_at_ns(now + NS_IN_DAY * 3)
                                .attempts(0)
                                .max_attempts(100)
                                .last_attempted_at_ns(now)
                                .backoff_scaling_factor(1.5)
                                .max_backoff_duration_ns(NS_IN_HOUR * 2)
                                .initial_backoff_duration_ns(NS_IN_MIN * 5)
                                .next_attempt_at_ns(now + NS_IN_MIN * 5)
                                .build(xmtp_proto::xmtp::mls::database::Task{
                                    task: Some(xmtp_proto::xmtp::mls::database::task::Task::ProcessWelcomePointer(welcome_pointer)),
                                })
                                // This will never fail as long as we have provided all the fields,
                                //so unwrap here to ensure we catch any issues in tests
                                .unwrap();
                            context.workers().task_channels().send(task);
                            return Err(GroupError::WelcomeDataNotFound(destination));
                        }
                    }
                }
            },
            ResumableWelcomeMessage::DecryptedWelcomePointer {
                decrypted_welcome_pointer,
                ..
            } => {
                let maybe_welcome =
                    Self::welcome_from_proto_welcome_pointer(decrypted_welcome_pointer, context)
                        .await?;
                match maybe_welcome {
                    Some(welcome) => welcome,
                    None => {
                        let destination = match &decrypted_welcome_pointer.version {
                            Some(xmtp_proto::xmtp::mls::message_contents::welcome_pointer::Version::V1(v1)) => {
                                hex::encode(v1.destination.as_slice())
                            }
                            None => {
                                return Err(xmtp_proto::ConversionError::InvalidValue { item: "WelcomePointer.version", expected: "V1", got: "None".into() }.into());
                            }
                        };
                        // This branch should only be hit if this is from a reprocessing task, so no need to create a new one
                        return Err(GroupError::WelcomeDataNotFound(destination));
                    }
                }
            }
        };

        let join_config = build_group_join_config();

        let provider = XmtpOpenMlsProviderRef::new(provider);
        let builder = StagedWelcome::build_from_welcome(&provider, &join_config, welcome.clone())?;
        let processed_welcome = builder.processed_welcome();

        let psks = processed_welcome.psks();
        if !psks.is_empty() {
            tracing::error!("No PSK support for welcome");
            return Err(GroupError::NoPSKSupport);
        }
        let staged_welcome = builder.skip_lifetime_validation().build()?;

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

fn deserialize_welcome(welcome_bytes: &Vec<u8>) -> Result<Welcome, ClientError> {
    let welcome = MlsMessageIn::tls_deserialize(&mut welcome_bytes.as_slice())?;
    match welcome.extract() {
        MlsMessageBodyIn::Welcome(welcome) => Ok(welcome),
        _ => Err(ClientError::Generic(
            "unexpected message type in welcome".to_string(),
        )),
    }
}

fn deserialize_welcome_metadata(metadata_bytes: &[u8]) -> Result<WelcomeMetadata, GroupError> {
    let metadata = WelcomeMetadata::decode(metadata_bytes).map_err(|_| {
        GroupError::Client(ClientError::Generic(
            "unexpected message type in welcome".to_string(),
        ))
    })?;
    Ok(metadata)
}

pub(crate) fn decrypt_welcome_pointer(
    provider: &impl XmtpMlsStorageProvider,
    welcome_pointer: &xmtp_proto::mls_v1::welcome_message::WelcomePointer,
) -> Result<xmtp_proto::xmtp::mls::message_contents::WelcomePointer, GroupError> {
    tracing::info!("Trying to decrypt welcome pointer");
    let hash_ref = find_key_package_hash_ref(provider, &welcome_pointer.hpke_public_key)?;
    let wrapper_algorithm =
        xmtp_proto::xmtp::mls::message_contents::WelcomePointerWrapperAlgorithm::try_from(
            welcome_pointer.wrapper_algorithm,
        )
        .map_err(|_| {
            GroupError::ConversionError(xmtp_proto::ConversionError::InvalidValue {
                item: "wrapper_algorithm",
                expected: "XwingMlkem768Draft6",
                got: welcome_pointer.wrapper_algorithm.to_string(),
            })
        })?
        .try_into()?;
    let private_key = find_private_key(provider, &hash_ref, &wrapper_algorithm)?;

    let welcome_bytes = unwrap_welcome(
        &welcome_pointer.welcome_pointer,
        &[],
        &private_key,
        wrapper_algorithm,
    )?;
    let welcome_pointer =
        <xmtp_proto::xmtp::mls::message_contents::WelcomePointer as prost::Message>::decode(
            welcome_bytes.0.as_slice(),
        )?;
    Ok(welcome_pointer)
}
