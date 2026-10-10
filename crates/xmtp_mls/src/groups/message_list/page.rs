use super::*;
use xmtp_db::{
    TransactionalKeyStore, XmtpMlsStorageProvider,
    delivery::{AppVisibleMessageRow, QueryDelivery},
    diesel::{Connection, SqliteConnection},
    group_message::RecoveryQueryArgs,
};

pub struct EnrichedMessagePage<P> {
    pub messages: Vec<EnrichedStoredMessage>,
    pub first_position: Option<P>,
    pub last_position: Option<P>,
    pub has_more: bool,
}

pub type EnrichedHistoryPage = EnrichedMessagePage<xmtp_db::delivery::HistoryPosition>;
pub type EnrichedRecoveryPage = EnrichedMessagePage<xmtp_db::group_message::RecoveryPosition>;

#[cfg(test)]
thread_local! {
    static BEFORE_ENRICHMENT: std::cell::RefCell<Option<Box<dyn FnOnce()>>> = const { std::cell::RefCell::new(None) };
}

fn with_page_snapshot<T>(
    conn: &impl DbQuery,
    read: impl FnOnce(&mut SqliteConnection) -> Result<T, EnrichMessageError>,
) -> Result<T, EnrichMessageError> {
    // A deferred transaction allows a WAL writer while this page keeps its snapshot.
    conn.raw_query(|sqlite| Ok(sqlite.transaction(read)))?
}

fn visible_recovery_query(mut query: RecoveryQueryArgs) -> RecoveryQueryArgs {
    query.messages = filter_out_hidden_message_types_from_query(&query.messages);
    query
}

fn enrich_page_rows(
    conn: &impl DbQuery,
    rows: Vec<AppVisibleMessageRow>,
) -> Result<Vec<EnrichedStoredMessage>, EnrichMessageError> {
    #[cfg(test)]
    BEFORE_ENRICHMENT.with_borrow_mut(|hook| {
        if let Some(hook) = hook.take() {
            hook();
        }
    });
    let positions: std::collections::HashMap<_, _> = rows
        .iter()
        .enumerate()
        .map(|(index, row)| (row.stored.id.clone(), (index, row.cursor)))
        .collect();
    let mut sources: std::collections::HashMap<_, Vec<_>> = std::collections::HashMap::new();
    for row in rows {
        sources
            .entry(row.stored.group_id)
            .or_default()
            .push(row.stored);
    }
    let mut messages = Vec::with_capacity(positions.len());
    for (group_id, rows) in sources {
        messages.extend(enrich_messages_with_stored(conn, &group_id, rows)?);
    }
    // Relations use the physical source. Display order uses the selected raw keys.
    messages.sort_by_key(|message| positions[&message.stored.id].0);
    for message in &mut messages {
        message.delivery_cursor = positions[&message.stored.id].1;
    }
    Ok(messages)
}

fn history_page_with_stored(
    conn: &impl DbQuery,
    group_id: &xmtp_proto::types::GroupId,
    query: &MsgQueryArgs,
) -> Result<EnrichedHistoryPage, EnrichMessageError> {
    with_page_snapshot(conn, |sqlite| {
        let store = sqlite.key_store();
        let conn = store.db();
        let page =
            conn.history_page_rows(group_id, &filter_out_hidden_message_types_from_query(query))?;
        Ok(EnrichedHistoryPage {
            messages: enrich_page_rows(&conn, page.rows)?,
            first_position: page.first_position,
            last_position: page.last_position,
            has_more: page.has_more,
        })
    })
}

fn recovery_page_with_stored(
    conn: &impl DbQuery,
    group_id: &xmtp_proto::types::GroupId,
    query: RecoveryQueryArgs,
) -> Result<EnrichedRecoveryPage, EnrichMessageError> {
    let query = visible_recovery_query(query);
    with_page_snapshot(conn, |sqlite| {
        let store = sqlite.key_store();
        let conn = store.db();
        let page = conn.recovery_page_rows(group_id, &query)?;
        Ok(EnrichedRecoveryPage {
            messages: enrich_page_rows(&conn, page.rows)?,
            first_position: page.first_position,
            last_position: page.last_position,
            has_more: page.has_more,
        })
    })
}

impl<Context: XmtpSharedContext> MlsGroup<Context> {
    /// Read base rows, raw coverage and relations in one database snapshot.
    pub fn find_history_page_with_stored(
        &self,
        query: &MsgQueryArgs,
    ) -> Result<EnrichedHistoryPage, EnrichMessageError> {
        history_page_with_stored(&self.context.db(), &self.group_id, query)
    }

    /// Read Failed and Unpublished rows and relations in one database snapshot.
    // implements: DMS-009
    pub fn find_recovery_page_with_stored(
        &self,
        query: RecoveryQueryArgs,
    ) -> Result<EnrichedRecoveryPage, EnrichMessageError> {
        recovery_page_with_stored(&self.context.db(), &self.group_id, query)
    }
}

#[cfg(test)]
mod tests;
