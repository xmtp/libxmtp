use std::sync::Arc;

use xmtp_id::associations::verify_signed_with_public_context;
use xmtp_mls::context::XmtpSharedContext;

#[cfg(not(target_arch = "wasm32"))]
use crate::CatchUpSummary;
use crate::{
    CanMessageEntry, Client, GroupSyncSummary, InboxCountEntry, InboxID, InboxState,
    InstallationID, KeyPackageLifetime, KeyPackageStatus, KeyPackageStatusEntry, PublicIdentity,
    SignatureRequest, Signer, XmtpError, signer,
};

fn installation_bytes(ids: Vec<InstallationID>) -> Result<Vec<Vec<u8>>, XmtpError> {
    ids.into_iter()
        .map(|id| hex::decode(id.0).map_err(XmtpError::unknown))
        .collect()
}

impl Client {
    fn request(
        &self,
        inner: xmtp_id::associations::builder::SignatureRequest,
    ) -> Arc<SignatureRequest> {
        SignatureRequest::new(inner, self.inner.clone())
    }

    async fn apply_with_signer(
        &self,
        request: Arc<SignatureRequest>,
        signer: Arc<dyn Signer>,
    ) -> Result<(), XmtpError> {
        request.sign(signer).await?;
        self.unsafe_apply_signature_request(request).await
    }

    pub(crate) fn ensure_open(&self) -> Result<(), XmtpError> {
        if self.inner.context.is_closed() {
            Err(XmtpError::closed())
        } else {
            Ok(())
        }
    }
}

#[xmtp_macro::sdk_export]
impl Client {
    pub fn identity(&self) -> PublicIdentity {
        self.identity.clone()
    }

    pub fn installation_id_bytes(&self) -> Vec<u8> {
        self.inner.installation_public_key().to_vec()
    }

    pub fn is_in_memory(&self) -> bool {
        matches!(
            self.options.storage.location,
            crate::StorageLocation::InMemory
        )
    }

    pub fn storage_path(&self) -> Option<String> {
        match &self.options.storage.location {
            crate::StorageLocation::Path(path) => Some(path.clone()),
            crate::StorageLocation::Directory(directory) => {
                #[cfg(not(target_arch = "wasm32"))]
                {
                    crate::client::native_storage_path(&self.options.storage, self.inner.inbox_id())
                        .ok()
                        .flatten()
                        .or_else(|| Some(directory.clone()))
                }
                #[cfg(target_arch = "wasm32")]
                {
                    Some(directory.clone())
                }
            }
            _ => None,
        }
    }

    pub fn libxmtp_version(&self) -> String {
        env!("CARGO_PKG_VERSION").into()
    }

    pub fn app_version(&self) -> Option<String> {
        self.options
            .backend
            .clone()
            .unwrap_or_default()
            .app_version()
    }

    pub fn options(&self) -> crate::ClientOptions {
        self.options.clone()
    }

    pub fn decode_content(&self, encoded: Vec<u8>) -> Result<crate::MessageContent, XmtpError> {
        self.ensure_open()?;
        crate::MessageContent::decode(encoded)
    }

    pub async fn register(&self) -> Result<(), XmtpError> {
        self.ensure_open()?;
        if self.inner.identity().is_ready() {
            return self
                .inner
                .ensure_registration_visible()
                .await
                .map_err(XmtpError::from_client);
        }
        let signer = self.signer.clone().ok_or_else(XmtpError::signer)?;
        let kind = signer::kind(signer.clone()).await?;
        self.register_with_signer(signer, kind).await
    }

    pub async fn is_registered(&self) -> Result<bool, XmtpError> {
        self.ensure_open()?;
        self.inner
            .is_registration_visible()
            .map_err(XmtpError::from_client)
    }

    pub async fn unsafe_create_inbox_signature_request(
        &self,
    ) -> Result<Option<Arc<SignatureRequest>>, XmtpError> {
        self.ensure_open()?;
        match self.inner.identity().signature_request() {
            Some(request) => Ok(Some(self.request(request))),
            None => {
                self.inner
                    .ensure_registration_visible()
                    .await
                    .map_err(XmtpError::from_client)?;
                Ok(None)
            }
        }
    }

