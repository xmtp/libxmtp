use super::*;
// verifies: ATCH-040
#[xmtp_common::test(unwrap_try = true)]
fn deployment_dirs_do_not_collide() {
    let values = ["acme/prod", "beta/prod", "Prod", "prod", "a:b", "ab"];
    let names: Vec<_> = values
        .iter()
        .map(|value| deployment_component(value))
        .collect();
    let distinct: std::collections::HashSet<_> = names.iter().collect();
    assert_eq!(distinct.len(), values.len());
    assert!(names.iter().all(|name| name.len() <= 255));
    assert!(
        names
            .iter()
            .all(|name| name.bytes().all(|b| b.is_ascii_lowercase()
                || b.is_ascii_digit()
                || b == b'-'
                || b == b'.'
                || b == b'_'))
    );
    let long = deployment_component(&"x".repeat(256));
    assert_eq!(long.len(), 255);
    assert_eq!(long.split('-').next().map(str::len), Some(190));
}
// verifies: ATCH-040
#[xmtp_common::test(unwrap_try = true)]
fn database_name_uses_deployment_and_inbox() {
    let inbox = "A1B2C3";
    let location = StorageLocation::DataDir(PathBuf::from("client-data"));
    let paths = location.resolve_identifier(inbox, "production")?;
    let name = paths.db_path.to_string_lossy().into_owned();
    let expected = format!(
        "client-data/{}/a1b2c3/xmtp.db3",
        deployment_component("production")
    );
    assert_eq!(name, expected);
    let option = xmtp_db::StorageOption::Persistent(name);
    assert!(matches!(option, xmtp_db::StorageOption::Persistent(ref path) if path == &expected));
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::*;

    // verifies: ATCH-040, ATCH-069
    #[xmtp_common::test(unwrap_try = true)]
    async fn bound_recorder_keeps_the_opened_deployment() {
        let dir = tempfile::tempdir()?;
        let recorder = DeploymentRecorder::new(dir.path().to_path_buf(), "http://localhost");
        recorder.record("deployment-a").await?;
        let bound = recorder
            .clone()
            .for_opened_identifier("deployment-a".into());
        assert!(matches!(
            bound.record("deployment-b").await,
            Err(StorageLocationError::DeploymentMismatch)
        ));
        assert_eq!(recorder.lookup().await?, Some("deployment-a".into()));
    }

    // verifies: ATCH-077
    #[cfg(unix)]
    #[xmtp_common::test(unwrap_try = true)]
    async fn deployment_record_is_readable_under_restrictive_umask() {
        use std::os::unix::fs::PermissionsExt as _;

        struct RestoreUmask(libc::mode_t);
        impl Drop for RestoreUmask {
            fn drop(&mut self) {
                unsafe { libc::umask(self.0) };
            }
        }

        let dir = tempfile::tempdir()?;
        let recorder = DeploymentRecorder::new(dir.path().to_path_buf(), "http://localhost");
        let _umask = RestoreUmask(unsafe { libc::umask(0o400) });
        recorder.record("deployment-a").await?;
        let record_path = dir.path().join("deployments.json");
        assert_eq!(
            std::fs::metadata(&record_path)?.permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(recorder.lookup().await?, Some("deployment-a".into()));
        recorder.record("deployment-b").await?;
        assert_eq!(
            std::fs::metadata(record_path)?.permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(recorder.lookup().await?, Some("deployment-b".into()));
    }

    // Covers plan P19.
    #[cfg(unix)]
    #[xmtp_common::test(unwrap_try = true)]
    async fn failed_record_keeps_the_prior_file() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir()?;
        let recorder = DeploymentRecorder::new(dir.path().to_path_buf(), "http://localhost");
        recorder.record("first").await?;
        let before = tokio::fs::read(dir.path().join("deployments.json")).await?;
        let old_mode = std::fs::metadata(dir.path())?.permissions().mode();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o555))?;
        let result = recorder.record("second").await;
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(old_mode))?;
        assert!(matches!(result, Err(StorageLocationError::Io(_))));
        assert_eq!(
            tokio::fs::read(dir.path().join("deployments.json")).await?,
            before
        );
        let entries = std::fs::read_dir(dir.path())?.collect::<Result<Vec<_>, _>>()?;
        assert_eq!(entries.len(), 1);
    }

    // verifies: ATCH-069
    #[xmtp_common::test(unwrap_try = true)]
    async fn cancelled_native_deployment_write_removes_temp() {
        use std::sync::Arc;
        let dir = tempfile::tempdir()?;
        let entered = Arc::new(tokio::sync::Notify::new());
        let resume = Arc::new(tokio::sync::Notify::new());
        *NATIVE_DEPLOYMENT_WRITE_PAUSE.lock() = Some((entered.clone(), resume.clone()));
        let path = dir.path().to_path_buf();
        let (write, abort) =
            futures::future::abortable(async move { write_file(&path, b"cancelled record").await });
        let task = xmtp_common::task::spawn(write);
        xmtp_common::time::timeout(std::time::Duration::from_secs(3), entered.notified()).await?;
        abort.abort();
        assert!(task.await?.is_err());
        resume.notify_one();
        xmtp_common::time::timeout(std::time::Duration::from_secs(3), async {
            while !dir.path().join("deployments.json").exists() {
                xmtp_common::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await?;
        let temps: Vec<_> = std::fs::read_dir(dir.path())?
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".deployments-")
            })
            .collect();
        assert!(temps.is_empty(), "cancelled write left a temporary file");
    }

    // verifies: ATCH-069
    #[xmtp_common::test(unwrap_try = true)]
    async fn cancelled_record_write_finishes_before_next_record() {
        use std::sync::Arc;
        let dir = tempfile::tempdir()?;
        let recorder = DeploymentRecorder::new(dir.path().to_path_buf(), "http://localhost");
        let entered = Arc::new(tokio::sync::Notify::new());
        let resume = Arc::new(tokio::sync::Notify::new());
        *NATIVE_DEPLOYMENT_WRITE_PAUSE.lock() = Some((entered.clone(), resume.clone()));
        let first = recorder.clone();
        let (write, abort) = futures::future::abortable(async move { first.record("first").await });
        let first_task = xmtp_common::task::spawn(write);
        xmtp_common::time::timeout(std::time::Duration::from_secs(3), entered.notified()).await?;
        abort.abort();
        assert!(first_task.await?.is_err());

        let second = recorder.clone();
        let (sent, mut received) = tokio::sync::oneshot::channel();
        drop(xmtp_common::task::spawn(async move {
            let _ = sent.send(second.record("second").await);
        }));
        let early =
            tokio::time::timeout(std::time::Duration::from_millis(500), &mut received).await;
        resume.notify_one();
        match early {
            Ok(result) => result??,
            Err(_) => {
                xmtp_common::time::timeout(std::time::Duration::from_secs(3), received).await???
            }
        }
        xmtp_common::time::timeout(std::time::Duration::from_secs(3), async {
            while std::fs::read_dir(dir.path())
                .expect("read deployment directory")
                .filter_map(Result::ok)
                .any(|entry| {
                    entry
                        .file_name()
                        .to_string_lossy()
                        .starts_with(".deployments-")
                })
            {
                xmtp_common::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await?;
        assert_eq!(recorder.lookup().await?, Some("second".to_owned()));
    }
}
