#[cfg(feature = "conformance")]
#[xmtp_macro::sdk_export]
impl Conversations {
    /// Read durable default progress for a conformance assertion.
    pub async fn sdk_conformance_delivery_position(
        &self,
        id: ConversationId,
    ) -> Result<String, XmtpError> {
        use xmtp_db::refresh_state::{EntityKind, QueryRefreshState};
        let context = self.client.context.clone();
        let group_id = GroupId::try_from(id)?;
        on_sdk_worker(self.client.context.clone(), async move {
            context
                .db()
                .get_refresh_state(&group_id, EntityKind::Delivery)
                .map(|state| {
                    state
                        .map_or(0, |state| state.sequence_id as u64)
                        .to_string()
                })
                .map_err(XmtpError::from_core)
        })
        .await
    }

    /// Set the fixed large cursor fixture before its first message is stored.
    pub async fn sdk_conformance_seed_delivery_cursor(&self) -> Result<(), XmtpError> {
        use xmtp_db::{
            ConnectionExt, delivery::QueryDelivery, diesel::prelude::*, refresh_state::EntityKind,
            schema::refresh_state::dsl,
        };
        let context = self.client.context.clone();
        on_sdk_worker(self.client.context.clone(), async move {
            let db = context.db();
            if db
                .current_delivery_cursor()
                .map_err(XmtpError::from_core)?
                .delivery_sequence
                != 0
            {
                return Err(XmtpError::unknown(
                    "cursor fixture requires an empty database",
                ));
            }
            db.raw_query(|conn| {
                xmtp_db::diesel::update(
                    dsl::refresh_state.filter(dsl::entity_kind.eq(EntityKind::DeliveryAllocator)),
                )
                .set(dsl::sequence_id.eq(1_i64 << 53))
                .execute(conn)
            })
            .map_err(XmtpError::from_core)?;
            Ok(())
        })
        .await
    }
}