    pub async fn unsafe_add_account_signature_request(
        &self,
        identity: PublicIdentity,
        allow_inbox_reassign: bool,
    ) -> Result<Arc<SignatureRequest>, XmtpError> {
        self.ensure_open()?;
        let identifier = identity.to_core()?;
        if !allow_inbox_reassign {
            let found = self
                .inner
                .find_inbox_id_from_identifier(&self.inner.context.db(), identifier.clone())
                .await
                .map_err(XmtpError::from_client)?;
            if found.is_some_and(|inbox| inbox != self.inner.inbox_id()) {
                return Err(XmtpError::invalid("identity belongs to another inbox"));
            }
        }
        let request = self
            .inner
            .identity_updates()
            .associate_identity(identifier)
            .await
            .map_err(XmtpError::from_client)?;
        Ok(self.request(request))
    }

    pub async fn unsafe_remove_account_signature_request(
        &self,
        identity: PublicIdentity,
    ) -> Result<Arc<SignatureRequest>, XmtpError> {
        self.ensure_open()?;
        let request = self
            .inner
            .identity_updates()
            .revoke_identities(vec![identity.to_core()?])
            .await
            .map_err(XmtpError::from_client)?;
        Ok(self.request(request))
    }

    pub async fn unsafe_revoke_installations_signature_request(
        &self,
        ids: Vec<InstallationID>,
    ) -> Result<Arc<SignatureRequest>, XmtpError> {
        self.ensure_open()?;
        let request = self
            .inner
            .identity_updates()
            .revoke_installations(installation_bytes(ids)?)
            .await
            .map_err(XmtpError::from_client)?;
        Ok(self.request(request))
    }

    pub async fn unsafe_revoke_all_other_installations_signature_request(
        &self,
    ) -> Result<Option<Arc<SignatureRequest>>, XmtpError> {
        self.ensure_open()?;
        let current = self.inner.installation_public_key().to_vec();
        let state = self
            .inner
            .inbox_state(true)
            .await
            .map_err(XmtpError::from_client)?;
        let ids = state
            .installation_ids()
            .into_iter()
            .filter(|id| id.as_slice() != current.as_slice())
            .collect::<Vec<_>>();
        if ids.is_empty() {
            return Ok(None);
        }
        let request = self
            .inner
            .identity_updates()
            .revoke_installations(ids)
            .await
            .map_err(XmtpError::from_client)?;
        Ok(Some(self.request(request)))
    }

    pub async fn unsafe_change_recovery_identifier_signature_request(
        &self,
        identity: PublicIdentity,
    ) -> Result<Arc<SignatureRequest>, XmtpError> {
        self.ensure_open()?;
        let request = self
            .inner
            .identity_updates()
            .change_recovery_identifier(identity.to_core()?)
            .await
            .map_err(XmtpError::from_client)?;
        Ok(self.request(request))
    }

    pub async fn unsafe_apply_signature_request(
        &self,
        request: Arc<SignatureRequest>,
    ) -> Result<(), XmtpError> {
        self.ensure_open()?;
        if !request.belongs_to(&self.inner) {
            return Err(XmtpError::invalid(
                "signature request belongs to another client",
            ));
        }
        let inner = request.clone_inner().await;
        if self.inner.identity().is_ready() {
            self.inner
                .identity_updates()
                .apply_signature_request(inner)
                .await
                .map_err(XmtpError::from_client)
        } else {
            self.inner
                .register_identity(inner)
                .await
                .map_err(XmtpError::from_client)
        }
    }

    pub async fn unsafe_add_account(
        &self,
        signer: Arc<dyn Signer>,
        allow_inbox_reassign: bool,
    ) -> Result<(), XmtpError> {
        let identity = signer::identity(signer.clone()).await?;
        let request = self
            .unsafe_add_account_signature_request(identity, allow_inbox_reassign)
            .await?;
        self.apply_with_signer(request, signer).await
    }

    pub async fn remove_account(
        &self,
        recovery_signer: Arc<dyn Signer>,
        identity: PublicIdentity,
    ) -> Result<(), XmtpError> {
        let request = self
            .unsafe_remove_account_signature_request(identity)
            .await?;
        self.apply_with_signer(request, recovery_signer).await
    }

