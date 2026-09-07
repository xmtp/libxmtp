use crate::{config::Config, db::Store};
use sqlx::{Connection, PgConnection};

#[xmtp_common::test(unwrap_try = true)]
async fn concurrent_initializers_apply_one_migration_on_an_empty_database() {
    let admin_url = std::env::var("DATABASE_URL")?;
    let name = format!("backend_migration_{}", xmtp_common::rand_hexstring());
    let mut admin = PgConnection::connect(&admin_url).await?;
    // Only a fixed prefix and generated hexadecimal digits enter this identifier.
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE DATABASE \"{name}\"")))
        .execute(&mut admin)
        .await?;
    let mut url = url::Url::parse(&admin_url)?;
    url.set_path(&name);
    let config: Config = toml::from_str(&format!("[database]\nurl = {:?}", url.as_str()))?;
    let (left, right) = tokio::join!(Store::connect(&config), Store::connect(&config));
    let left = left?;
    let right = right?;
    let migrations = sqlx::query_scalar!("SELECT count(*) FROM _sqlx_migrations WHERE success")
        .fetch_one(&left.primary)
        .await?;
    assert_eq!(migrations, Some(1));
    assert_eq!(
        sqlx::query_scalar!("SELECT closed_sequence_id FROM allocation_boundary WHERE singleton")
            .fetch_one(&right.primary)
            .await?,
        0
    );
    left.primary.close().await;
    right.primary.close().await;
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "DROP DATABASE \"{name}\" WITH (FORCE)"
    )))
    .execute(&mut admin)
    .await?;
}
