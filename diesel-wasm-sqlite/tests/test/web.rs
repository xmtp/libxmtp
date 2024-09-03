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

    diesel::table! {
        test_table (id, id2) {
            id -> Text,
            id2 -> BigInt,
            timestamp_ns -> BigInt,
            payload -> Binary,
        }
    }
}

use schema::{books, test_table};

#[derive(Deserialize, Insertable, Debug, PartialEq, Clone)]
#[diesel(table_name = books)]
pub struct BookForm {
    title: String,
    author: Option<String>,
    // published_year: NaiveDateTime,
}

#[derive(Queryable, QueryableByName, Selectable, PartialEq, Debug)]
#[diesel(table_name = books)]
pub struct StoredBook {
    #[diesel(sql_type = Integer)]
    id: i32,
    #[diesel(sql_type = Text)]
    title: String,
    #[diesel(sql_type = Nullable<String>)]
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
    init().await;
    let mut conn = establish_connection().await;
    let new_books = vec![
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
    ];
    let rows_changed = insert_books(&mut conn, new_books).unwrap();
    assert_eq!(rows_changed, 6);
    tracing::info!("{} rows changed", rows_changed);
    console::log_1(&"Showing Users".into());
    let title = schema::books::table
        .select(schema::books::title)
        .filter(schema::books::id.eq(2))
        .first::<String>(&mut conn)
        .unwrap();
    assert_eq!(title, "The Hobbit");

    let author = schema::books::table
        .select(schema::books::author)
        .filter(schema::books::id.eq(1))
        .first::<Option<String>>(&mut conn)
        .unwrap();
    assert_eq!(author, Some("George R.R".into()));

    let id = schema::books::table
        .select(schema::books::id)
        .filter(schema::books::id.eq(1))
        .first::<i32>(&mut conn)
        .unwrap();
    assert_eq!(id, 1);

    let loaded_books = schema::books::dsl::books
        .select(StoredBook::as_select())
        .limit(5)
        .load(&mut conn);
    assert_eq!(
        loaded_books.unwrap(),
        vec![
            StoredBook {
                id: 1,
                title: "Game of Thrones".into(),
                author: Some("George R.R".into()),
            },
            StoredBook {
                id: 2,
                title: "The Hobbit".into(),
                author: Some("J.R.R. Tolkien".into()),
            },
            StoredBook {
                id: 3,
                title: "To Kill a Mockingbird".into(),
                author: Some("Harper Lee".into()),
            },
            StoredBook {
                id: 4,
                title: "1984".into(),
                author: Some("George Orwell".into()),
            },
            StoredBook {
                id: 5,
                title: "Pride and Prejudice".into(),
                author: Some("Jane Austen".into()),
            },
        ]
    )
}


/// StoredIdentityUpdate holds a serialized IdentityUpdate record
#[derive(Insertable, Identifiable, Queryable, Debug, Clone, PartialEq, Eq)]
#[diesel(table_name = test_table)]
#[diesel(primary_key(id, id2))]
pub struct Item {
    pub id: String,
    pub id2: i64,
    pub timestamp_ns: i64,
    pub payload: Vec<u8>,
}

fn insert_or_ignore(updates: &[Item], conn: &mut WasmSqliteConnection) {
    use schema::test_table::dsl::*;

    diesel::insert_or_ignore_into(test_table)
        .values(updates)
        .execute(conn).unwrap();

}

#[wasm_bindgen_test]
async fn can_insert_or_ignore() {
    use schema::books::dsl::*;

    init().await;
    let mut conn = establish_connection().await;
    let updates = vec![
        Item {
            id: "test".into(),
            id2: 13,
            timestamp_ns: 1231232,
            payload: b"testing this testing this".to_vec()
        },
        Item {
            id: "test2".into(),
            id2: 14,
            timestamp_ns: 1201222,
            payload: b"222testing this testing this".to_vec()

        }
    ];
    insert_or_ignore(&updates, &mut conn);

}


