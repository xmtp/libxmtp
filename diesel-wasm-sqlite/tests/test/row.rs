use crate::common::{connection, prelude::*};
use diesel_wasm_sqlite::dsl::RunQueryDsl;

// test copied from diesel
#[wasm_bindgen_test]
async fn fun_with_row_iters() {
    diesel_wasm_sqlite::init_sqlite().await;

    diesel::table! {
        #[allow(unused_parens)]
        users(id) {
            id -> Integer,
            name -> Text,
        }
    }

    use diesel::connection::LoadConnection;
    use diesel::deserialize::{FromSql, FromSqlRow};
    use diesel::row::{Field, Row};
    use diesel::sql_types;

    let conn: &mut WasmSqliteConnection = &mut connection().await;

    diesel::sql_query("CREATE TABLE users(id INTEGER PRIMARY KEY, name TEXT NOT NULL);")
        .execute(conn)
        .unwrap();

    diesel::insert_into(users::table)
        .values(vec![
            (users::id.eq(1), users::name.eq("Sean")),
            (users::id.eq(2), users::name.eq("Tess")),
        ])
        .execute(conn)
        .unwrap();

    let query = users::table.select((users::id, users::name));

    let expected = vec![(1, String::from("Sean")), (2, String::from("Tess"))];
    let row_iter = conn.load(query).unwrap();
    for (row, expected) in row_iter.zip(&expected) {
        let row = row.expect("Unwrap failed");

        let deserialized = <(i32, String) as FromSqlRow<
            (sql_types::Integer, sql_types::Text),
            _,
        >>::build_from_row(&row)
        .unwrap();

        assert_eq!(&deserialized, expected);
    }

    {
        let collected_rows = conn.load(query).unwrap().collect::<Vec<_>>();

        for (row, expected) in collected_rows.iter().zip(&expected) {
            let deserialized = row
                .as_ref()
                .map(|row| {
                    <(i32, String) as FromSqlRow<
                            (sql_types::Integer, sql_types::Text),
                        _,
                        >>::build_from_row(row).unwrap()
                })
                .unwrap();

            assert_eq!(&deserialized, expected);
        }
    }

    let mut row_iter = conn.load(query).unwrap();

    let first_row = row_iter.next().unwrap().unwrap();
    let first_fields = (first_row.get(0).unwrap(), first_row.get(1).unwrap());
    let first_values = (first_fields.0.value(), first_fields.1.value());

    assert!(row_iter.next().unwrap().is_err());
    std::mem::drop(first_values);
    assert!(row_iter.next().unwrap().is_err());
    std::mem::drop(first_fields);

    let second_row = row_iter.next().unwrap().unwrap();
    let second_fields = (second_row.get(0).unwrap(), second_row.get(1).unwrap());
    let second_values = (second_fields.0.value(), second_fields.1.value());

    assert!(row_iter.next().unwrap().is_err());
    std::mem::drop(second_values);
    assert!(row_iter.next().unwrap().is_err());
    std::mem::drop(second_fields);

    assert!(row_iter.next().is_none());

    let first_fields = (first_row.get(0).unwrap(), first_row.get(1).unwrap());
    let second_fields = (second_row.get(0).unwrap(), second_row.get(1).unwrap());

    let first_values = (first_fields.0.value(), first_fields.1.value());
    let second_values = (second_fields.0.value(), second_fields.1.value());

    assert_eq!(
        <i32 as FromSql<sql_types::Integer, WasmSqlite>>::from_nullable_sql(first_values.0)
            .unwrap(),
        expected[0].0
    );
    assert_eq!(
        <String as FromSql<sql_types::Text, WasmSqlite>>::from_nullable_sql(first_values.1)
            .unwrap(),
        expected[0].1
    );

    assert_eq!(
        <i32 as FromSql<sql_types::Integer, WasmSqlite>>::from_nullable_sql(second_values.0)
            .unwrap(),
        expected[1].0
    );
    assert_eq!(
        <String as FromSql<sql_types::Text, WasmSqlite>>::from_nullable_sql(second_values.1)
            .unwrap(),
        expected[1].1
    );

    let first_fields = (first_row.get(0).unwrap(), first_row.get(1).unwrap());
    let first_values = (first_fields.0.value(), first_fields.1.value());

    assert_eq!(
        <i32 as FromSql<sql_types::Integer, WasmSqlite>>::from_nullable_sql(first_values.0)
            .unwrap(),
        expected[0].0
    );
    assert_eq!(
        <String as FromSql<sql_types::Text, WasmSqlite>>::from_nullable_sql(first_values.1)
            .unwrap(),
        expected[0].1
    );
}
