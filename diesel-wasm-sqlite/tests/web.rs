#![cfg(target_arch = "wasm32")]

use diesel::connection::Connection;
use diesel_wasm_sqlite::connection::{AsyncConnection, WasmSqliteConnection};
use wasm_bindgen_test::*;
use web_sys::console;
wasm_bindgen_test_configure!(run_in_dedicated_worker);

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
