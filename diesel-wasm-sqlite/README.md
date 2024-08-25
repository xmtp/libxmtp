# Diesel Backend for SQLite and WASM

### Use SQLite with Diesel ORM in your web apps!

## Quickstart

add `diesel-wasm-sqlite` to your project. SQLite is automatically bundled with
the library.

```toml
[dependencies]
diesel = { version = "2.2" }
diesel-wasm-sqlite = { git = "https://github.com/xmtp/libxmtp", branch = "wasm-backend" }
wasm-bindgen = "0.2"
```

```rust
use diesel_wasm_sqlite::{connection::WasmSqliteConnection, WasmSqlite};
use wasm_bindgen::prelude::*;

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("./tests/web/migrations/");

mod schema {
    diesel::table! {
        books {
            id -> Integer,
            title -> Text,
            author -> Nullable<Text>,
        }
    }
}

// SQLite must be instantiated in a web-worker
// to take advantage of OPFS
#[wasm_bindgen]
fn code_in_web_worker() -> Result<i32, diesel::QueryResult<usize>> {
    use schema::books::dsl::*;
    // init_sqlite must be ran before anything else, or we crash.
    diesel_wasm_sqlite::init_sqlite().await;

    // create a new persistent SQLite database with OPFS
    let result = WasmSqliteConnection::establish(&format!("test-{}", rng));
    let query = insert_into(books).values(vec![
        BookForm {
                title: "Game of Thrones".into(),
                author: Some("George R.R".into()),
            },
            BookForm {
                title: "The Hobbit".into(),
                author: Some("J.R.R. Tolkien".into()),
            },
    ]);
    Ok(query.execute(conn)?)
}
```

## Development

### Compile rust code without creating a npm package

`cargo build --target wasm32-unknown-unknown`

### Run Tests

wasm-pack test --safari

navigate to `http://localhost:8000` to observe test output

(headless tests don't work yet)

### Setting up the project in VSCode

rust-analyzer does not like crates with different targets in the same workspace.
If you want this to work well with your LSP, open `diesel-wasm-sqlite` as it's
own project in VSCode.
