//! Creating an identity and keeping its key packages: the identity-owned
//! constructors and key package rotation over the [`Identity`] struct.

use crate::XmtpApi;
use crate::core::api_status::definite_refusal;
use crate::identity::{
    Identity, IdentityError, NewKeyPackageResult, XmtpKeyPackage, create_credential,
    serialize_key_package_hash_ref,
};
use crate::identity_updates::{get_association_state_with_verifier, load_identity_updates};
use crate::mls_store::verify_served_key_package;
use crate::worker::key_package_maintenance::discard_key_package;
use openmls::prelude::tls_codec::Serialize;
use std::sync::atomic::AtomicBool;
use xmtp_api::ApiClientWrapper;
use xmtp_configuration::KEY_PACKAGE_ROTATION_INTERVAL_NS;
use xmtp_cryptography::{CredentialSign, XmtpInstallationCredential};
use xmtp_db::TransactionOutcome::Continue;
use xmtp_db::{MlsProviderExt, StorageError, TransactionOutcome};
use xmtp_db::{XmtpMlsStorageProvider, XmtpOpenMlsProviderRef, prelude::*};
use xmtp_id::associations::unverified::UnverifiedSignature;
use xmtp_id::associations::{Identifier, InstallationKeyContext};
use xmtp_id::scw_verifier::SmartContractSignatureVerifier;
use xmtp_id::{
    InboxId,
    associations::{MemberIdentifier, builder::SignatureRequestBuilder},
};

/// Create a new [Identity] instance.
///
/// If the address is already associated with an inbox_id, the existing inbox_id will be used.
/// If it is not, a new inbox_id is generated from the identifier and nonce.
/// Either way, the caller must sign with their wallet.
#[tracing::instrument(level = "trace", skip_all)]
pub(crate) async fn new_identity<ApiClient: XmtpApi, S: XmtpMlsStorageProvider>(
    inbox_id: InboxId,
    identifier: Identifier,
    nonce: u64,
    api_client: &ApiClientWrapper<ApiClient>,
    mls_storage: &S,
    scw_signature_verifier: impl SmartContractSignatureVerifier,
) -> Result<Identity, IdentityError> {
    // check if address is already associated with an inbox_id
    let inbox_ids = api_client
        .get_inbox_ids(vec![identifier.clone().into()])
        .await?;
    let associated_inbox_id = inbox_ids.first().and_then(Option::as_ref);
    let installation_keys = XmtpInstallationCredential::new();

    if let Some(associated_inbox_id) = associated_inbox_id {
        // If an inbox is associated with address, we'd use it to create Identity and ignore the nonce.
        // We would need a signature from user's wallet.
        if *associated_inbox_id != inbox_id {
            return Err(IdentityError::NewIdentity("Inbox ID mismatch".to_string()));
        }

        // get sequence_id from identity updates and loaded into the DB
        load_identity_updates(
            api_client,
            &mls_storage.db(),
            &[associated_inbox_id.as_str()],
        )
        .await
        .map_err(|e| IdentityError::NewIdentity(format!("Failed to load identity updates: {e}")))?;

        let state = get_association_state_with_verifier(
            &mls_storage.db(),
            &inbox_id,
            None,
            &scw_signature_verifier,
        )
        .await
        .map_err(|err| {
            IdentityError::NewIdentity(format!("Error resolving identity state: {}", err))
        })?;

        // The deployment sets the ceiling; the wrapper carries the
        // snapshot resolved before any identity work ran.
        let max_installations = api_client.configuration().mls.max_installations_per_inbox;
        let current_installation_count = state.installation_ids().len();
        // implements: CONF-044
        if current_installation_count >= max_installations {
            return Err(IdentityError::TooManyInstallations {
                inbox_id: associated_inbox_id.clone(),
                count: current_installation_count,
                max: max_installations,
            });
        }

        let builder = SignatureRequestBuilder::new(associated_inbox_id.clone());
        let mut signature_request = builder
            .add_association(
                MemberIdentifier::installation(installation_keys.public_slice().to_vec()),
                identifier.clone().into(),
            )
            .build();

        let signature = installation_keys
            .credential_sign::<InstallationKeyContext>(signature_request.signature_text())?;
        signature_request
            .add_signature(
                UnverifiedSignature::new_installation_key(
                    signature,
                    installation_keys.verifying_key(),
                ),
                &scw_signature_verifier,
            )
            .await?;

        let identity = Identity {
            inbox_id: associated_inbox_id.clone(),
            installation_keys,
            credential: create_credential(associated_inbox_id.clone())?,
            signature_request: Some(signature_request),
            is_ready: AtomicBool::new(false),
        };

        Ok(identity)
    } else {
        let generated_inbox_id = identifier.inbox_id(nonce)?;
        if inbox_id != generated_inbox_id {
            return Err(IdentityError::NewIdentity(
                "Inbox ID doesn't match nonce & address".to_string(),
            ));
        }
        create_inbox(
            identifier,
            nonce,
            installation_keys,
            &scw_signature_verifier,
        )
        .await
    }
}

