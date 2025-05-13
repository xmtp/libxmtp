use super::db_connection::DbConnection;
use super::ConnectionExt;
use crate::schema::icebox;
use diesel::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(
    Debug,
    Clone,
    Serialize,
    Deserialize,
    Insertable,
    Identifiable,
    Queryable,
    Eq,
    PartialEq,
    QueryableByName,
)]
#[diesel(table_name = icebox)]
#[diesel(primary_key(sequence_id, originator_id))]
pub struct Icebox {
    pub sequence_id: i64,
    pub originator_id: i64,
    pub depending_sequence_id: Option<i64>,
    pub depending_originator_id: Option<i64>,
    pub envelope_payload: Vec<u8>,
}

impl<C: ConnectionExt> DbConnection<C> {
    pub fn get_dependency_chain(
        &self,
        sequence_id: i64,
        originator_id: i64,
    ) -> Result<Vec<Icebox>, crate::ConnectionError> {
        let query = diesel::sql_query(
            r#"
            WITH RECURSIVE dependency_chain AS (
                -- Base case: Start with the specified primary key
                SELECT sequence_id, originator_id, depending_sequence_id, depending_originator_id
                FROM icebox
                WHERE sequence_id = $1 AND originator_id = $2
                
                UNION ALL
                
                -- Recursive case: Join with dependencies
                SELECT t.*
                FROM icebox t
                JOIN dependency_chain dc ON t.sequence_id = dc.depending_sequence_id 
                                        AND t.originator_id = dc.depending_originator_id
            )
            SELECT * FROM dependency_chain
            ORDER BY sequence_id DESC;
        "#,
        )
        .bind::<diesel::sql_types::BigInt, _>(sequence_id)
        .bind::<diesel::sql_types::BigInt, _>(originator_id);

        self.raw_query_read(|conn| query.load(conn))
    }
}
