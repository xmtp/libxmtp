use super::schema::messages;
use diesel::prelude::*;

#[derive(Queryable)]
pub struct DecryptedMessage {
    pub id: i32,
    pub created_at: f32,
    pub convoid: String,
    pub addr_from: String,
    pub content: String,
}

#[derive(Insertable)]
#[diesel(table_name = messages)]
pub struct NewDecryptedMessage {
    pub created_at: f32,
    pub convoid: String,
    pub addr_from: String,
    pub content: String,
}

#[derive(Queryable)]
pub struct Channel {
    pub id: i32,
    pub channel_type: String,
    pub created_at: f32,
    pub sent_at: bool,
    pub contents: String,
}
