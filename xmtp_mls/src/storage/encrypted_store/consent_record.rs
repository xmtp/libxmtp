// use diesel::{
//     backend::Backend,
//     deserialize::{self, FromSql, FromSqlRow},
//     expression::AsExpression,
//     prelude::*,
//     serialize::{self, IsNull, Output, ToSql},
//     sql_types::Integer,
//     sqlite::Sqlite,
// };
// use serde::{Deserialize, Serialize};

// use super::{
//     db_connection::DbConnection,
//     schema::{groups, groups::dsl},
// };
// use crate::{impl_fetch, impl_store, StorageError};

// #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Insertable, Identifiable, Queryable)]
// #[diesel(table_name = consent_record)]
// #[diesel(primary_key(id))]
// pub struct StoredConsentRecord {
//     /// Enum, [`ConsentType`] representing access to the group
//     pub entity_type: ConsentType,
//     /// Enum, [`ConsentType`] representing access to the group
//     pub state: ConsentState,
//     /// The inbox_id of who added the user to a group.
//     pub entity: String,
// }
