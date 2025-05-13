use super::db_connection::DbConnection;
use super::ConnectionExt;
use crate::{impl_store, schema::icebox};
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

impl_store!(Icebox, icebox);

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
                SELECT *
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

#[cfg(test)]
mod tests {
    use crate::{with_connection, Store};

    use super::*;

    fn give_ice() -> Vec<Icebox> {
        vec![
            Icebox {
                sequence_id: 41,
                originator_id: 1,
                depending_sequence_id: Some(40),
                depending_originator_id: Some(1),
                envelope_payload: vec![1, 2, 3],
            },
            Icebox {
                sequence_id: 40,
                originator_id: 1,
                depending_sequence_id: Some(39),
                depending_originator_id: Some(2),
                envelope_payload: vec![1, 2, 3],
            },
            Icebox {
                sequence_id: 39,
                depending_sequence_id: None,
                originator_id: 2,
                depending_originator_id: None,
                envelope_payload: vec![1, 2, 3],
            },
        ]
    }

    #[xmtp_common::test(unwrap_try = "true")]
    async fn icebox_dependency_chain() {
        with_connection(|conn| {
            let ice = give_ice();
            ice.iter().for_each(|i| i.store(conn)?);

            let dep_chain = conn.get_dependency_chain(41, 1)?;
            assert_eq!(dep_chain, ice);
        })
        .await
    }

    #[xmtp_common::test(unwrap_try = "true")]
    async fn test_icebox_broken_dep_chain() {
        with_connection(|conn| {
            let mut ice = give_ice();
            ice[2].originator_id = 1;
            ice.iter().for_each(|i| i.store(conn)?);

            let dep_chain = conn.get_dependency_chain(41, 1)?;
            // The last iced message should not be there due to the wrong originator_id.
            ice.pop();
            assert_eq!(dep_chain, ice);
        })
        .await
    }
}
