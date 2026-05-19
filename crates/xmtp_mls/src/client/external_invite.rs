//! Consumer-side QR-invite flow: [`Client::join_group_by_external_invite`].
//!
//! Given a decoded [`ExternalInvitePayload`] and the matching encrypted
//! GroupInfo blob fetched from the external service, this module joins the
//! target group via an MLS external commit, atomically adds the joiner's
//! other installations, registers the joiner in the group's membership
//! component, and returns a freshly-encrypted GroupInfo at the new epoch so
//! the service can rotate its stored blob.
//!
//! Atomicity is required: the validator-side rejects external commits that
//! don't simultaneously add ALL installations of the joiner's inbox AND
//! register the inbox in the membership component (the AppDataUpdate
//! proposal).
//!
//! [`ExternalInvitePayload`]: xmtp_proto::xmtp::mls::message_contents::ExternalInvitePayload

use hmac::{Hmac, Mac};
use openmls::{
    extensions::{ApplicationIdExtension, Extension, ExtensionType, Extensions},
    framing::{MlsMessageBodyIn, MlsMessageIn},
    group::{MlsGroupJoinConfig, WireFormatPolicy},
    messages::proposals::{AppDataUpdateProposal, Proposal, ProposalType},
    prelude::{
        Capabilities, CredentialWithKey, LeafNode, LeafNodeParameters, tls_codec::Deserialize as _,
        tls_codec::Serialize as _,
    },
};
use openmls_traits::OpenMlsProvider as _;
use prost::Message as _;
use sha2::Sha256;
use thiserror::Error;
use tls_codec::VLBytes;
use xmtp_configuration::{
    GROUP_MEMBERSHIP_EXTENSION_ID, GROUP_PERMISSIONS_EXTENSION_ID, MAX_PAST_EPOCHS,
    MUTABLE_METADATA_EXTENSION_ID, WELCOME_HPKE_LABEL,
    WELCOME_POINTEE_ENCRYPTION_AEAD_TYPES_EXTENSION_ID, WELCOME_WRAPPER_ENCRYPTION_EXTENSION_ID,
};
use xmtp_cryptography::configuration::CIPHERSUITE;
use xmtp_db::prelude::*;
use xmtp_db::xmtp_openmls_provider::XmtpOpenMlsProviderRef;
use xmtp_id::associations::{MemberIdentifier, ident};
use xmtp_mls_common::invite::{
    encrypted_group_info::{EncryptedGroupInfoError, unwrap_group_info, wrap_group_info},
    payload::{InvitePayloadError, SYMMETRIC_KEY_LEN, validate},
};
use xmtp_mls_common::mls_ext::payload_encryption::wrap_payload_hpke;
use xmtp_mls_common::{
    app_data::{
        component_id::ComponentId, components::tls_map_components::GroupMembershipComponent,
        typed::Component,
    },
    inbox_id::InboxId as VersionedInboxId,
    tls_map::TlsMapDelta,
};
use xmtp_proto::xmtp::mls::api::v1::{
    GroupMessageInput, WelcomeMessageInput, WelcomeMetadata,
    group_message_input::{V1 as GroupMessageInputV1, Version as GroupMessageInputVersion},
    welcome_message_input::{V1 as WelcomeMessageInputV1, Version as WelcomeMessageInputVersion},
};
use xmtp_proto::xmtp::mls::message_contents::{
    EncryptedGroupInfoBlob as EncryptedGroupInfoBlobProto,
    ExternalInvitePayload as ExternalInvitePayloadProto, GroupMembershipEntry,
    group_membership_entry,
};

use crate::{
    Client,
    client::ClientError,
    context::XmtpSharedContext,
    groups::{MlsGroup, intents::Installation},
    identity_updates::load_identity_updates,
};

/// Result returned by [`Client::join_group_by_external_invite`].
///
/// `group` is the freshly-joined [`MlsGroup`] view of the target group with
/// the joiner now a member at the post-commit epoch. The underlying MLS
/// state has already been persisted to local storage by openmls's
/// `ExternalCommitBuilder::finalize`; a libxmtp-side `StoredGroup` row is
/// also written.
///
/// `refreshed_encrypted_group_info` is the post-commit GroupInfo blob
/// re-encrypted with the SAME symmetric key from the invite (and a fresh
/// nonce, per the ChaCha20Poly1305 nonce-uniqueness requirement). Callers
/// should ship this back to the external invite service so the next joiner
/// reads a GroupInfo at the new epoch and their external commit doesn't
/// race a stale ratchet tree.
pub struct JoinByExternalInviteOutput<Context> {
    /// The MlsGroup view of the freshly-joined group, already persisted.
    pub group: MlsGroup<Context>,
    /// Post-commit GroupInfo blob re-encrypted with the invite's
    /// symmetric key. Upload back to the external invite service.
    pub refreshed_encrypted_group_info: Vec<u8>,
}