/// An unregistered identity that creates the inbox of `identifier` and
/// `nonce` with `installation_keys`. The request is pre-signed with the
/// installation key; the caller adds the wallet signature.
async fn create_inbox(
    identifier: Identifier,
    nonce: u64,
    installation_keys: XmtpInstallationCredential,
    scw_signature_verifier: impl SmartContractSignatureVerifier,
) -> Result<Identity, IdentityError> {
    let inbox_id = identifier.inbox_id(nonce)?;
    let mut builder = SignatureRequestBuilder::new(inbox_id.clone());
    builder = builder.create_inbox(identifier.clone(), nonce);

    let mut signature_request = builder
        .add_association(
            MemberIdentifier::installation(installation_keys.public_slice().to_vec()),
            identifier.into(),
        )
        .build();

    // We can pre-sign the request with an installation key signature, since we have access to the key
    let sig = installation_keys
        .credential_sign::<InstallationKeyContext>(signature_request.signature_text())?;
    signature_request
        .add_signature(
            UnverifiedSignature::new_installation_key(sig, installation_keys.verifying_key()),
            &scw_signature_verifier,
        )
        .await?;

    Ok(Identity {
        credential: create_credential(inbox_id.clone())?,
        inbox_id,
        installation_keys,
        signature_request: Some(signature_request),
        is_ready: AtomicBool::new(false),
    })
}

/// An unregistered identity for a new inbox that uses the given
/// installation key. Use it with [`IdentityStrategy::ExternalIdentity`],
/// then register with the owner of `identifier`.
#[cfg(any(test, feature = "test-utils"))]
pub async fn new_inbox_with_installation_keys(
    identifier: Identifier,
    nonce: u64,
    installation_keys: XmtpInstallationCredential,
) -> Result<Identity, IdentityError> {
    create_inbox(
        identifier,
        nonce,
        installation_keys,
        xmtp_id::associations::test_utils::MockSmartContractSignatureVerifier::new(true),
    )
    .await
}

/// Generate a new key package and store the associated keys in the database.
#[tracing::instrument(level = "trace", skip_all)]
pub(crate) fn new_key_package(
    identity: &Identity,
    provider: &impl MlsProviderExt,
    include_post_quantum: bool,
) -> Result<NewKeyPackageResult, IdentityError> {
    XmtpKeyPackage::builder()
        .inbox_id(identity.inbox_id())
        .credential(identity.credential())
        .installation_keys(identity.installation_keys.clone())
        .build(provider, include_post_quantum)
}

