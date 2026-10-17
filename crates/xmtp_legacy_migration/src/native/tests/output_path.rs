use super::*;
use std::os::unix::fs::symlink;

// verifies: MIG-002, MIG-004
#[xmtp_common::test(unwrap_try = true)]
fn output_parent_symlink_change_does_not_replace_source() {
    let mut failures = vec![];
    for previous in [false, true] {
        for before_open in [false, true] {
            for outcome in ["success", "failure", "cancel"] {
                let (directory, args) = fixture("stable.db3");
                let before = source_bytes(&args);
                let destination = directory.path().join("output");
                fs::create_dir(&destination)?;
                let published = destination.join("source.db3");
                if previous {
                    fs::write(&published, b"completed archive")?;
                }
                let alias = directory.path().join("alias");
                symlink(&destination, &alias)?;
                let output_path = alias.join("source.db3");
                let cancel = CancellationToken::new();
                let (_copy, output_path) =
                    working_copy(Path::new(&args.database_path), &output_path, &cancel)?;
                let retarget = || -> io::Result<()> {
                    fs::remove_file(&alias)?;
                    symlink(directory.path(), &alias)
                };
                // Change the parent at each output phase, after source admission.
                if before_open {
                    retarget()?;
                }
                let result = write_output(&output_path, &cancel, |sink| {
                    sink.write_all(b"new completed archive").map_err(output)?;
                    if !before_open {
                        retarget().map_err(output)?;
                    }
                    match outcome {
                        "failure" => Err(output(io::Error::other("write failed"))),
                        "cancel" => {
                            cancel.cancel();
                            Ok(())
                        }
                        _ => Ok(()),
                    }
                });
                let case =
                    format!("previous={previous}, before_open={before_open}, outcome={outcome}");
                if source_bytes(&args) != before {
                    failures.push(format!("{case}: publication replaced source"));
                }
                if result.is_ok() != (outcome == "success") {
                    failures.push(format!("{case}: unexpected result {result:?}"));
                }
                let expected = if outcome == "success" {
                    Some(b"new completed archive".as_slice())
                } else if previous {
                    Some(b"completed archive".as_slice())
                } else {
                    None
                };
                if fs::read(&published).ok().as_deref() != expected {
                    failures.push(format!("{case}: checked output differs"));
                }
                let expected_entries = if expected.is_some() {
                    vec![std::ffi::OsString::from("source.db3")]
                } else {
                    vec![]
                };
                if entries(&destination) != expected_entries {
                    failures.push(format!("{case}: output directory entries differ"));
                }
            }
        }
    }
    assert!(failures.is_empty(), "output parent changed: {failures:?}");
}

// verifies: MIG-001, MIG-002, MIG-004
#[xmtp_common::test(unwrap_try = true)]
async fn output_parent_change_in_run_preserves_source() {
    let mut failures = vec![];
    for previous in [false, true] {
        let (directory, mut args) = fixture("stable.db3");
        let before = source_bytes(&args);
        let destination = directory.path().join("output");
        fs::create_dir(&destination)?;
        let published = destination.join("source.db3");
        if previous {
            fs::write(&published, b"completed archive")?;
        }
        let alias = directory.path().join("alias");
        symlink(&destination, &alias)?;
        args.output_path = alias.join("source.db3").to_str().unwrap().to_owned();
        let result = run(
            args.clone(),
            &CancellationToken::new(),
            |source, destination_path, cancel| {
                let copy = working_copy(source, destination_path, cancel)?;
                fs::remove_file(&alias).map_err(output)?;
                symlink(directory.path(), &alias).map_err(output)?;
                Ok(copy)
            },
        );
        if source_bytes(&args) != before {
            failures.push(format!("previous={previous}: run replaced source"));
            continue;
        }
        let report = result?;
        assert_eq!(report.message_count, 3);
        assert_eq!(elements(published.to_str().unwrap()).await.len(), 6);
        assert_eq!(
            entries(&destination),
            vec![std::ffi::OsString::from("source.db3")]
        );
    }
    assert!(
        failures.is_empty(),
        "run used a changed output parent: {failures:?}"
    );
}