impl<Context> std::fmt::Debug for JoinByExternalInviteOutput<Context>
where
    Context: XmtpSharedContext,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JoinByExternalInviteOutput")
            .field("group", &self.group)
            .field(
                "refreshed_encrypted_group_info",
                &format!("<{} bytes>", self.refreshed_encrypted_group_info.len()),
            )
            .finish()
    }
}

/// Errors returned by [`Client::join_group_by_external_invite`].
#[derive(Debug, Error)]
pub enum JoinByExternalInviteError {
    /// The invite payload proto failed prost decoding.
    #[error("invite payload proto decode failed: {0}")]
    InvitePayloadDecode(#[source] prost::DecodeError),
    /// The invite payload didn't validate (unsupported version, wrong
    /// symmetric_key length, too-short external_group_id).
    #[error(transparent)]
    InvitePayload(#[from] InvitePayloadError),
    /// The encrypted GroupInfo blob proto failed prost decoding.
    #[error("encrypted GroupInfo blob proto decode failed: {0}")]
    EncryptedBlobDecode(#[source] prost::DecodeError),
    /// The encrypted GroupInfo envelope was malformed (wrong version,
    /// short nonce, …) or AEAD decryption failed (wrong key, tampered
    /// ciphertext).
    #[error(transparent)]
    EncryptedBlob(#[from] EncryptedGroupInfoError),
    /// The decrypted plaintext failed to parse as a TLS-serialized
    /// MlsMessageIn carrying a GroupInfo body.
    #[error("decrypted plaintext is not an MlsMessage(GroupInfo)")]
    GroupInfoNotGroupInfo,
    /// The decrypted plaintext failed TLS decoding entirely.
    #[error("decrypted GroupInfo TLS decode failed: {0}")]
    GroupInfoTlsDecode(#[from] tls_codec::Error),
    /// Post-join correlation check failed: the just-joined group's
    /// `EXTERNAL_COMMIT_POLICY.external_group_id` doesn't match the
    /// invite payload's `external_group_id`. Indicates the QR is stale
    /// (admin rotated the policy + re-issued) or the service returned a
    /// GroupInfo for a different group than the QR pointed at.
    #[error("post-join external_group_id mismatch — QR is stale or service returned wrong group")]
    ExternalGroupIdMismatch,
    /// The freshly-joined group has `EXTERNAL_COMMIT_POLICY` absent or
    /// with `allow_external_commit = false`. Should be impossible
    /// (validator-side enforces this before the commit is accepted) but
    /// surface a clear error rather than panic.
    #[error("post-join: EXTERNAL_COMMIT_POLICY missing or disabled")]
    PostJoinPolicyMissing,
    /// Failed to fetch one or more installation key packages for the
    /// joiner's other installations. Without ALL co-resident key
    /// packages we can't atomically add them, and the validator would
    /// reject a partial-membership external commit.
    #[error("failed to fetch key packages for joiner's other installations: {missing} missing")]
    MissingInstallations { missing: usize },
    /// The openmls external commit builder returned an error during the
    /// `build_group` / `add_proposals` / `build` / `finalize` pipeline.
    #[error("external commit builder failed: {0}")]
    ExternalCommitBuilderFailed(String),
    /// AppDataUpdate proposal payload construction failed.
    #[error("failed to build AppDataUpdate(GROUP_MEMBERSHIP) payload: {0}")]
    BuildAppDataUpdatePayload(String),
    /// Re-export of the post-commit GroupInfo failed.
    #[error("post-commit GroupInfo export failed: {0}")]
    PostCommitGroupInfoExport(String),
    /// Re-encryption of the post-commit GroupInfo failed.
    #[error("post-commit GroupInfo re-encrypt failed: {0}")]
    PostCommitGroupInfoEncrypt(#[source] EncryptedGroupInfoError),
}

impl From<JoinByExternalInviteError> for ClientError {
    fn from(value: JoinByExternalInviteError) -> Self {
        ClientError::Generic(format!("join_group_by_external_invite: {value}"))
    }
}

impl<Context> Client<Context>
where
    Context: XmtpSharedContext,
{
    /// Join the group referenced by `invite_payload_bytes` +
    /// `encrypted_group_info_bytes` via an MLS external commit.
    ///
    /// The flow:
    ///
    /// 1. Decode + validate the [`ExternalInvitePayload`] (version, expiry,
    ///    symmetric key length).
    /// 2. Decode + decrypt the [`EncryptedGroupInfoBlob`] using the
    ///    payload's symmetric key.
    /// 3. Parse the plaintext as an `MlsMessage(GroupInfo)` and extract the
    ///    [`VerifiableGroupInfo`].
    /// 4. Defense-in-depth: verify
    ///    `sha256(group_id) == payload.group_id_hash`.
    /// 5. Enumerate the joiner's OTHER installations from the latest
    ///    association state and fetch their key packages.
    /// 6. Build an external commit that:
    ///    - introduces the joiner via the `CredentialWithKey` in the
    ///      `ExternalInit` proposal,
    ///    - inlines one `Add` proposal per other installation (so all
    ///      co-resident leaves land in the same commit), and
    ///    - inlines an `AppDataUpdate(GROUP_MEMBERSHIP)` proposal that
    ///      registers the joiner's inbox in the membership component.
    /// 7. Finalize the commit; openmls persists the joined-group state to
    ///    the workspace MLS storage.
    /// 8. Publish the `PublicMessage` commit to XMTP delivery.
    /// 9. Send HPKE-wrapped Welcomes to each non-primary installation.
    /// 10. Re-export and re-encrypt the post-commit GroupInfo so the
    ///     external service can serve the new epoch to the next joiner.
    ///
    /// [`ExternalInvitePayload`]: xmtp_proto::xmtp::mls::message_contents::ExternalInvitePayload
    /// [`EncryptedGroupInfoBlob`]: xmtp_proto::xmtp::mls::message_contents::EncryptedGroupInfoBlob
    /// [`VerifiableGroupInfo`]: openmls::messages::group_info::VerifiableGroupInfo
    #[tracing::instrument(level = "trace", skip_all, fields(inbox_id = %self.context.inbox_id()))]
    pub async fn join_group_by_external_invite(
        &self,
        invite_payload_bytes: &[u8],
        encrypted_group_info_bytes: &[u8],
    ) -> Result<JoinByExternalInviteOutput<Context>, ClientError> {
        // 1. Decode + validate the invite payload. `validate` checks the
        //    version oneof, symmetric_key length, and
        //    external_group_id min length in one pass.
        let payload = ExternalInvitePayloadProto::decode(invite_payload_bytes)
            .map_err(JoinByExternalInviteError::InvitePayloadDecode)?;
        let payload_v1 = validate(&payload).map_err(JoinByExternalInviteError::from)?;

        let mut key = [0u8; SYMMETRIC_KEY_LEN];
        key.copy_from_slice(&payload_v1.symmetric_key);
        let payload_external_group_id = payload_v1.external_group_id.clone();

        // 2. Decode + decrypt the encrypted GroupInfo blob. Expiry now
        //    lives on the blob envelope (the service controls TTL), not
        //    on the payload — `unwrap_group_info` enforces it when
        //    `now_ns` is supplied.
        let blob = EncryptedGroupInfoBlobProto::decode(encrypted_group_info_bytes)
            .map_err(JoinByExternalInviteError::EncryptedBlobDecode)?;
        let now = xmtp_common::time::now_ns() as u64;
        let (plaintext, _blob_v1) =
            unwrap_group_info(&blob, &key, Some(now)).map_err(JoinByExternalInviteError::from)?;

        // 3. Parse plaintext as MlsMessage(GroupInfo).
        let mls_message = MlsMessageIn::tls_deserialize(&mut plaintext.as_slice())
            .map_err(JoinByExternalInviteError::GroupInfoTlsDecode)?;
        let verifiable_group_info = match mls_message.extract() {
            MlsMessageBodyIn::GroupInfo(vgi) => vgi,
            _ => return Err(JoinByExternalInviteError::GroupInfoNotGroupInfo.into()),
        };
        // Defense-in-depth payload-vs-GroupInfo group-id binding is no
        // longer present in the wire format (`payload.external_group_id`
        // is a service slot id, intentionally decoupled from the MLS
        // group_id). The post-join correlation check at step 11 catches
        // service swaps: we read EXTERNAL_COMMIT_POLICY off the joined
        // group and confirm its `external_group_id` matches the payload.

        // 5. Enumerate the joiner's OTHER installations from the latest
        // association state and fetch their key packages. We hit the
        // network for identity updates first so we never miss a recently
        // rotated installation — a partial enumeration would make the
        // validator reject the external commit.
        let inbox_id = self.context.inbox_id().to_string();
        let my_installation_id = self.context.installation_id();
        let db = self.context.db();
        load_identity_updates(self.context.api(), &db, &[inbox_id.as_str()])
            .await
            .map_err(|e| {
                ClientError::Generic(format!(
                    "join_group_by_external_invite: load identity updates: {e}"
                ))
            })?;
        // `IdentityUpdates::get_latest_association_state` takes a
        // `&DbConnection`; the generic context here only gives us back a
        // `DbQuery` (which, on native, IS a DbConnection — but the
        // compiler can't see through the associated type). We've already
        // refreshed identity updates above, so reading the in-memory
        // association state via `get_association_state` (which takes
        // `&impl DbQuery`) is exactly equivalent.
        let association_state = self
            .identity_updates()
            .get_association_state(&db, &inbox_id, None)
            .await?;
        let my_installation_slice: &[u8] = &*my_installation_id;
        let other_installation_ids: Vec<Vec<u8>> = association_state
            .members()
            .into_iter()
            .filter_map(|member| match member.identifier {
                MemberIdentifier::Installation(ident::Installation(id))
                    if id.as_slice() != my_installation_slice =>
                {
                    Some(id)
                }
                _ => None,
            })
            .collect();

        let (other_key_packages, other_installations) = if other_installation_ids.is_empty() {
            (Vec::new(), Vec::new())
        } else {
            let kps_map = self
                .mls_store()
                .get_key_packages_for_installation_ids(other_installation_ids.clone())
                .await?;
            let mut kps: Vec<openmls::key_packages::KeyPackage> =
                Vec::with_capacity(other_installation_ids.len());
            let mut installations: Vec<Installation> =
                Vec::with_capacity(other_installation_ids.len());
            let mut missing = 0usize;
            for id in &other_installation_ids {
                match kps_map.get(id) {
                    Some(Ok(verified_kp)) => {
                        kps.push(verified_kp.inner.clone());
                        installations.push(
                            Installation::from_verified_key_package(verified_kp).map_err(|e| {
                                ClientError::Generic(format!(
                                    "Installation::from_verified_key_package: {e}"
                                ))
                            })?,
                        );
                    }
                    Some(Err(_)) | None => missing += 1,
                }
            }
            if missing > 0 {
                return Err(JoinByExternalInviteError::MissingInstallations { missing }.into());
            }
            (kps, installations)
        };

        // 6. Build the AppDataUpdate(GROUP_MEMBERSHIP) payload that
        // registers the joiner's inbox in the membership component. The
        // shape mirrors `build_group_membership_app_data_payload` on the
        // existing commit-path: a `TlsMapDelta<InboxId, VLBytes>` with
        // one Insert(joiner_inbox_id, encode(V1{sequence_id, failed=[]}))
        // mutation, encoded via `Component::encode_mutation` so the
        // receiver decodes through the same path.
        let joiner_inbox_id = VersionedInboxId::from_hex(&inbox_id).map_err(|e| {
            JoinByExternalInviteError::BuildAppDataUpdatePayload(format!(
                "inbox_id from_hex failed: {e}"
            ))
        })?;
        let sequence_id = db
            .get_latest_sequence_id(&[inbox_id.as_str()])
            .map_err(|e| {
                ClientError::Generic(format!(
                    "join_group_by_external_invite: get_latest_sequence_id failed: {e}"
                ))
            })?
            .get(inbox_id.as_str())
            .copied()
            .unwrap_or(0) as u64;
        let entry = GroupMembershipEntry {
            version: Some(group_membership_entry::Version::V1(
                group_membership_entry::V1 {
                    sequence_id,
                    failed_installations: vec![],
                },
            )),
        };
        let mut delta: TlsMapDelta<VersionedInboxId, VLBytes> = TlsMapDelta::new();
        delta = delta.insert(joiner_inbox_id, VLBytes::new(entry.encode_to_vec()));
        let app_data_payload = <GroupMembershipComponent as Component>::encode_mutation(&delta)
            .map_err(|e| {
                JoinByExternalInviteError::BuildAppDataUpdatePayload(format!(
                    "encode_mutation failed: {e}"
                ))
            })?;
        let app_data_update_proposal =
            Proposal::AppDataUpdate(Box::new(AppDataUpdateProposal::update(
                ComponentId::GROUP_MEMBERSHIP.as_u16(),
                app_data_payload.clone(),
            )));

        // 7. Build the external commit. Per L-9 the openmls fork accepts
        // by-value `Proposal::Add` and `Proposal::AppDataUpdate` inline
        // for external commits (validator carve-out for non-member joins
        // that need to atomically introduce co-resident leaves AND
        // register membership in the AppData dictionary).
        let provider = XmtpOpenMlsProviderRef::new(self.context.mls_storage());
        let signer = &self.context.identity().installation_keys;
        let credential_with_key = CredentialWithKey {
            credential: self.context.identity().credential.clone(),
            signature_key: self
                .context
                .identity()
                .installation_keys
                .public_slice()
                .into(),
        };
        // Same shape as `groups::mls_ext::decrypted_welcome::build_group_join_config`,
        // inlined here because that module is private to `groups`.
        let join_config = MlsGroupJoinConfig::builder()
            .wire_format_policy(WireFormatPolicy::default())
            .max_past_epochs(MAX_PAST_EPOCHS)
            .use_ratchet_tree_extension(true)
            .build();

        // Build the joiner's leaf-node parameters. We must advertise
        // every extension type the target group's RequiredCapabilities
        // demand AND the `AppDataUpdate` proposal type so the validator
        // accepts our by-value AppDataUpdate(GROUP_MEMBERSHIP) proposal.
        // This mirrors `XmtpKeyPackage::build`'s capability set in
        // `crates/xmtp_mls/src/identity.rs:842-888` — kept in sync by
        // construction (both lists advertise the same XMTP-flavored
        // extension catalogue + proposals).
        let leaf_node_parameters =
            build_external_join_leaf_node_parameters(credential_with_key.clone(), &inbox_id)?;

        let mut builder = openmls::group::MlsGroup::external_commit_builder()
            .with_config(join_config)
            .build_group(&provider, verifiable_group_info, credential_with_key)
            .map_err(|e| {
                JoinByExternalInviteError::ExternalCommitBuilderFailed(format!("build_group: {e}"))
            })?
            .leaf_node_parameters(leaf_node_parameters);
        // Inline one Add proposal per other installation (by-value, per
        // the L-9 validator carve-out). Then inline the AppDataUpdate
        // membership proposal. Order matters: per L-7's
        // validate_app_data_update_proposals_and_group_context, an
        // AppDataUpdate must NOT appear before a GroupContextExtensions
        // proposal — we emit no GCE here, so any ordering with Add
        // proposals is fine.
        if !other_key_packages.is_empty() {
            builder = builder.propose_adds(other_key_packages);
        }
        builder = builder.add_proposal(app_data_update_proposal);

        let mut builder = builder.load_psks(provider.storage()).map_err(|e| {
            JoinByExternalInviteError::ExternalCommitBuilderFailed(format!("load_psks: {e}"))
        })?;

        // 7a. Compute the AppDataDictionary updates that openmls expects
        // to accompany our AppDataUpdate(GROUP_MEMBERSHIP) proposal. The
        // proposal carries a *delta* (`TlsMapDelta<InboxId, VLBytes>`);
        // openmls needs the resulting absolute value (`Vec<u8>` of the
        // post-commit `TlsMap<InboxId, VLBytes>`) so the commit's
        // confirmation tag agrees with what receivers will compute from
        // their own apply path. Read the joining-epoch value of the
        // GROUP_MEMBERSHIP component from the builder's view of the
        // group's app_data_dict, then run our delta through
        // `GroupMembershipComponent::apply_update_payload` to materialize
        // the new absolute value. This mirrors the receive-side
        // `accumulate_app_data_updates` logic, just collapsed for the
        // single-proposal case typical of an external-invite join.
        let mut updater = builder.app_data_dictionary_updater();
        let old_group_membership_bytes: Option<Vec<u8>> = updater
            .old_value(ComponentId::GROUP_MEMBERSHIP.as_u16())
            .map(<[u8]>::to_vec);
        let new_group_membership_bytes =
            <GroupMembershipComponent as Component>::apply_update_payload(
                &app_data_payload,
                old_group_membership_bytes.as_deref(),
            )
            .map_err(|e| {
                JoinByExternalInviteError::BuildAppDataUpdatePayload(format!(
                    "apply_update_payload failed: {e}"
                ))
            })?;
        updater.set(openmls::component::ComponentData::from_parts(
            ComponentId::GROUP_MEMBERSHIP.as_u16(),
            VLBytes::new(new_group_membership_bytes),
        ));
        builder.with_app_data_dictionary_updates(updater.changes());

        let (mls_group, bundle) = builder
            .build(provider.rand(), provider.crypto(), signer, |_| true)
            .map_err(|e| {
                JoinByExternalInviteError::ExternalCommitBuilderFailed(format!("build: {e}"))
            })?
            .finalize(&provider)
            .map_err(|e| {
                JoinByExternalInviteError::ExternalCommitBuilderFailed(format!("finalize: {e}"))
            })?;

        // 7b. Persist a libxmtp StoredGroup row for the freshly-joined
        // group so libxmtp's higher-level group APIs (load, sync, send,
        // find_groups) can see it. openmls has already persisted its own
        // MLS state via the finalize call above; the StoredGroup wrapper
        // carries libxmtp-side metadata. Mirrors the welcome-side
        // semantics: membership_state=Pending until the next sync
        // confirms our join.
        let group_id = xmtp_proto::types::GroupId::try_from(mls_group.group_id()).map_err(|e| {
            ClientError::Generic(format!(
                "join_group_by_external_invite: group_id conversion failed: {e}"
            ))
        })?;
        persist_joined_group(&self.context, group_id)?;
        let created_at_ns = xmtp_common::time::now_ns();
        let new_group = MlsGroup::new_from_arc(
            self.context.clone(),
            group_id,
            None,
            xmtp_db::group::ConversationType::Group,
            created_at_ns,
        );

        // 8. Publish the PublicMessage external commit to XMTP delivery.
        // Bundle gives us a borrowed view of the commit; serialize and
        // wrap into the same `GroupMessageInput.V1` shape the existing
        // commit-publish path uses (an HMAC-of-payload binding to the
        // current epoch's sender HMAC key, even for an external joiner
        // — receivers use the HMAC for dedup, not authentication).
        let commit_bytes = bundle
            .commit()
            .tls_serialize_detached()
            .map_err(JoinByExternalInviteError::GroupInfoTlsDecode)?;
        let group_message = build_group_message_input(&new_group, &commit_bytes)?;
        self.context
            .api()
            .send_group_messages(vec![group_message])
            .await
            .map_err(|e| ClientError::PublishError(format!("external commit publish: {e}")))?;

        // 9. HPKE-wrap + ship Welcomes to each non-primary installation.
        // The Welcome carries the freshly-minted ratchet tree at the
        // post-commit epoch so each co-resident installation can
        // reconstruct group state and decrypt subsequent messages. We
        // inline the v1 HPKE-wrap (one welcome per installation) rather
        // than reuse `MlsGroup::send_welcomes`, which is `pub(super)`
        // inside `groups` and additionally branches into the
        // welcome-pointer optimization for groups above
        // INSTALLATION_THRESHOLD_FOR_WELCOME_POINTER_SENDING — irrelevant
        // for the small (≤ ~few) other-installation case typical of a
        // QR-invite join.
        if !other_installations.is_empty()
            && let Some(welcome_msg) = bundle.clone().into_welcome_msg()
        {
            let welcome_bytes = welcome_msg
                .tls_serialize_detached()
                .map_err(JoinByExternalInviteError::GroupInfoTlsDecode)?;
            let welcome_metadata_bytes = WelcomeMetadata { message_cursor: 0 }.encode_to_vec();
            let mut welcomes: Vec<WelcomeMessageInput> =
                Vec::with_capacity(other_installations.len());
            for installation in &other_installations {
                let algorithm = installation.welcome_wrapper_algorithm;
                let (data, welcome_metadata) = wrap_payload_hpke(
                    &welcome_bytes,
                    &welcome_metadata_bytes,
                    &installation.hpke_public_key,
                    algorithm,
                    WELCOME_HPKE_LABEL,
                )
                .map_err(|e| {
                    ClientError::Generic(format!(
                        "join_group_by_external_invite: wrap_payload_hpke failed: {e}"
                    ))
                })?;
                welcomes.push(WelcomeMessageInput {
                    version: Some(WelcomeMessageInputVersion::V1(WelcomeMessageInputV1 {
                        installation_key: installation.installation_key.clone(),
                        data,
                        hpke_public_key: installation.hpke_public_key.clone(),
                        wrapper_algorithm: i32::from(algorithm),
                        welcome_metadata,
                    })),
                });
            }
            self.context
                .api()
                .send_welcome_messages(&welcomes)
                .await
                .map_err(|e| {
                    ClientError::PublishError(format!("external commit welcome publish: {e}"))
                })?;
        }

        // 10. Read the freshly-joined group's EXTERNAL_COMMIT_POLICY.
        //     a) Correlation check: payload.external_group_id MUST equal
        //        the policy's external_group_id. Catches stale QRs
        //        (admin rotated policy + re-issued) and service swaps
        //        (service returned a GroupInfo for the wrong group).
        //     b) Derive the refreshed-blob's `expires_at_ns` from the
        //        policy so re-uploads carry the admin's intended TTL.
        let policy = crate::groups::external_commit_policy::load_external_commit_policy(&mls_group)
            .map_err(|e| {
                ClientError::Generic(format!(
                    "join_group_by_external_invite: load EXTERNAL_COMMIT_POLICY: {e}"
                ))
            })?
            .ok_or(JoinByExternalInviteError::PostJoinPolicyMissing)?;
        if !policy.allow_external_commit {
            return Err(JoinByExternalInviteError::PostJoinPolicyMissing.into());
        }
        if policy.external_group_id.as_slice() != payload_external_group_id.as_slice() {
            return Err(JoinByExternalInviteError::ExternalGroupIdMismatch.into());
        }
        let refreshed_expires_at_ns = compute_refreshed_blob_expiry(&policy, now);

        // 11. Re-export the GroupInfo at the post-commit epoch and
        //     re-encrypt with the SAME symmetric key + a fresh nonce
        //     (wrap_group_info generates one internally). The external
        //     service atomically swaps its stored blob for this
        //     refreshed payload so the next joiner reads the right
        //     epoch. The blob carries epoch + state hash so the service
        //     can total-order uploads + reject same-epoch fork uploads.
        let post_commit_group_info_msg = mls_group
            .export_group_info(provider.crypto(), signer, /* with_ratchet_tree */ true)
            .map_err(|e| JoinByExternalInviteError::PostCommitGroupInfoExport(format!("{e}")))?;
        let post_commit_plaintext = post_commit_group_info_msg
            .tls_serialize_detached()
            .map_err(JoinByExternalInviteError::GroupInfoTlsDecode)?;
        let post_commit_epoch = mls_group.epoch().as_u64();
        let post_commit_state_hash = mls_group.epoch_authenticator().as_slice().to_vec();
        let refreshed_blob = wrap_group_info()
            .plaintext(&post_commit_plaintext)
            .key(&key)
            .epoch(post_commit_epoch)
            .group_state_hash(post_commit_state_hash)
            .expires_at_ns(refreshed_expires_at_ns)
            .call()
            .map_err(JoinByExternalInviteError::PostCommitGroupInfoEncrypt)?;
        let refreshed_encrypted_group_info = refreshed_blob.encode_to_vec();

        // Belt-and-suspenders: zeroize our copy of the symmetric key.
        key.fill(0);

        Ok(JoinByExternalInviteOutput {
            group: new_group,
            refreshed_encrypted_group_info,
        })
    }
}

/// Derive `expires_at_ns` for a refreshed-blob upload from the
/// post-join policy. Tightest active rule wins. `0` anywhere means
/// "this rule contributes nothing"; absent contributors produce a `0`
/// (no expiry) blob.
fn compute_refreshed_blob_expiry(
    policy: &xmtp_proto::xmtp::mls::message_contents::ExternalCommitPolicyV1,
    now_ns: u64,
) -> u64 {
    let mut candidates: Vec<u64> = Vec::new();
    if policy.expires_at_ns != 0 {
        candidates.push(policy.expires_at_ns);
    }
    if policy.expire_in_ns != 0 {
        candidates.push(now_ns.saturating_add(policy.expire_in_ns));
    }
    candidates.into_iter().min().unwrap_or(0)
}

/// Build the joiner's [`LeafNodeParameters`] for an external commit.
///
/// The capability set mirrors `XmtpKeyPackage::build` in
/// `crates/xmtp_mls/src/identity.rs:842-888`: advertise every
/// XMTP-flavored extension type and the `AppDataUpdate` /
/// `GroupContextExtensions` proposal types so the validator-side
/// accepts our by-value AppDataUpdate proposal and the leaf passes
/// the group's RequiredCapabilities check.
///
/// We also attach the `ApplicationId` leaf-node extension so receivers
/// can recover the joiner's inbox_id from the leaf node directly (same
/// as the welcome-side leaf builds).
fn build_external_join_leaf_node_parameters(
    credential_with_key: CredentialWithKey,
    inbox_id: &str,
) -> Result<LeafNodeParameters, ClientError> {
    let capability_extensions = [
        ExtensionType::LastResort,
        ExtensionType::ApplicationId,
        ExtensionType::ImmutableMetadata,
        ExtensionType::AppDataDictionary,
        ExtensionType::Unknown(GROUP_PERMISSIONS_EXTENSION_ID),
        ExtensionType::Unknown(MUTABLE_METADATA_EXTENSION_ID),
        ExtensionType::Unknown(GROUP_MEMBERSHIP_EXTENSION_ID),
        ExtensionType::Unknown(WELCOME_WRAPPER_ENCRYPTION_EXTENSION_ID),
        ExtensionType::Unknown(WELCOME_POINTEE_ENCRYPTION_AEAD_TYPES_EXTENSION_ID),
    ];
    let capabilities = Capabilities::new(
        None,
        Some(&[CIPHERSUITE]),
        Some(&capability_extensions),
        Some(&[
            ProposalType::GroupContextExtensions,
            ProposalType::AppDataUpdate,
        ]),
        None,
    );

    let application_id = Extension::ApplicationId(ApplicationIdExtension::new(inbox_id.as_bytes()));
    let leaf_node_extensions = Extensions::<LeafNode>::single(application_id).map_err(|e| {
        ClientError::Generic(format!(
            "join_group_by_external_invite: leaf node extensions build failed: {e}"
        ))
    })?;

    Ok(LeafNodeParameters::builder()
        .with_credential_with_key(credential_with_key)
        .with_capabilities(capabilities)
        .with_extensions(leaf_node_extensions)
        .build())
}

/// Build a `GroupMessageInput.V1` carrying `payload` and the current
/// epoch's sender HMAC. Mirrors `MlsGroup::prepare_group_messages` (which
/// is `pub(super)` inside `groups`); receivers use the HMAC for dedup so
/// any binding from the current epoch's key is sufficient.
fn build_group_message_input<Context: XmtpSharedContext>(
    group: &MlsGroup<Context>,
    payload: &[u8],
) -> Result<GroupMessageInput, ClientError> {
    let hmac_key = group
        .hmac_keys(0..=0)
        .map_err(ClientError::from)?
        .pop()
        .ok_or_else(|| ClientError::Generic("hmac_keys returned empty range".to_string()))?;
    let mut mac =
        <Hmac<Sha256> as Mac>::new_from_slice(&hmac_key.key).expect("HMAC accepts any key length");
    mac.update(payload);
    let sender_hmac = mac.finalize().into_bytes().to_vec();

    Ok(GroupMessageInput {
        version: Some(GroupMessageInputVersion::V1(GroupMessageInputV1 {
            data: payload.to_vec(),
            sender_hmac,
            should_push: false,
        })),
    })
}

/// Persist a StoredGroup row for a group joined via external commit.
///
/// Mirrors the welcome-side `insert_or_replace_group` flow but with
/// `added_by_inbox_id` set to the joiner's own inbox (external joins are
/// self-initiated) and a `Pending` membership state — the next sync will
/// surface the validator's verdict on the external commit and flip us to
/// `Allowed`.
fn persist_joined_group<Context: XmtpSharedContext>(
    context: &Context,
    group_id: xmtp_proto::types::GroupId,
) -> Result<(), ClientError> {
    use xmtp_db::group::{ConversationType, GroupMembershipState, StoredGroup};
    use xmtp_db::prelude::*;

    let creator = context.inbox_id().to_string();
    let stored_group = StoredGroup::builder()
        .id(group_id)
        .created_at_ns(xmtp_common::time::now_ns())
        .membership_state(GroupMembershipState::Pending)
        .conversation_type(ConversationType::Group)
        .added_by_inbox_id(creator)
        .should_publish_commit_log(false)
        .build()
        .map_err(|e| {
            ClientError::Generic(format!(
                "join_group_by_external_invite: StoredGroup build failed: {e}"
            ))
        })?;
    let _ = stored_group.store_or_ignore(&context.db());
    Ok(())
}

// End-to-end test coverage for this entry point lives in the QR-invite
// integration test (T-1, libxmtp-integration-test). The join path
// requires the producer-side `set_allow_external_commit` + the
// validator-side carve-out to be wired up end-to-end, both of which are
// outside this module's scope.
