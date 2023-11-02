use super::schema::group_intents;
use diesel::prelude::*;

#[derive(Queryable, Identifiable, Debug, Clone)]
#[diesel(table_name = group_intents)]
#[diesel(primary_key(id))]
pub struct StoredGroupIntent {
    pub id: i32,
    pub kind: i32,
    pub state: i32,
    pub group_id: Vec<u8>,
    pub data: Vec<u8>,
    pub payload_hash: Option<Vec<u8>>,
    pub post_commit_data: Option<Vec<u8>>,
}

#[derive(Insertable, Debug, Clone)]
#[diesel(table_name = group_intents)]
pub struct NewGroupIntent {
    pub kind: i32,
    pub state: i32,
    pub group_id: Vec<u8>,
    pub data: Vec<u8>,
}
