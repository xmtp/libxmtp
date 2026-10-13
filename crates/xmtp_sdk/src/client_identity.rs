use std::{collections::HashMap, sync::Arc};

use xmtp_id::associations::verify_signed_with_public_context;
use xmtp_mls::context::{ForegroundCall, XmtpSharedContext};

use crate::CatchUpSummary;
use crate::{
    Client, GroupSyncSummary, InboxId, InboxState, InstallationId, KeyPackageLifetime,
    KeyPackageStatus, PublicIdentity, SignatureRequest, Signer, XmtpError, signer,
};

fn installation_bytes(ids: &[InstallationId]) -> Result<Vec<Vec<u8>>, XmtpError> {
    ids.iter().map(InstallationId::to_bytes).collect()
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

    /// Enter the call gate for a direct client call. Hold the guard for the
    /// whole call. Drop it before a host callback, such as a signer, because
    /// the host can call end() from that callback.
    pub(crate) fn ensure_open(&self) -> Result<ForegroundCall, XmtpError> {
        crate::conversation::enter_call(&self.inner.context)
    }
}

#[xmtp_macro::sdk_export]
impl Client {
    #[sdk(immutable)]
    pub fn identity(&self) -> PublicIdentity {
        self.identity.clone()
    }

    #[sdk(immutable)]
    pub fn installation_id_bytes(&self) -> Vec<u8> {
        self.inner.installation_public_key().to_vec()
    }

    #[sdk(immutable)]
    pub fn is_in_memory(&self) -> bool {
        matches!(
            self.options.storage.location,
            crate::StorageLocation::InMemory
        )
    }

    #[sdk(immutable)]
    pub fn storage_path(&self) -> Option<String> {
        self.storage_path.clone()
    }

    #[sdk(immutable)]
    pub fn libxmtp_version(&self) -> String {
        env!("CARGO_PKG_VERSION").into()
    }

    #[sdk(immutable)]
    pub fn app_version(&self) -> Option<String> {
        self.options
            .backend
            .clone()
            .unwrap_or_default()
            .app_version()
    }

    /// The options this client was built with, without its secrets: the
    /// static credential, the credential source, and the storage encryption
    /// key are `None`. Any holder of the client can read these options, and
    /// the browser worker copies them to the page.
    #[sdk(immutable)]
    pub fn options(&self) -> crate::ClientOptions {
        let mut options = self.options.clone();
        if let Some(crate::BackendSource::Options { options: backend }) = &mut options.backend {
            backend.credential = None;
            backend.credentials = None;
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            options.storage.encryption_key = None;
        }
        options
    }

    pub async fn decode_content(
        &self,
        encoded: crate::EncodedContent,
    ) -> Result<crate::MessageContent, XmtpError> {
        crate::conversation::on_sdk_worker(self.inner.context.clone(), async move {
            use prost::Message as _;
            crate::MessageContent::decode(
                xmtp_proto::xmtp::mls::message_contents::EncodedContent::from(encoded)
                    .encode_to_vec(),
            )
        })
        .await
    }

    pub async fn register(&self) -> Result<(), XmtpError> {
        let call = self.ensure_open()?;
        if self.inner.identity().is_ready() {
            return self
                .inner
                .ensure_registration_visible()
                .await
                .map_err(XmtpError::from_client);
        }
        drop(call);
        let signer = self.signer.clone().ok_or_else(XmtpError::signer)?;
        let kind = signer::kind(signer.clone()).await?;
        self.register_with_signer(signer, kind).await
    }

    pub async fn is_registered(&self) -> Result<bool, XmtpError> {
        let _call = self.ensure_open()?;
        self.inner
            .is_registration_visible()
            .map_err(XmtpError::from_client)
    }

    pub async fn unsafe_create_inbox_signature_request(
        &self,
    ) -> Result<Option<Arc<SignatureRequest>>, XmtpError> {
        let _call = self.ensure_open()?;
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
        let _call = self.ensure_open()?;
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
        let _call = self.ensure_open()?;
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
        ids: Vec<InstallationId>,
    ) -> Result<Arc<SignatureRequest>, XmtpError> {
        let bytes = installation_bytes(&ids)?;
        let _call = self.ensure_open()?;
        let request = self
            .inner
            .identity_updates()
            .revoke_installations(bytes)
            .await
            .map_err(XmtpError::from_client)?;
        Ok(self.request(request))
    }

    pub async fn unsafe_revoke_all_other_installations_signature_request(
        &self,
    ) -> Result<Option<Arc<SignatureRequest>>, XmtpError> {
        let _call = self.ensure_open()?;
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
        let _call = self.ensure_open()?;
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
        let _call = self.ensure_open()?;
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
        ids: Vec<InstallationId>,
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
        let _call = self.ensure_open()?;
        #[cfg(test)]
        let gate = self.call_gate.lock().take();
        #[cfg(test)]
        if let Some(gate) = gate {
            gate.arrived.notify_one();
            gate.release.notified().await;
        }
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
        ids: Vec<InboxId>,
        refresh_from_network: bool,
    ) -> Result<Vec<InboxState>, XmtpError> {
        let refs = ids
            .iter()
            .map(InboxId::checked)
            .collect::<Result<Vec<_>, _>>()?;
        let _call = self.ensure_open()?;
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
    ) -> Result<Option<InboxId>, XmtpError> {
        let _call = self.ensure_open()?;
        self.inner
            .find_inbox_id_from_identifier(&self.inner.context.db(), identity.to_core()?)
            .await
            .map_err(XmtpError::from_client)?
            .map(InboxId::try_from)
            .transpose()
    }

