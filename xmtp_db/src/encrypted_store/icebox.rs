use super::db_connection::DbConnection;
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

impl Icebox {
    pub fn backward_dep_chain(
        conn: &DbConnection,
        sequence_id: i64,
        originator_id: i64,
    ) -> Result<Vec<Self>, crate::ConnectionError> {
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

        conn.raw_query_read(|conn| query.load(conn))
    }

    pub fn forward_dep_chain(
        conn: &DbConnection,
        sequence_id: i64,
        originator_id: i64,
    ) -> Result<Vec<Self>, crate::ConnectionError> {
        let query = diesel::sql_query(
            r#"
            WITH RECURSIVE dependency_chain AS (
                -- Base case: Start with the specified primary key
                SELECT *
                FROM icebox
                WHERE sequence_id = $1 AND originator_id = $2
                
                UNION ALL
                
                -- Recursive case: Join with dependents (reversed direction)
                SELECT t.*
                FROM icebox t
                JOIN dependency_chain dc ON t.depending_sequence_id = dc.sequence_id 
                                        AND t.depending_originator_id = dc.originator_id
            )
            SELECT * FROM dependency_chain
            ORDER BY sequence_id ASC;
        "#,
        )
        .bind::<diesel::sql_types::BigInt, _>(sequence_id)
        .bind::<diesel::sql_types::BigInt, _>(originator_id);

        conn.raw_query_read(|conn| query.load(conn))
    }
}

#[cfg(test)]
mod tests {
    use crate::{with_connection, Store};

    use super::*;

    fn iced() -> Vec<Icebox> {
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
                originator_id: 2,
                depending_sequence_id: None,
                depending_originator_id: None,
                envelope_payload: vec![1, 2, 3],
            },
        ]
    }

    #[xmtp_common::test(unwrap_try = "true")]
    async fn icebox_dependency_chain() {
        with_connection(|conn| {
            let ice = iced();
            ice.iter().for_each(|i| i.store(conn)?);

            let dep_chain = Icebox::backward_dep_chain(&conn, 41, 1)?;
            assert_eq!(dep_chain, ice);

            let dep_chain = Icebox::forward_dep_chain(&conn, 39, 2)?;
            assert_eq!(dep_chain, ice.into_iter().rev().collect::<Vec<_>>());
        })
        .await
    }

    #[xmtp_common::test(unwrap_try = "true")]
    async fn test_icebox_wrong_originator() {
        with_connection(|conn| {
            // Break the chain by unsetting the originator.
            let mut ice = iced();
            ice[2].originator_id = 1;
            ice.iter().for_each(|i| i.store(conn)?);

            let dep_chain = Icebox::backward_dep_chain(&conn, 41, 1)?;
            // The last iced message should not be there due to the wrong originator_id.
            let leftover = ice.pop()?;
            assert_eq!(dep_chain, ice);

            let dep_chain = Icebox::forward_dep_chain(&conn, 39, 1)?;
            assert_eq!(dep_chain, vec![leftover]);
        })
        .await
    }

    #[xmtp_common::test(unwrap_try = "true")]
    async fn test_icebox_wrong_sequence() {
        with_connection(|conn| {
            // Break the chain by unsetting the originator.
            let mut ice = iced();
            ice[2].sequence_id = 38;
            ice.iter().for_each(|i| i.store(conn)?);

            let dep_chain = Icebox::backward_dep_chain(&conn, 41, 1)?;
            // The last iced message should not be there due to the wrong originator_id.
            let leftover = ice.pop()?;
            assert_eq!(dep_chain, ice);

            let dep_chain = Icebox::forward_dep_chain(&conn, 38, 2)?;
            assert_eq!(dep_chain, vec![leftover]);
        })
        .await
    }

    #[xmtp_common::test(unwrap_try = "true")]
    async fn test_icebox_depending_fields_xor() {
        with_connection(|conn| {
            // Test to ensure that if one dependency field is set, they both are.
            let mut ice = Icebox {
                sequence_id: 2,
                originator_id: 2,
                depending_sequence_id: Some(1),
                depending_originator_id: None,
                envelope_payload: vec![1; 10],
            };
            let result = ice.store(&conn);
            assert!(result.is_err());

            ice.depending_originator_id = Some(1);
            ice.depending_sequence_id = None;
            let result = ice.store(&conn);
            assert!(result.is_err());

            ice.depending_sequence_id = Some(1);
            let result = ice.store(&conn);
            assert!(result.is_ok());
        })
        .await
    }
}
