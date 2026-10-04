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

    /// Bind the database to another deployment at another URL, so the next
    /// offline build must re-check the backend before its first request.
    pub async fn sdk_conformance_bind_other_deployment(&self) -> Result<(), XmtpError> {
        use xmtp_db::prelude::QueryServerConfiguration;
        let context = self.client.context.clone();
        on_sdk_worker(self.client.context.clone(), async move {
            let db = context.db();
            let stored = db
                .server_configuration()
                .map_err(XmtpError::from_core)?
                .ok_or_else(|| XmtpError::unknown("no stored server configuration"))?;
            db.store_server_configuration(
                "org.example.other-deployment",
                "http://moved.example",
                &stored.response,
                stored.fetched_at_ns,
            )
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

#[cfg(all(feature = "conformance", not(target_arch = "wasm32")))]
#[xmtp_macro::sdk_export]
impl Conversations {
    /// Install one SQLite ACK failure in this conformance client's store.
    pub async fn sdk_conformance_install_ack_failure(
        &self,
        at_commit: bool,
    ) -> Result<(), XmtpError> {
        use xmtp_db::{ConnectionExt, diesel::connection::SimpleConnection};
        let context = self.client.context.clone();
        on_sdk_worker(self.client.context.clone(), async move {
            context
                .db()
                .raw_query(|conn| {
                    if at_commit {
                        conn.batch_execute(
                            "CREATE TABLE sdk_conformance_ack_parent (id INTEGER PRIMARY KEY); \
                             CREATE TABLE sdk_conformance_ack_child (id INTEGER PRIMARY KEY, parent INTEGER \
                             REFERENCES sdk_conformance_ack_parent(id) DEFERRABLE INITIALLY DEFERRED); \
                             CREATE TRIGGER sdk_conformance_fail_ack AFTER INSERT ON refresh_state \
                             WHEN NEW.entity_kind = 10 BEGIN INSERT INTO sdk_conformance_ack_child VALUES (1, 1); END",
                        )
                    } else {
                        conn.batch_execute(
                            "CREATE TRIGGER sdk_conformance_fail_ack BEFORE INSERT ON refresh_state \
                             WHEN NEW.entity_kind = 10 BEGIN SELECT RAISE(FAIL, 'forced ACK write error'); END",
                        )
                    }
                })
                .map_err(XmtpError::from_core)
        })
        .await
    }

    /// Remove the ACK fault before a new reader checks retained delivery.
    pub async fn sdk_conformance_clear_ack_failure(&self) -> Result<(), XmtpError> {
        use xmtp_db::{ConnectionExt, diesel::connection::SimpleConnection};
        let context = self.client.context.clone();
        on_sdk_worker(self.client.context.clone(), async move {
            context
                .db()
                .raw_query(|conn| {
                    conn.batch_execute(
                        "DROP TRIGGER IF EXISTS sdk_conformance_fail_ack; \
                         DROP TABLE IF EXISTS sdk_conformance_ack_child; \
                         DROP TABLE IF EXISTS sdk_conformance_ack_parent",
                    )
                })
                .map_err(XmtpError::from_core)
        })
        .await
    }
}