    /// Returns one entry per core identity. Keys use `ethereum:<core text>` or
    /// `passkey:<lowercase core hex>`.
    pub async fn can_message(
        &self,
        identities: Vec<PublicIdentity>,
    ) -> Result<HashMap<String, bool>, XmtpError> {
        let _call = self.ensure_open()?;
        let core = identities
            .iter()
            .map(PublicIdentity::to_core)
            .collect::<Result<Vec<_>, _>>()?;
        let answer = self
            .inner
            .can_message(&core)
            .await
            .map_err(XmtpError::from_client)?;
        Ok(crate::signer::can_message_results(core.into_iter().map(
            |key| {
                let available = answer.get(&key).copied().unwrap_or(false);
                (key, available)
            },
        )))
    }

    pub async fn latest_inbox_updates_count(
        &self,
        ids: Vec<InboxId>,
        refresh_from_network: bool,
    ) -> Result<HashMap<String, u64>, XmtpError> {
        let refs = ids
            .iter()
            .map(InboxId::checked)
            .collect::<Result<Vec<_>, _>>()?;
        let _call = self.ensure_open()?;
        let answer = self
            .inner
            .fetch_inbox_updates_count(refresh_from_network, refs.clone())
            .await
            .map_err(XmtpError::from_client)?;
        Ok(refs
            .into_iter()
            .map(|inbox_id| {
                let count = u64::from(answer.get(inbox_id).copied().unwrap_or(0));
                (inbox_id.to_owned(), count)
            })
            .collect())
    }

    pub async fn own_inbox_updates_count(
        &self,
        refresh_from_network: bool,
    ) -> Result<u64, XmtpError> {
        let _call = self.ensure_open()?;
        self.inner
            .fetch_own_inbox_updates_count(refresh_from_network)
            .await
            .map(u64::from)
            .map_err(XmtpError::from_client)
    }

    pub async fn key_package_statuses(
        &self,
        ids: Vec<InstallationId>,
    ) -> Result<HashMap<String, KeyPackageStatus>, XmtpError> {
        let bytes = installation_bytes(&ids)?;
        let _call = self.ensure_open()?;
        let found = self
            .inner
            .get_key_packages_for_installation_ids(bytes.clone())
            .await
            .map_err(XmtpError::from_client)?;
        Ok(bytes
            .into_iter()
            .map(|bytes| {
                let status = match found.get(&bytes) {
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
                (hex::encode(bytes), status)
            })
            .collect())
    }

    pub async fn sign_with_installation_key(&self, text: String) -> Result<Vec<u8>, XmtpError> {
        let _call = self.ensure_open()?;
        self.inner
            .context
            .sign_with_public_context(text)
            .map_err(XmtpError::from_core)
    }

    pub async fn verify_signed_with_installation_key(
        &self,
        text: String,
        signature: Vec<u8>,
    ) -> Result<bool, XmtpError> {
        let _call = self.ensure_open()?;
        verify_signature(
            text,
            signature,
            self.inner.installation_public_key().to_vec(),
        )
    }

    pub async fn sync_all_device_sync_groups(&self) -> Result<GroupSyncSummary, XmtpError> {
        let _call = self.ensure_open()?;
        self.inner
            .sync_all_device_sync_groups()
            .await
            .map(Into::into)
            .map_err(XmtpError::from_client)
    }

    #[sdk(immutable)]
    pub fn server_configuration(&self) -> crate::ServerConfiguration {
        self.inner.server_configuration().into()
    }

    pub async fn refresh_server_configuration(
        &self,
    ) -> Result<crate::ServerConfiguration, XmtpError> {
        let _call = self.ensure_open()?;
        let value = self
            .inner
            .refresh_server_configuration()
            .await
            .map_err(XmtpError::from_client)?;
        Ok((&value).into())
    }

    pub async fn set_credential(&self, credential: crate::Credential) -> Result<(), XmtpError> {
        let _call = self.ensure_open()?;
        let handle = self
            .auth_handle
            .as_ref()
            .ok_or_else(|| XmtpError::invalid("credential handle is unavailable"))?;
        handle.set(credential.to_backend()?).await;
        Ok(())
    }
}

#[xmtp_macro::sdk_export]
impl Client {
    pub async fn catch_up_to_live(
        &self,
        timeout_ms: Option<u64>,
    ) -> Result<CatchUpSummary, XmtpError> {
        let _call = self.ensure_open()?;
        self.inner
            .catch_up_to_live(timeout_ms.map(std::time::Duration::from_millis))
            .await
            .map(Into::into)
            .map_err(XmtpError::from_core)
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

#[xmtp_macro::sdk_export(client_static)]
pub async fn verify_signed_with_public_key(
    text: String,
    signature: Vec<u8>,
    public_key: Vec<u8>,
) -> Result<bool, XmtpError> {
    verify_signature(text, signature, public_key)
}

#[xmtp_macro::sdk_export(client_static)]
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
