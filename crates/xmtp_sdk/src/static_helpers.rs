use std::sync::Arc;

use xmtp_id::associations::{
    MemberIdentifier,
    unverified::{NewUnverifiedSmartContractWalletSignature, UnverifiedSignature},
};
use xmtp_id::key_package::VerifiedKeyPackageV2;
use xmtp_id::scw_verifier::SmartContractSignatureVerifier;
use xmtp_proto::types::{ApiIdentifier, GroupId, InstallationId};

use crate::{
    Backend, CanMessageEntry, ConversationID, InboxID, InboxState, InstallationID,
    KeyPackageLifetime, KeyPackageStatus, KeyPackageStatusEntry, PublicIdentity, Signature, Signer,
    SigningRequest, Timestamp, XmtpError, signer,
};

fn api(backend: &Backend) -> xmtp_api::ApiClientWrapper<xmtp_mls::XmtpApiClient> {
    xmtp_api::ApiClientWrapper::new(backend.api.clone(), Default::default())
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct MessageMetadataEntry {
    pub conversation_id: ConversationID,
    pub sequence_id: u64,
    pub created_at: Timestamp,
}

#[xmtp_macro::sdk_export]
pub async fn can_message_with_backend(
    backend: Arc<Backend>,
    identities: Vec<PublicIdentity>,
) -> Result<Vec<CanMessageEntry>, XmtpError> {
    let core = identities
        .iter()
        .map(PublicIdentity::to_core)
        .collect::<Result<Vec<_>, _>>()?;
    let ids: Vec<ApiIdentifier> = core.iter().cloned().map(Into::into).collect();
    let found = api(&backend)
        .get_inbox_ids(ids)
        .await
        .map_err(XmtpError::unknown)?;
    Ok(identities
        .into_iter()
        .zip(found)
        .map(|(identity, inbox)| CanMessageEntry {
            identity,
            can_message: inbox.is_some(),
        })
        .collect())
}

#[xmtp_macro::sdk_export]
pub async fn inbox_id_for_with_backend(
    backend: Arc<Backend>,
    identity: PublicIdentity,
) -> Result<InboxID, XmtpError> {
    let identifier = identity.to_core()?;
    let found = api(&backend)
        .get_inbox_ids(vec![identifier.clone().into()])
        .await
        .map_err(XmtpError::unknown)?;
    let inbox = match found.into_iter().next().flatten() {
        Some(value) => value,
        None => identifier.inbox_id(0).map_err(XmtpError::unknown)?,
    };
    InboxID::try_from(inbox)
}

#[xmtp_macro::sdk_export]
pub async fn inbox_states_with_backend(
    backend: Arc<Backend>,
    ids: Vec<InboxID>,
) -> Result<Vec<InboxState>, XmtpError> {
    let store = crate::client::open_store(
        &crate::StorageOptions {
            location: crate::StorageLocation::InMemory,
            ..Default::default()
        },
        "static-inbox-states",
    )
    .await?;
    let api = api(&backend);
    let verifier = Box::new(api.clone()) as Box<dyn SmartContractSignatureVerifier>;
    let refs = ids.iter().map(|id| id.0.as_str()).collect();
    let states =
        xmtp_mls::client::inbox_addresses_with_verifier(&api, &store.db(), refs, &verifier)
            .await
            .map_err(XmtpError::from_client)?;
    states
        .into_iter()
        .map(|state| InboxState::from_core(state, None))
        .collect()
}

#[xmtp_macro::sdk_export]
pub async fn key_package_statuses_with_backend(
    backend: Arc<Backend>,
    ids: Vec<InstallationID>,
) -> Result<Vec<KeyPackageStatusEntry>, XmtpError> {
    let installations = ids
        .iter()
        .map(|id| hex::decode(&id.0).map_err(XmtpError::unknown))
        .map(|value| {
            value.and_then(|bytes| InstallationId::try_from(bytes).map_err(XmtpError::unknown))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let found = api(&backend)
        .fetch_key_packages(&installations)
        .await
        .map_err(XmtpError::unknown)?;
    let crypto = xmtp_db::XmtpOpenMlsProvider::<()>::new_crypto();
    Ok(ids
        .into_iter()
        .zip(installations)
        .map(|(installation_id, key)| {
            let status = match found.get(&key).and_then(Option::as_ref) {
                Some(package) => match VerifiedKeyPackageV2::from_bytes(
                    &crypto,
                    &package.key_package_tls_serialized,
                ) {
                    Ok(package) => KeyPackageStatus {
                        lifetime: package.life_time().map(|value| KeyPackageLifetime {
                            not_before: value.not_before,
                            not_after: value.not_after,
                        }),
                        validation_error: None,
                    },
                    Err(error) => KeyPackageStatus {
                        lifetime: None,
                        validation_error: Some(error.to_string()),
                    },
                },
                None => KeyPackageStatus {
                    lifetime: None,
                    validation_error: Some("key package not found".into()),
                },
            };
            KeyPackageStatusEntry {
                installation_id,
                status,
            }
        })
        .collect())
}

#[xmtp_macro::sdk_export]
pub async fn newest_message_metadata_with_backend(
    backend: Arc<Backend>,
    ids: Vec<ConversationID>,
) -> Result<Vec<MessageMetadataEntry>, XmtpError> {
    let groups = ids
        .iter()
        .cloned()
        .map(GroupId::try_from)
        .collect::<Result<Vec<_>, _>>()?;
    let found = api(&backend)
        .get_newest_message_metadata(&groups)
        .await
        .map_err(XmtpError::unknown)?;
    found
        .into_values()
        .map(|value| {
            let created = value
                .created_ns
                .timestamp_nanos_opt()
                .ok_or_else(|| XmtpError::invalid("message time exceeds i64"))?;
            Ok(MessageMetadataEntry {
                conversation_id: value.group_id.into(),
                sequence_id: value.cursor.0,
                created_at: Timestamp(created),
            })
        })
        .collect()
}

#[xmtp_macro::sdk_export]
pub async fn is_address_authorized_with_backend(
    backend: Arc<Backend>,
    inbox_id: InboxID,
    address: String,
) -> Result<bool, XmtpError> {
    let member = MemberIdentifier::eth(address).map_err(XmtpError::unknown)?;
    xmtp_mls::identity_updates::is_member_of_association_state(
        &api(&backend),
        &inbox_id.0,
        &member,
        None,
    )
    .await
    .map_err(XmtpError::from_client)
}

#[xmtp_macro::sdk_export]
pub async fn is_installation_authorized_with_backend(
    backend: Arc<Backend>,
    inbox_id: InboxID,
    installation_id: InstallationID,
) -> Result<bool, XmtpError> {
    let member =
        MemberIdentifier::installation(hex::decode(installation_id.0).map_err(XmtpError::unknown)?);
    xmtp_mls::identity_updates::is_member_of_association_state(
        &api(&backend),
        &inbox_id.0,
        &member,
        None,
    )
    .await
    .map_err(XmtpError::from_client)
}

#[xmtp_macro::sdk_export]
pub async fn revoke_installations_with_backend(
    backend: Arc<Backend>,
    signer: Arc<dyn Signer>,
    inbox_id: InboxID,
    ids: Vec<InstallationID>,
) -> Result<(), XmtpError> {
    let identity = signer::identity(signer.clone()).await?.to_core()?;
    let installations = ids
        .into_iter()
        .map(|id| hex::decode(id.0).map_err(XmtpError::unknown))
        .collect::<Result<Vec<_>, _>>()?;
    let mut request = xmtp_mls::identity_updates::revoke_installations_with_verifier(
        &identity,
        &inbox_id.0,
        installations,
    )
    .map_err(XmtpError::from_client)?;
    let signature = signer::sign(
        signer,
        SigningRequest {
            text: request.signature_text(),
        },
    )
    .await?;
    let api = api(&backend);
    let verifier = Box::new(api.clone()) as Box<dyn SmartContractSignatureVerifier>;
    match signature {
        Signature::Ecdsa(bytes) => request
            .add_signature(UnverifiedSignature::new_recoverable_ecdsa(bytes), &verifier)
            .await
            .map_err(XmtpError::from_signature_request)?,
        Signature::Passkey {
            signature,
            public_key,
            authenticator_data,
            client_data_json,
        } => request
            .add_signature(
                UnverifiedSignature::new_passkey(
                    public_key,
                    signature,
                    authenticator_data,
                    client_data_json,
                ),
                &verifier,
            )
            .await
            .map_err(XmtpError::from_signature_request)?,
        Signature::Scw {
            bytes,
            address,
            chain_id,
            block_number,
        } => request
            .add_new_unverified_smart_contract_signature(
                NewUnverifiedSmartContractWalletSignature::new(
                    bytes,
                    xmtp_id::associations::AccountId::new_evm(chain_id, address),
                    block_number,
                ),
                &verifier,
            )
            .await
            .map_err(XmtpError::from_signature_request)?,
    }
    let store = crate::client::open_store(
        &crate::StorageOptions {
            location: crate::StorageLocation::InMemory,
            ..Default::default()
        },
        &inbox_id.0,
    )
    .await?;
    xmtp_mls::identity_updates::apply_signature_request_with_verifier(
        &api,
        &store.db(),
        request,
        &verifier,
    )
    .await
    .map_err(XmtpError::from_client)
}
