use super::*;
use xmtp_db::delivery::{AppVisibleMessageRow, QueryDelivery};

pub struct EnrichedMessagePage<P> {
    pub messages: Vec<EnrichedStoredMessage>,
    pub first_position: Option<P>,
    pub last_position: Option<P>,
    pub has_more: bool,
}

pub type EnrichedHistoryPage = EnrichedMessagePage<xmtp_db::delivery::HistoryPosition>;
pub type EnrichedRecoveryPage = EnrichedMessagePage<xmtp_db::group_message::RecoveryPosition>;

fn enrich_page_rows(
    conn: &impl DbQuery,
    rows: Vec<AppVisibleMessageRow>,
) -> Result<Vec<EnrichedStoredMessage>, EnrichMessageError> {
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

impl<Context: XmtpSharedContext> MlsGroup<Context> {
    /// Read base rows and raw coverage in one database snapshot.
    pub fn find_history_page_with_stored(
        &self,
        query: &MsgQueryArgs,
    ) -> Result<EnrichedHistoryPage, EnrichMessageError> {
        let conn = self.context.db();
        let page = conn.history_page_rows(
            &self.group_id,
            &filter_out_hidden_message_types_from_query(query),
        )?;
        Ok(EnrichedHistoryPage {
            messages: enrich_page_rows(&conn, page.rows)?,
            first_position: page.first_position,
            last_position: page.last_position,
            has_more: page.has_more,
        })
    }

    /// Read Failed and Unpublished rows with raw ID continuation.
    // implements: DMS-009
    pub fn find_recovery_page_with_stored(
        &self,
        query: &xmtp_db::group_message::RecoveryQueryArgs,
    ) -> Result<EnrichedRecoveryPage, EnrichMessageError> {
        let conn = self.context.db();
        let query = xmtp_db::group_message::RecoveryQueryArgs {
            messages: filter_out_hidden_message_types_from_query(&query.messages),
            ..query.clone()
        };
        let page = conn.recovery_page_rows(&self.group_id, &query)?;
        Ok(EnrichedRecoveryPage {
            messages: enrich_page_rows(&conn, page.rows)?,
            first_position: page.first_position,
            last_position: page.last_position,
            has_more: page.has_more,
        })
    }
}
