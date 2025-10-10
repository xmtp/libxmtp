use super::*;

#[xmtp_common::test]
async fn update_dm_trigger() {
    let db = crate::TestDb::create_database(None).await;
    db.conn()
        .raw_query_read(|c| {
            db.validate(c).unwrap();
            Ok(())
        })
        .unwrap();

    db.conn()
        .raw_query_write(|conn| {
            for _ in 0..25 {
                conn.run_next_migration(MIGRATIONS).unwrap();
            }

            sql_query(
                r#"
            INSERT INTO user_preferences (
                hmac_key
            ) VALUES ($1)"#,
            )
            .bind::<Blob, _>(vec![1, 2, 3, 4, 5])
            .execute(conn)?;

            Ok(())
        })
        .unwrap();

    db.conn()
        .raw_query_write(|conn| {
            conn.run_pending_migrations(MIGRATIONS).unwrap();
            Ok(())
        })
        .unwrap();
}