    pub async fn revoke_installations(
        &self,
        signer: Arc<dyn Signer>,
        ids: Vec<InstallationID>,
    ) -> Result<(), XmtpError> {
        let request = self
            .unsafe_revoke_installations_signature_request(ids)
            .await?;
        self.apply_with_signer(request, signer).await
    }

    pub async fn revoke_all_other_installations(
        &self,
        signer: Arc<dyn Signer>,
    ) -> Result<(), XmtpError> {
        if let Some(request) = self
            .unsafe_revoke_all_other_installations_signature_request()
            .await?
        {
            self.apply_with_signer(request, signer).await?;
        }
        Ok(())
    }

    pub async fn change_recovery_identifier(
        &self,
        signer: Arc<dyn Signer>,
        identity: PublicIdentity,
    ) -> Result<(), XmtpError> {
        let request = self
            .unsafe_change_recovery_identifier_signature_request(identity)
            .await?;
        self.apply_with_signer(request, signer).await
    }

    pub async fn inbox_state(&self, refresh_from_network: bool) -> Result<InboxState, XmtpError> {
        self.ensure_open()?;
        let state = self
            .inner
            .inbox_state(refresh_from_network)
            .await
            .map_err(XmtpError::from_client)?;
        let kind = self
            .inner
            .inbox_creation_signature_kind(self.inner.inbox_id(), false)
            .await
            .map_err(XmtpError::from_client)?;
        InboxState::from_core(state, kind)
    }

    pub async fn inbox_states(
        &self,
        ids: Vec<InboxID>,
        refresh_from_network: bool,
    ) -> Result<Vec<InboxState>, XmtpError> {
        self.ensure_open()?;
        let refs = ids.iter().map(|id| id.0.as_str()).collect();
        let states = self
            .inner
            .inbox_addresses(refresh_from_network, refs)
            .await
            .map_err(XmtpError::from_client)?;
        let mut result = Vec::with_capacity(states.len());
        for state in states {
            let kind = self
                .inner
                .inbox_creation_signature_kind(state.inbox_id(), false)
                .await
                .map_err(XmtpError::from_client)?;
            result.push(InboxState::from_core(state, kind)?);
        }
        Ok(result)
    }

    pub async fn inbox_id_for(
        &self,
        identity: PublicIdentity,
    ) -> Result<Option<InboxID>, XmtpError> {
        self.ensure_open()?;
        self.inner
            .find_inbox_id_from_identifier(&self.inner.context.db(), identity.to_core()?)
            .await
            .map_err(XmtpError::from_client)?
            .map(InboxID::try_from)
            .transpose()
    }

    pub async fn can_message(
        &self,
        identities: Vec<PublicIdentity>,
    ) -> Result<Vec<CanMessageEntry>, XmtpError> {
        self.ensure_open()?;
        let core = identities
            .iter()
            .map(PublicIdentity::to_core)
            .collect::<Result<Vec<_>, _>>()?;
        let answer = self
            .inner
            .can_message(&core)
            .await
            .map_err(XmtpError::from_client)?;
        Ok(identities
            .into_iter()
            .zip(core)
            .map(|(identity, key)| CanMessageEntry {
                identity,
                can_message: answer.get(&key).copied().unwrap_or(false),
            })
            .collect())
    }

    pub async fn latest_inbox_updates_count(
        &self,
        ids: Vec<InboxID>,
        refresh_from_network: bool,
    ) -> Result<Vec<InboxCountEntry>, XmtpError> {
        self.ensure_open()?;
        let refs = ids.iter().map(|id| id.0.as_str()).collect();
        let answer = self
            .inner
            .fetch_inbox_updates_count(refresh_from_network, refs)
            .await
            .map_err(XmtpError::from_client)?;
        Ok(ids
            .into_iter()
            .map(|inbox_id| InboxCountEntry {
                count: u64::from(answer.get(&inbox_id.0).copied().unwrap_or(0)),
                inbox_id,
            })
            .collect())
    }

    pub async fn own_inbox_updates_count(
        &self,
        refresh_from_network: bool,
    ) -> Result<u64, XmtpError> {
        self.ensure_open()?;
        self.inner
            .fetch_own_inbox_updates_count(refresh_from_network)
            .await
            .map(u64::from)
            .map_err(XmtpError::from_client)
    }

