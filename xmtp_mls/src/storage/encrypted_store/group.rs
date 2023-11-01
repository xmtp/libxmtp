use super::schema::groups;
use diesel::prelude::*;

#[derive(Insertable, Identifiable, Queryable, Debug, Clone)]
#[diesel(table_name = groups)]
#[diesel(primary_key(id))]
pub struct StoredGroup {
    pub id: Vec<u8>,
    pub created_at_ns: i64,
    pub membership_state: i32,
}
