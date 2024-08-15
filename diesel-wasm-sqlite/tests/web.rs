#![cfg(target_arch = "wasm32")]

use diesel::connection::Connection;
use diesel_wasm_sqlite::connection::{AsyncConnection, WasmSqliteConnection};
use wasm_bindgen_test::*;
use web_sys::console;

use crate::WasmSqliteConnection;
use chrono::NaiveDateTime;
use diesel::debug_query;
use diesel::insert_into;
use diesel::prelude::*;
use serde::Deserialize;
use std::error::Error;

wasm_bindgen_test_configure!(run_in_dedicated_worker);

mod schema {
    diesel::table! {
        users {
            id -> Integer,
            name -> Text,
            hair_color -> Nullable<Text>,
            created_at -> Timestamp,
            updated_at -> Timestamp,
        }
    }
}

use schema::users;

#[derive(Deserialize, Insertable)]
#[diesel(table_name = users)]
pub struct UserForm<'a> {
    name: &'a str,
    hair_color: Option<&'a str>,
}

#[derive(Queryable, PartialEq, Debug)]
pub struct User {
    id: i32,
    name: String,
    hair_color: Option<String>,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
}

pub fn insert_default_values(conn: &mut WasmSqliteConnection) -> QueryResult<usize> {
    use schema::users::dsl::*;

    insert_into(users).default_values().execute(conn)
}

#[test]
fn examine_sql_from_insert_default_values() {
    use schema::users::dsl::*;

    let query = insert_into(users).default_values();
    let sql = "INSERT INTO `users` DEFAULT VALUES -- binds: []";
    assert_eq!(sql, debug_query::<Sqlite, _>(&query).to_string());
}

/*
#[wasm_bindgen_test]
async fn test_establish_and_exec() {
    let rng: u16 = rand::random();
    let result = WasmSqliteConnection::establish("test-15873").await;
    let mut conn = result.unwrap();
    console::log_1(&"CONNECTED".into());

    let raw = conn.raw_connection;

    console::log_1(&"CREATE".into());
    raw.exec(
        "
        CREATE TABLE books (
            id INTEGER PRIMARY KEY,
            title TEXT NOT NULL,
            author TEXT NOT NULL,
            published_year INTEGER,
            genre TEXT
        );",
    )
    .await;

    console::log_1(&"INSERT".into());
    raw.exec(
        "
            INSERT INTO books (title, author, published_year, genre) VALUES
            ('To Kill a Mockingbird', 'Harper Lee', 1960, 'Fiction'),
            ('1984', 'George Orwell', 1949, 'Dystopian'),
            ('The Great Gatsby', 'F. Scott Fitzgerald', 1925, 'Classics'),
            ('Pride and Prejudice', 'Jane Austen', 1813, 'Romance');
    ",
    )
    .await;

    console::log_1(&"SELECT ALL".into());
    raw.exec("SELECT * FROM books").await;

    console::log_1(&"SELECT title, author FROM books WHERE published_year > 1950;".into());
    raw.exec(
        "

        SELECT title, published_year FROM books WHERE author = 'George Orwell';
    ",
    )
    .await;

    console::log_1(
        &"SELECT title, published_year FROM books WHERE author = 'George Orwell';".into(),
    );
    raw.exec("SELECT title, author FROM books WHERE published_year > 1950;".into())
        .await;
}
*/
