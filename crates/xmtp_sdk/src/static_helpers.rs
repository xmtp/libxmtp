use std::{collections::HashMap, sync::Arc};

use xmtp_id::associations::{
    MemberIdentifier,
    unverified::{NewUnverifiedSmartContractWalletSignature, UnverifiedSignature},
};
use xmtp_id::key_package::VerifiedKeyPackageV2;
use xmtp_id::scw_verifier::SmartContractSignatureVerifier;
use xmtp_proto::types::{ApiIdentifier, GroupId, InstallationId as CoreInstallationId};

use crate::{
    Backend, BackendSource, ConversationId, InboxId, InboxState, InstallationId,
    KeyPackageLifetime, KeyPackageStatus, PublicIdentity, Signature, Signer, SigningRequest,
    Timestamp, XmtpError, signer,
};

fn api(backend: &Backend) -> xmtp_api::ApiClientWrapper<xmtp_mls::XmtpApiClient> {
    xmtp_api::ApiClientWrapper::new(backend.api.clone(), Default::default())
}

/// Read inbox update counts without opening a client or local storage.
#[xmtp_macro::sdk_export]
pub async fn latest_inbox_updates_count(
    inbox_ids: Vec<InboxId>,
    backend: BackendSource,
) -> Result<HashMap<String, u64>, XmtpError> {
    let filters = inbox_ids
        .iter()
        .map(|id| {
            Ok(xmtp_api::GetIdentityUpdatesV2Filter {
                inbox_id: id.checked()?.to_owned(),
                sequence_id: None,
            })
        })
        .collect::<Result<Vec<_>, XmtpError>>()?;
    let backend = backend.resolve().await?;
    let updates = api(&backend)
        .get_identity_updates_v2(filters)
        .await
        .map_err(XmtpError::from_api)?;
    Ok(updates
        .into_iter()
        .map(|(id, values)| (id, values.len() as u64))
        .collect())
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct MessageMetadataEntry {
    pub sequence_id: u64,
    pub created_at: Timestamp,
}

#[xmtp_macro::sdk_export(client_static)]
/// Returns one entry per core identity. Keys use `ethereum:<core text>` or
/// `passkey:<lowercase core hex>`.
pub async fn can_message_with_backend(
    backend: BackendSource,
    identities: Vec<PublicIdentity>,
) -> Result<HashMap<String, bool>, XmtpError> {
    let backend = backend.resolve().await?;
    let core = identities
        .iter()
        .map(PublicIdentity::to_core)
        .collect::<Result<Vec<_>, _>>()?;
    let ids: Vec<ApiIdentifier> = core.iter().cloned().map(Into::into).collect();
    let found = api(&backend)
        .get_inbox_ids(ids)
        .await
        .map_err(XmtpError::from_api)?;
    Ok(signer::can_message_results(
        core.into_iter()
            .zip(found.into_iter().map(|inbox| inbox.is_some())),
    ))
}

#[xmtp_macro::sdk_export(client_static)]
pub async fn inbox_id_for_with_backend(
    backend: BackendSource,
    identity: PublicIdentity,
) -> Result<InboxId, XmtpError> {
    let backend = backend.resolve().await?;
    let identifier = identity.to_core()?;
    let found = api(&backend)
        .get_inbox_ids(vec![identifier.clone().into()])
        .await
        .map_err(XmtpError::from_api)?;
    let inbox = match found.into_iter().next().flatten() {
        Some(value) => value,
        None => identifier.inbox_id(0).map_err(XmtpError::from_core)?,
    };
    InboxId::try_from(inbox)
}

#[xmtp_macro::sdk_export(client_static)]
pub async fn inbox_states_with_backend(
    backend: BackendSource,
    ids: Vec<InboxId>,
) -> Result<Vec<InboxState>, XmtpError> {
    let refs = ids
        .iter()
        .map(InboxId::checked)
        .collect::<Result<Vec<_>, _>>()?;
    let backend = backend.resolve().await?;
    let store = crate::client::open_store(
        &crate::StorageOptions {
            location: crate::StorageLocation::InMemory,
            ..Default::default()
        },
        None,
    )
    .await?;
    let api = api(&backend);
    let verifier = Box::new(api.clone()) as Box<dyn SmartContractSignatureVerifier>;
    let states =
        xmtp_mls::client::inbox_addresses_with_verifier(&api, &store.db(), refs, &verifier)
            .await
            .map_err(XmtpError::from_client)?;
    states
        .into_iter()
        .map(|state| InboxState::from_core(state, None))
        .collect()
}

#[xmtp_macro::sdk_export(client_static)]
pub async fn key_package_statuses_with_backend(
    backend: BackendSource,
    ids: Vec<InstallationId>,
) -> Result<HashMap<String, KeyPackageStatus>, XmtpError> {
    let installations = ids
        .iter()
        .map(|id| CoreInstallationId::try_from(id.to_bytes()?).map_err(XmtpError::from_core))
        .collect::<Result<Vec<_>, _>>()?;
    let backend = backend.resolve().await?;
    let found = api(&backend)
        .fetch_key_packages(&installations)
        .await
        .map_err(XmtpError::from_api)?;
    let crypto = xmtp_db::XmtpOpenMlsProvider::<()>::new_crypto();
    Ok(installations
        .into_iter()
        .map(|key| {
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
            (hex::encode(Vec::<u8>::from(key)), status)
        })
        .collect())
}

#[xmtp_macro::sdk_export(client_static)]
pub async fn newest_message_metadata_with_backend(
    backend: BackendSource,
    ids: Vec<ConversationId>,
) -> Result<HashMap<String, MessageMetadataEntry>, XmtpError> {
    let groups = ids
        .into_iter()
        .map(GroupId::try_from)
        .collect::<Result<Vec<_>, _>>()?;
    let backend = backend.resolve().await?;
    let found = api(&backend)
        .get_newest_message_metadata(&groups)
        .await
        .map_err(XmtpError::from_api)?;
    found
        .into_values()
        .map(|value| {
            let created = value
                .created_ns
                .timestamp_nanos_opt()
                .ok_or_else(|| XmtpError::invalid("message time exceeds i64"))?;
            Ok((
                hex::encode(value.group_id.as_slice()),
                MessageMetadataEntry {
                    sequence_id: value.cursor.0,
                    created_at: Timestamp(created),
                },
            ))
        })
        .collect()
}

#[xmtp_macro::sdk_export(client_static)]
pub async fn is_address_authorized_with_backend(
    backend: BackendSource,
    inbox_id: InboxId,
    address: String,
) -> Result<bool, XmtpError> {
    let inbox_id = inbox_id.checked()?;
    let backend = backend.resolve().await?;
    let member = MemberIdentifier::eth(address).map_err(XmtpError::from_core)?;
    xmtp_mls::identity_updates::is_member_of_association_state(
        &api(&backend),
        inbox_id,
        &member,
        None,
    )
    .await
    .map_err(XmtpError::from_client)
}

#[xmtp_macro::sdk_export(client_static)]
pub async fn is_installation_authorized_with_backend(
    backend: BackendSource,
    inbox_id: InboxId,
    installation_id: InstallationId,
) -> Result<bool, XmtpError> {
    let inbox_id = inbox_id.checked()?;
    let member = MemberIdentifier::installation(installation_id.to_bytes()?);
    let backend = backend.resolve().await?;
    xmtp_mls::identity_updates::is_member_of_association_state(
        &api(&backend),
        inbox_id,
        &member,
        None,
    )
    .await
    .map_err(XmtpError::from_client)
}

#[xmtp_macro::sdk_export(client_static)]
pub async fn revoke_installations_with_backend(
    backend: BackendSource,
    signer: Arc<dyn Signer>,
    inbox_id: InboxId,
    ids: Vec<InstallationId>,
) -> Result<(), XmtpError> {
    let inbox_id = inbox_id.checked()?;
    let installations = ids
        .iter()
        .map(InstallationId::to_bytes)
        .collect::<Result<Vec<_>, _>>()?;
    let backend = backend.resolve().await?;
    let identity = signer::identity(signer.clone()).await?.to_core()?;
    let mut request = xmtp_mls::identity_updates::revoke_installations_with_verifier(
        &identity,
        inbox_id,
        installations,
    )
    .map_err(XmtpError::from_client)?;
    let api = api(&backend);
    let configuration = xmtp_mls::server_configuration::fetch_server_configuration(&api)
        .await
        .map_err(XmtpError::from_client)?;
    request.restrict_chains(configuration.smart_contract_wallet_chains.clone().into());
    let signature = signer::sign(
        signer,
        SigningRequest {
            text: request.signature_text(),
        },
    )
    .await?;
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
        None,
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
