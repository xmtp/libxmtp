#![recursion_limit = "256"]
#![cfg(target_arch = "wasm32")]

use diesel_migrations::embed_migrations;
use diesel_migrations::EmbeddedMigrations;
use diesel_wasm_sqlite::{connection::WasmSqliteConnection, DebugQueryWrapper, WasmSqlite};
use wasm_bindgen_test::*;
use web_sys::console;

use chrono::{NaiveDate, NaiveDateTime};
use diesel::connection::SimpleConnection;
use diesel::debug_query;
use diesel::insert_into;
use diesel::prelude::*;
use serde::Deserialize;
use std::error::Error;

wasm_bindgen_test_configure!(run_in_dedicated_worker);

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("./tests/web/migrations/");

mod schema {
    diesel::table! {
        books {
            id -> Integer,
            title -> Text,
            author -> Nullable<Text>,
            // published_year -> Timestamp,
        }
    }
}

use schema::books;

#[derive(Deserialize, Insertable, Debug, PartialEq, Clone)]
#[diesel(table_name = books)]
pub struct BookForm {
    title: String,
    author: Option<String>,
    // published_year: NaiveDateTime,
}

#[derive(Queryable, Selectable, PartialEq, Debug)]
pub struct Book {
    id: i32,
    title: String,
    author: Option<String>,
    // published_year: NaiveDateTime,
}

async fn establish_connection() -> WasmSqliteConnection {
    diesel_wasm_sqlite::init_sqlite().await;

    let rng: u16 = rand::random();
    let result = WasmSqliteConnection::establish(&format!("test-{}", rng));
    let mut conn = result.unwrap();
    // conn.run_pending_migrations(MIGRATIONS);
    //TODO: we can use `embed_migrations` to run our migrations
    tracing::info!("trying to establish...");

    conn.batch_execute(
        "
        CREATE TABLE books (
            id INTEGER PRIMARY KEY,
            title TEXT NOT NULL,
            author TEXT
        )
    ",
    )
    .expect("Batch exec failed to run");
    conn
}

fn insert_books(conn: &mut WasmSqliteConnection, new_books: Vec<BookForm>) -> QueryResult<usize> {
    use schema::books::dsl::*;
    let query = insert_into(books).values(new_books);
    let sql = DebugQueryWrapper::<_, WasmSqlite>::new(&query).to_string();
    tracing::info!("QUERY = {}", sql);
    let rows_changed = query.execute(conn).unwrap();
    Ok(rows_changed)
}

fn insert_book(conn: &mut WasmSqliteConnection, new_book: BookForm) -> QueryResult<usize> {
    use schema::books::dsl::*;
    let query = insert_into(books).values(new_book);
    let sql = debug_query::<WasmSqlite, _>(&query).to_string();
    tracing::info!("QUERY = {}", sql);
    let rows_changed = query.execute(conn).unwrap();
    Ok(rows_changed)
}

#[wasm_bindgen_test]
fn examine_sql_from_insert_default_values() {
    use schema::books::dsl::*;

    let query = insert_into(books).default_values();
    let sql = "INSERT INTO `books` DEFAULT VALUES -- binds: []";
    assert_eq!(sql, debug_query::<WasmSqlite, _>(&query).to_string());
    console::log_1(&debug_query::<WasmSqlite, _>(&query).to_string().into());
}

#[wasm_bindgen_test]
async fn test_orm_insert() {
    console_error_panic_hook::set_once();
    tracing_wasm::set_as_global_default();

    let mut conn = establish_connection().await;

    let rows_changed = insert_books(
        &mut conn,
        vec![
            BookForm {
                title: "Game of Thrones".into(),
                author: Some("George R.R".into()),
                // published_year: NaiveDate::from_ymd_opt(2015, 5, 3).unwrap(),
            },
            BookForm {
                title: "The Hobbit".into(),
                author: Some("J.R.R. Tolkien".into()),
                // published_year: NaiveDate::from_ymd_opt(1937, 9, 21).unwrap(),
            },
            BookForm {
                title: "To Kill a Mockingbird".into(),
                author: Some("Harper Lee".into()),
                // published_year: NaiveDate::from_ymd_opt(1960, 7, 11).unwrap(),
            },
            BookForm {
                title: "1984".into(),
                author: Some("George Orwell".into()),
                // published_year: NaiveDate::from_ymd_opt(1949, 6, 8).unwrap(),
            },
            BookForm {
                title: "Pride and Prejudice".into(),
                author: Some("Jane Austen".into()),
                // published_year: NaiveDate::from_ymd_opt(1813, 1, 28).unwrap(),
            },
            BookForm {
                title: "Moby-Dick".into(),
                author: Some("Herman Melville".into()),
                // published_year: NaiveDate::from_ymd_opt(1851, 10, 18).unwrap(),
            },
        ],
    )
    .unwrap();
    assert_eq!(rows_changed, 6);
    tracing::info!("{} rows changed", rows_changed);
    console::log_1(&"Showing Users".into());

    let books = schema::books::table
        .limit(5)
        .select(Book::as_select())
        .load(&mut conn)
        .unwrap();
    tracing::info!("BOOKS??? {:?}----------", books);

    // console::log_1(&debug_query::<WasmSqlite, _>(&query).to_string().into());
    // .load(&mut conn)
    // .await
    // .expect("Error loading users");

    /*
        for book in books {
            console::log_1(&format!("{}", book.title).into());
        }
    */
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
