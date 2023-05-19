use std::time::{SystemTime, UNIX_EPOCH};

use super::schema::messages;
use diesel::prelude::*;

#[derive(Queryable, Debug)]
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

impl NewDecryptedMessage {
    pub fn new(convo_id: String, addr_from: String, content: String) -> Self {
        Self {
            created_at: now(),
            convoid: convo_id,
            addr_from,
            content,
        }
    }
}

#[derive(Queryable)]
pub struct Channel {
    pub id: i32,
    pub channel_type: String,
    pub created_at: f32,
    pub sent_at: bool,
    pub contents: String,
}

// Diesel + Sqlite is giving trouble when trying to use f64 with REAL type. Downgraded to f32 timestamps
fn now() -> f32 {
    let start = SystemTime::now();
    start
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_secs_f32()
}
