//! Loading identity updates from the network into the local store.

use super::*;

/// Load identity updates. A load that includes this client's own inbox
/// reports the changes to its own installations of every update it stored,
/// even when it fails after storing some pages.
pub async fn load_identity_updates_for_client<Context: IdentityContext>(
    context: &Context,
    conn: &impl DbQuery,
    inbox_ids: &[&str],
) -> Result<(), IdentityUpdateError> {
    let load = Box::pin(store_identity_update_pages(
        context.api(),
        conn,
        inbox_ids,
        context.stream_policy().fetched_limits(),
    ));
    if inbox_ids.contains(&context.inbox_id()) {
        IdentityUpdates::new(context)
            .with_own_installation_events(conn, load)
            .await
    } else {
        load.await
    }
}

/// Store every identity update of `inbox_ids` newer than the latest stored
/// one of each inbox.
#[tracing::instrument(level = "trace", skip_all)]
pub async fn load_identity_updates<ApiClient: XmtpApi>(
    api_client: &ApiClientWrapper<ApiClient>,
    conn: &impl xmtp_db::DbQuery,
    inbox_ids: &[&str],
) -> Result<(), IdentityUpdateError> {
    store_identity_update_pages(
        api_client,
        conn,
        inbox_ids,
        IncomingBatchLimits::STREAM_BATCH,
    )
    .await
}

/// Read the updates of `inbox_ids` after the latest stored one of each inbox,
/// one bounded page at a time, and store each page before the next request.
/// A failure leaves each inbox with a stored prefix of its log, and the next
/// load resumes after it.
async fn store_identity_update_pages<ApiClient: XmtpApi>(
    api_client: &ApiClientWrapper<ApiClient>,
    conn: &impl DbQuery,
    inbox_ids: &[&str],
    limits: IncomingBatchLimits,
) -> Result<(), IdentityUpdateError> {
    if inbox_ids.is_empty() {
        return Ok(());
    }
    tracing::debug!("Fetching identity updates for: {:?}", inbox_ids);

    let existing_sequence_ids = conn.get_latest_sequence_id(inbox_ids)?;
    let filters: Vec<GetIdentityUpdatesV2Filter> = inbox_ids
        .iter()
        .map(|inbox_id| GetIdentityUpdatesV2Filter {
            sequence_id: existing_sequence_ids.get(*inbox_id).map(|i| *i as u64),
            inbox_id: inbox_id.to_string(),
        })
        .collect();
    api_client
        .fold_identity_updates(filters, limits, |page| {
            let page = page
                .into_iter()
                .map(|entry| {
                    let sequence_id = entry
                        .meta
                        .cursor
                        .ok_or(xmtp_api::ApiError::InvalidResponse("identity cursor"))?
                        .sequence_id;
                    let inbox_id = entry.update.inbox_id.clone();
                    let update: UnverifiedIdentityUpdate = entry.update.try_into()?;
                    Ok(StoredIdentityUpdate {
                        inbox_id,
                        sequence_id: sequence_id as i64,
                        server_timestamp_ns: entry.meta.server_ns as i64,
                        payload: update.into(),
                    })
                })
                .collect::<Result<Vec<_>, IdentityUpdateError>>()?;
            conn.insert_or_ignore_identity_updates(&page)?;
            Ok(())
        })
        .await
}
