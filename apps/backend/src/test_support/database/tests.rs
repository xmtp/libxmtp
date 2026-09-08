use crate::test_support::{TestResult, TestServer};
use sqlx::{Connection, PgConnection};

#[xmtp_common::test(unwrap_try = true)]
async fn failed_initialization_errors_and_panics_leave_no_database_after_runtime_exit() {
    let (send, names) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        for failure in ["initialization", "error", "assertion"] {
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                runtime.block_on(async {
                    let server = TestServer::new(|config| {
                        let url = url::Url::parse(&config.database.url).unwrap();
                        send.send(url.path().trim_start_matches('/').to_owned())
                            .unwrap();
                        if failure == "initialization" {
                            config.database.max_connections = 0;
                        }
                    })
                    .await?;
                    if failure == "assertion" {
                        panic!("intentional fixture cleanup assertion");
                    }
                    let _server = server;
                    Err::<(), _>("intentional fixture cleanup error".into())
                }) as TestResult
            }));
            assert!(outcome.is_err() || outcome.unwrap().is_err());
        }
        runtime.shutdown_timeout(std::time::Duration::from_millis(100));
    });
    worker.join().expect("cleanup scenarios complete");
    let names: Vec<_> = names.into_iter().collect();
    assert_eq!(names.len(), 3);
    let mut admin = PgConnection::connect(&std::env::var("DATABASE_URL")?).await?;
    for name in names {
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_database WHERE datname = $1)")
                .bind(name)
                .fetch_one(&mut admin)
                .await?;
        assert!(
            !exists,
            "failure cleanup must finish before the test runtime exits"
        );
    }
}
