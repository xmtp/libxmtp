use super::*;
use diesel::connection::InstrumentationEvent;

// verifies: MIG-002, MIG-004
#[xmtp_common::test(unwrap_try = true)]
async fn dropped_call_stops_remaining_migrations_and_removes_copy() {
    const COMPLETED_PREFIX: usize = 55;
    for previous_output in [false, true] {
        let (directory, args) = fixture("early.db3");
        let before = source_bytes(&args);
        if previous_output {
            fs::write(&args.output_path, b"completed archive")?;
        }
        let entries_before = fs::read_dir(directory.path())?.count();
        let (entered, ready) = tokio::sync::oneshot::channel();
        let (release, gate) = std::sync::mpsc::channel();
        let (finished, done) = tokio::sync::oneshot::channel();
        let worker_args = args.clone();
        let mut future = Box::pin(offload(move |cancel| {
            let output_path = Path::new(&worker_args.output_path);
            let temporary =
                working_copy(Path::new(&worker_args.database_path), output_path, cancel)?;
            let copy_path = temporary.path().to_owned();
            let mut conn = open_copy(&copy_path.join("source.db3"), None)?;
            let mut entered = Some(entered);
            conn.set_instrumentation(move |event: InstrumentationEvent<'_>| {
                if let InstrumentationEvent::FinishQuery { query, error, .. } = event
                    && query
                        .to_string()
                        .contains("CREATE TABLE icebox_dependencies")
                {
                    assert!(error.is_none());
                    // The real table rebuild has run; its transaction is still open.
                    entered.take().unwrap().send(()).unwrap();
                    gate.recv().unwrap();
                }
            });
            let migrated = crate::migrations::apply(&mut conn, || live(cancel).map_err(output));
            let completed = crate::migrations::validate(&mut conn)?;
            let result = migrated.and_then(|()| {
                write_output(output_path, cancel, |sink| {
                    sink.write_all(b"unexpected output").map_err(output)
                })
            });
            drop(conn);
            drop(temporary);
            finished
                .send((result, completed, copy_path.exists()))
                .unwrap();
            Ok(())
        }));
        assert!(futures::poll!(future.as_mut()).is_pending());
        ready.await?;
        drop(future);
        release.send(())?;
        let (result, completed, copy_exists) = done.await?;
        assert!(matches!(result, Err(MigrationError::Output(_))));
        assert_eq!(
            completed, COMPLETED_PREFIX,
            "cancellation applied the remaining migration suffix"
        );
        assert!(
            !copy_exists,
            "cancelled migration retained its working copy"
        );
        assert_eq!(source_bytes(&args), before);
        if previous_output {
            assert_eq!(fs::read(&args.output_path)?, b"completed archive");
        } else {
            assert!(!Path::new(&args.output_path).exists());
        }
        assert_eq!(fs::read_dir(directory.path())?.count(), entries_before);
    }
}