    pub async fn key_package_statuses(
        &self,
        ids: Vec<InstallationID>,
    ) -> Result<Vec<KeyPackageStatusEntry>, XmtpError> {
        self.ensure_open()?;
        let found = self
            .inner
            .get_key_packages_for_installation_ids(installation_bytes(ids.clone())?)
            .await
            .map_err(XmtpError::from_client)?;
        Ok(ids
            .into_iter()
            .map(|installation_id| {
                let status = match hex::decode(&installation_id.0)
                    .ok()
                    .and_then(|id| found.get(&id))
                {
                    Some(Ok(package)) => KeyPackageStatus {
                        lifetime: package.life_time().map(|value| KeyPackageLifetime {
                            not_before: value.not_before,
                            not_after: value.not_after,
                        }),
                        validation_error: None,
                    },
                    Some(Err(error)) => KeyPackageStatus {
                        lifetime: None,
                        validation_error: Some(error.to_string()),
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

    pub async fn sign_with_installation_key(&self, text: String) -> Result<Vec<u8>, XmtpError> {
        self.ensure_open()?;
        self.inner
            .context
            .sign_with_public_context(text)
            .map_err(XmtpError::unknown)
    }

    pub async fn verify_signed_with_installation_key(
        &self,
        text: String,
        signature: Vec<u8>,
    ) -> Result<bool, XmtpError> {
        self.ensure_open()?;
        verify_signature(
            text,
            signature,
            self.inner.installation_public_key().to_vec(),
        )
    }

    pub async fn sync_all_device_sync_groups(&self) -> Result<GroupSyncSummary, XmtpError> {
        self.ensure_open()?;
        self.inner
            .sync_all_device_sync_groups()
            .await
            .map(Into::into)
            .map_err(XmtpError::from_client)
    }

    pub fn server_configuration(&self) -> crate::ServerConfiguration {
        self.inner.server_configuration().into()
    }

    pub async fn refresh_server_configuration(
        &self,
    ) -> Result<crate::ServerConfiguration, XmtpError> {
        self.ensure_open()?;
        let value = self
            .inner
            .refresh_server_configuration()
            .await
            .map_err(XmtpError::from_client)?;
        Ok((&value).into())
    }

    pub async fn set_credential(&self, credential: crate::Credential) -> Result<(), XmtpError> {
        self.ensure_open()?;
        let handle = self
            .auth_handle
            .as_ref()
            .ok_or_else(|| XmtpError::invalid("credential handle is unavailable"))?;
        handle.set(credential.to_backend()?).await;
        Ok(())
    }
}

#[xmtp_macro::sdk_export(native_only)]
impl Client {
    pub async fn catch_up_to_live(
        &self,
        timeout_ms: Option<u64>,
    ) -> Result<CatchUpSummary, XmtpError> {
        self.ensure_open()?;
        self.inner
            .catch_up_to_live(timeout_ms.map(std::time::Duration::from_millis))
            .await
            .map(Into::into)
            .map_err(XmtpError::unknown)
    }
}

fn verify_signature(
    text: String,
    signature: Vec<u8>,
    public_key: Vec<u8>,
) -> Result<bool, XmtpError> {
    let signature: [u8; 64] = signature
        .try_into()
        .map_err(|_| XmtpError::invalid("signature must be 64 bytes"))?;
    let public_key: [u8; 32] = public_key
        .try_into()
        .map_err(|_| XmtpError::invalid("public key must be 32 bytes"))?;
    Ok(verify_signed_with_public_context(text, &signature, &public_key).is_ok())
}

#[xmtp_macro::sdk_export]
pub async fn verify_signed_with_public_key(
    text: String,
    signature: Vec<u8>,
    public_key: Vec<u8>,
) -> Result<bool, XmtpError> {
    verify_signature(text, signature, public_key)
}

#[xmtp_macro::sdk_export]
pub async fn fetch_server_configuration(
    backend: crate::BackendSource,
) -> Result<crate::ServerConfiguration, XmtpError> {
    let backend = backend.resolve().await?;
    let api = xmtp_api::ApiClientWrapper::new(backend.api.clone(), Default::default());
    let value = xmtp_mls::server_configuration::fetch_server_configuration(&api)
        .await
        .map_err(XmtpError::from_client)?;
    Ok((&value).into())
}
