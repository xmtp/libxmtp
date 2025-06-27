use std::collections::HashSet;
use std::sync::Arc;

use openmls::group::StagedWelcome;
use xmtp_api::XmtpApi;
use xmtp_db::XmtpDb;

use crate::context::{XmtpContextProvider, XmtpMlsLocalContext};
use crate::groups::extract_group_membership;
use crate::groups::MlsGroup;
use crate::identity_updates::load_identity_updates;
use crate::identity_updates::IdentityUpdates;
use crate::intents::ProcessIntentError;
use crate::GroupError;

/**
 * Ensures that the membership in the MLS tree matches the inboxes specified in the `GroupMembership` extension.
 */
pub async fn validate_initial_group_membership<ApiClient, Db>(
    context: &Arc<XmtpMlsLocalContext<ApiClient, Db>>,
    staged_welcome: &StagedWelcome,
) -> Result<(), GroupError>
where
    ApiClient: XmtpApi,
    Db: XmtpDb,
{
    let provider = context.mls_provider();
    let conn = provider.db();
    tracing::info!("Validating initial group membership");
    let extensions = staged_welcome.public_group().group_context().extensions();
    let membership = extract_group_membership(extensions)?;
    let needs_update = conn.filter_inbox_ids_needing_updates(membership.to_filters().as_slice())?;
    if !needs_update.is_empty() {
        let ids = needs_update.iter().map(AsRef::as_ref).collect::<Vec<_>>();
        load_identity_updates(context.api(), conn, ids.as_slice()).await?;
    }

    let mut expected_installation_ids = HashSet::<Vec<u8>>::new();

    let identity_updates = IdentityUpdates::new(context.clone());
    let futures: Vec<_> = membership
        .members
        .iter()
        .map(|(inbox_id, sequence_id)| {
            identity_updates.get_association_state(conn, inbox_id, Some(*sequence_id as i64))
        })
        .collect();

    let results = futures::future::try_join_all(futures).await?;

    for association_state in results {
        expected_installation_ids.extend(association_state.installation_ids());
    }

    let actual_installation_ids: HashSet<Vec<u8>> = staged_welcome
        .public_group()
        .members()
        .map(|member| member.signature_key)
        .collect();

    // exclude failed installations
    expected_installation_ids.retain(|id| !membership.failed_installations.contains(id));

    if expected_installation_ids != actual_installation_ids {
        return Err(GroupError::InvalidGroupMembership);
    }

    tracing::info!("Group membership validated");

    Ok(())
}

pub async fn validate_no_existing_group<ApiClient, Db>(
    context: &Arc<XmtpMlsLocalContext<ApiClient, Db>>,
    staged_welcome: &StagedWelcome,
    welcome_id: u64,
) -> Result<(), GroupError>
where
    ApiClient: XmtpApi,
    Db: XmtpDb,
{
    let provider = context.mls_provider();
    let conn = provider.db();
    let group_id = staged_welcome.public_group().group_id();
    if conn.find_group(group_id.as_slice())?.is_some() {
        // Fetch the original MLS group, rather than the one from the welcome
        let result = MlsGroup::new_cached(context.clone(), group_id.as_slice());
        if result.is_err() {
            tracing::error!(
                "Error fetching group while validating welcome: {:?}",
                result.err()
            );
        } else {
            let (group, _) = result.unwrap();
            // Check the group epoch as well, because we may not have synced the latest is_active state
            // TODO(rich): Design a better way to detect if incoming welcomes are valid
            if group.is_active()?
                && staged_welcome
                    .public_group()
                    .group_context()
                    .epoch()
                    .as_u64()
                    <= group.epoch().await?
            {
                tracing::error!(
                    "Skipping welcome {} because we are already in group {}",
                    welcome_id,
                    hex::encode(group_id.as_slice())
                );
                return Err(ProcessIntentError::WelcomeAlreadyProcessed(welcome_id).into());
            }
        }
    }
    Ok(())
}
