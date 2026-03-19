use crate::{
    context::XmtpSharedContext,
    groups::{
        GroupError, MlsGroup, commit_log::CommitLogError, commit_log_key::CommitLogKeyCrypto,
    },
    identity_updates::load_identity_updates,
};
use hkdf::Hkdf;
use openmls_traits::OpenMlsProvider;
use prost::Message;
use sha2::Sha256;
use xmtp_configuration::HMAC_SALT;
use xmtp_db::prelude::QueryAssociationStateCache;
use xmtp_id::associations::AssociationState;
use xmtp_proto::{
    ConversionError,
    xmtp::mls::message_contents::{CommitLogEntry, PlaintextCommitLogEntry},
};

pub struct VerifiedCommitLogEntry {
    pub entry: CommitLogEntry,
    pub log: PlaintextCommitLogEntry,
    pub installation_id: Vec<u8>,
}

impl VerifiedCommitLogEntry {
    /// Recovers the super-admin installation_id and verifies the signature in the entry.
    /// Returns None if it could not verify.
    pub async fn new<Context>(
        context: Context,
        group_id: &[u8],
        entry: CommitLogEntry,
    ) -> Result<Option<Self>, CommitLogError>
    where
        Context: XmtpSharedContext,
    {
        let (group, _stored_group) = MlsGroup::new_cached(context.clone(), group_id)?;
        let group_salt = group.mutable_metadata()?.salt().unwrap();
        let log = PlaintextCommitLogEntry::decode(&*entry.serialized_commit_log_entry)?;

        // Try once without fetching from the network.
        // Try a second time after fetching.
        for i in 0..2 {
            let super_admin_inbox_ids = group
                .super_admin_list()?
                .into_iter()
                .map(|id| (id, 0))
                .collect::<Vec<_>>();
            let association_states: Result<Vec<AssociationState>, ConversionError> = context
                .db()
                .batch_read_from_cache(super_admin_inbox_ids)?
                .into_iter()
                .map(|a| a.try_into())
                .collect();
            let association_states = association_states.map_err(GroupError::from)?;
            let installation_ids: Vec<Vec<u8>> = association_states
                .iter()
                .map(|a| a.installation_ids())
                .flatten()
                .collect();

            let installation_id_and_hmac = installation_ids
                .into_iter()
                .map(|id| {
                    let mut okm = [0; 32];
                    let hkdf = Hkdf::<Sha256>::new(Some(HMAC_SALT), group_salt.as_slice());
                    hkdf.expand(&id, &mut okm);
                    (id, okm)
                })
                .collect::<Vec<_>>();

            for (installation_id, hmac) in installation_id_and_hmac {
                if log.installation_hmac != hmac {
                    continue;
                }

                // We've found a matching hmac. Now verify the signature.
                context
                    .mls_provider()
                    .crypto()
                    .verify_commit_log_signature(&installation_id, &entry, group_salt.as_slice())?;

                return Ok(Some(Self {
                    entry,
                    log,
                    installation_id,
                }));
            }

            if i == 1 {
                // We've already loaded identity updates.
                // Break if we haven't matched by now.
                break;
            }

            load_identity_updates(
                context.api(),
                &context.db(),
                &association_states
                    .iter()
                    .map(|a| a.inbox_id())
                    .collect::<Vec<_>>(),
            )
            .await
            .map_err(GroupError::from)?;
        }

        Ok(None)
    }
}
