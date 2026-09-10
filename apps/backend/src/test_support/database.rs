use super::TestResult;
use sqlx::{Connection, PgConnection};
use std::time::Duration;

const DATABASE_COMMAND_TIMEOUT: Duration = Duration::from_secs(5);
const RUNTIME_SHUTDOWN_TIMEOUT: Duration = Duration::from_millis(100);
#[cfg(test)]
mod tests;

/// Own a unique disposable database. Synchronous, bounded cleanup uses a separate
/// runtime so assertion failures and test-runtime teardown cannot cancel it.
/// Each database operation and its runtime shutdown share a 5 s budget.
/// Drop logs a failed cleanup with the database name. It does not raise the error.
pub struct TestDatabase {
    name: Option<String>,
    admin_url: String,
    url: String,
}

impl TestDatabase {
    /// Create an isolated database before returning its cleanup guard.
    pub fn new() -> TestResult<Self> {
        let admin_url = std::env::var("DATABASE_URL")?;
        let name = format!("backend_test_{}", xmtp_common::rand_hexstring());
        let mut url = url::Url::parse(&admin_url)?;
        url.set_path(&name);
        let mut database = Self {
            name: Some(name.clone()),
            admin_url,
            url: url.to_string(),
        };
        // The identifier contains only a fixed prefix and generated hexadecimal digits.
        if let Err(error) =
            database_command(&database.admin_url, format!("CREATE DATABASE \"{name}\""))
        {
            if matches!(error.downcast_ref::<sqlx::Error>(), Some(sqlx::Error::Database(error)) if error.code().as_deref() == Some("42P04"))
            {
                database.name = None;
            }
            return Err(error);
        }
        Ok(database)
    }

    /// List database names on the server selected by DATABASE_URL.
    pub async fn names() -> TestResult<Vec<String>> {
        let mut admin = PgConnection::connect(&std::env::var("DATABASE_URL")?).await?;
        Ok(sqlx::query_scalar("SELECT datname::text FROM pg_database")
            .fetch_all(&mut admin)
            .await?)
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    /// Delete only this fixture's database. Keep ownership if cleanup fails so
    /// Drop can retry without hiding the original test failure.
    pub fn remove(&mut self) -> TestResult {
        if let Some(name) = &self.name {
            database_command(
                &self.admin_url,
                format!("DROP DATABASE IF EXISTS \"{name}\" WITH (FORCE)"),
            )?;
            self.name = None;
        }
        Ok(())
    }
}

impl Drop for TestDatabase {
    fn drop(&mut self) {
        if let Err(error) = self.remove() {
            tracing::error!(database = self.name.as_deref(), %error, "disposable test database cleanup failed");
        }
    }
}

/// Complete CREATE before exposing the fixture, and DROP before returning from
/// cleanup. Neither operation depends on the lifetime of the calling test runtime.
fn database_command(admin_url: &str, command: String) -> TestResult {
    let admin_url = admin_url.to_owned();
    std::thread::spawn(move || -> TestResult {
        let started = xmtp_common::time::Instant::now();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let result = runtime.block_on(xmtp_common::time::timeout(
            DATABASE_COMMAND_TIMEOUT.saturating_sub(started.elapsed()),
            async {
                let mut admin = PgConnection::connect(&admin_url).await?;
                sqlx::raw_sql(sqlx::AssertSqlSafe(command))
                    .execute(&mut admin)
                    .await?;
                Ok::<_, sqlx::Error>(())
            },
        ));
        runtime.shutdown_timeout(
            RUNTIME_SHUTDOWN_TIMEOUT
                .min(DATABASE_COMMAND_TIMEOUT.saturating_sub(started.elapsed())),
        );
        result??;
        Ok(())
    })
    .join()
    .map_err(|_| "test database command thread failed")?
}
