//! General tests for migrations/diesel ORM/persistant databases
use crate::common::prelude::*;

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("./tests/migrations/");

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
#[diesel(table_name = books)]
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
    tracing::info!("running migrations...");
    conn.run_pending_migrations(MIGRATIONS).unwrap();
    conn
}

fn insert_books(conn: &mut WasmSqliteConnection, new_books: Vec<BookForm>) -> QueryResult<usize> {
    use schema::books::dsl::*;
    let query = insert_into(books).values(new_books);
    let sql = DebugQueryWrapper::<_, WasmSqlite>::new(&query).to_string();
    tracing::info!("QUERY = {}", sql);
    let rows_changed = query.execute(conn)?;
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
/*
#[wasm_bindgen_test]
fn examine_sql_from_insert_default_values() {
    use schema::books::dsl::*;

    let query = insert_into(books).default_values();
    let sql = "INSERT INTO `books` DEFAULT VALUES -- binds: []";
    assert_eq!(sql, debug_query::<WasmSqlite, _>(&query).to_string());
    console::log_1(&debug_query::<WasmSqlite, _>(&query).to_string().into());
}
*/

#[wasm_bindgen_test]
async fn test_orm_insert() {
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
    let book = schema::books::table
        .select(schema::books::title)
        .load::<String>(&mut conn);
    tracing::info!("Loaded book {:?}", book);
    let query = schema::books::table.limit(5).select(Book::as_select());
    let books = conn.load(query).unwrap().collect::<Vec<_>>();
}