/// Store fresh key material before upload, then record its publication receipt.
/// Receipt order controls retirement. A read settles unknown publications first.
/// Receipt bookkeeping and the next rotation deadline commit together.
#[tracing::instrument(level = "trace", skip_all)]
pub(crate) async fn rotate_and_upload_key_package<ApiClient: XmtpApi, S: XmtpMlsStorageProvider>(
    identity: &Identity,
    api_client: &ApiClientWrapper<ApiClient>,
    mls_storage: &S,
    include_post_quantum: bool,
) -> Result<(), IdentityError> {
    tracing::info!("Start rotating keys and uploading the new key package");

    // implements: JOIN-074
    if let Err(error) = settle_unconfirmed_key_packages(identity, api_client, mls_storage).await {
        tracing::warn!(%error, "key package settlement deferred");
    }

    // Generate and store key package locally
    let (kp_bytes, history_id) =
        generate_and_store_key_package(identity, mls_storage, include_post_quantum)?;

    // Upload to network
    match api_client.upload_key_package(kp_bytes).await {
        Ok(meta) => {
            let published_cursor = meta.cursor.filter(|cursor| cursor.sequence_id > 0).ok_or(
                xmtp_api::ApiError::InvalidResponse("key package publish cursor"),
            )?;
            // Backend publication order can differ from local generation order.
            crate::state_tx::state_write(mls_storage, |tx| {
                let storage = tx.storage();
                storage.db().record_key_package_publication(
                    history_id,
                    xmtp_proto::types::Cursor(published_cursor.sequence_id),
                )?;
                storage
                    .db()
                    .reset_key_package_rotation_queue(KEY_PACKAGE_ROTATION_INTERVAL_NS)?;
                Ok::<_, StorageError>(Continue(()))
            })
            .map(TransactionOutcome::into_continued)?;
            Ok(())
        }
        // INVALID_ARGUMENT stored nothing, so no sender can address the package.
        Err(err) if definite_refusal(&err) => {
            discard_key_package(mls_storage, history_id)?;
            Err(IdentityError::ApiClient(err))
        }
        Err(err) => Err(IdentityError::ApiClient(err)),
    }
}

/// Settle receipt-less keys against the package the backend serves, before a new one exists.
async fn settle_unconfirmed_key_packages<ApiClient: XmtpApi, S: XmtpMlsStorageProvider>(
    identity: &Identity,
    api_client: &ApiClientWrapper<ApiClient>,
    mls_storage: &S,
) -> Result<(), IdentityError> {
    let mut history = mls_storage
        .db()
        .find_key_package_history_entries_before_id(i32::MAX)?
        .into_iter();
    if !history.any(|kp| kp.published_sequence_id.is_none() && kp.delete_at_ns.is_none()) {
        return Ok(());
    }
    let id = identity.installation_id();
    let Some((package, served)) = api_client.newest_key_package(&id).await? else {
        return Ok(());
    };
    let crypto = xmtp_db::XmtpOpenMlsProvider::<()>::new_crypto();
    let Some(Ok(verified)) = verify_served_key_package(&crypto, &id, &package) else {
        return Ok(());
    };
    let provider = XmtpOpenMlsProviderRef::new(mls_storage);
    let hash_ref = serialize_key_package_hash_ref(&verified.inner, &provider)?;
    Ok(mls_storage.db().settle_key_packages(hash_ref, served)?)
}

/// Store key material and its history row in one state transaction.
/// Return bytes for upload and the history ID for its later receipt.
#[tracing::instrument(level = "trace", skip_all)]
pub(crate) fn generate_and_store_key_package<S: XmtpMlsStorageProvider>(
    identity: &Identity,
    mls_storage: &S,
    include_post_quantum: bool,
) -> Result<(Vec<u8>, i32), IdentityError> {
    crate::state_tx::state_write_durable(mls_storage, |tx| {
        let storage = tx.storage();
        let provider = XmtpOpenMlsProviderRef::new(&storage);
        let NewKeyPackageResult {
            key_package: kp,
            pq_pub_key,
        } = new_key_package(identity, &provider, include_post_quantum)?;
        let hash_ref = serialize_key_package_hash_ref(&kp, &provider)?;
        let history_id = storage
            .db()
            .store_key_package_history_entry(hash_ref, pq_pub_key)?
            .id;
        let kp_bytes = kp.tls_serialize_detached()?;
        Ok::<_, IdentityError>(Continue((kp_bytes, history_id)))
    })
    .map(TransactionOutcome::into_continued)
}
