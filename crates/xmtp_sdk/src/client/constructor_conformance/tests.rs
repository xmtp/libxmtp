use super::*;

fn options() -> ClientOptions {
    ClientOptions {
        backend: Some(crate::BackendSource::Options {
            options: crate::BackendOptions {
                url: xmtp_configuration::backend_test_url(),
                app_version: None,
                credentials: None,
                credential: None,
            },
        }),
        storage: crate::StorageOptions {
            location: crate::StorageLocation::InMemory,
            ..Default::default()
        },
        device_sync: false,
        registration: crate::client::RegistrationOptions {
            auto: false,
            ..Default::default()
        },
        ..Default::default()
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn constructor_wait_bounds_pre_spawn_failure() {
    let probe = SdkConformanceConstructorProbe::open().await;
    let settings = ClientOptions {
        fork_recovery: Some(crate::client::ForkRecoveryOptions {
            groups: vec![crate::ConversationId::unchecked("malformed".into())],
            ..Default::default()
        }),
        ..options()
    };
    let rejected = probe
        .create(crate::generate_local_signer().await, settings)
        .await;
    assert!(matches!(rejected, Err(XmtpError::InvalidArgument(_))));
    assert!(probe.probe.task.lock().is_none());
    let result = xmtp_common::time::timeout(
        xmtp_common::time::Duration::from_millis(200),
        probe.wait_for_completed(),
    )
    .await;
    probe.cleanup().await?;
    let error = result
        .expect("completion wait escaped its pre-spawn bound")
        .unwrap_err();
    assert!(matches!(error, XmtpError::Unknown(_)));
}

#[xmtp_common::test(unwrap_try = true)]
async fn constructor_cleanup_preserves_first_error_and_closes() {
    let probe = SdkConformanceConstructorProbe::open().await;
    probe.release();
    let path = xmtp_common::tmp_path();
    let settings = ClientOptions {
        storage: crate::StorageOptions {
            location: crate::StorageLocation::Explicit {
                db_path: path.clone(),
                attachments_dir: format!("{path}.attachments"),
            },
            ..Default::default()
        },
        ..options()
    };
    probe
        .create(crate::generate_local_signer().await, settings)
        .await?;
    probe.cleanup_end_failure.store(true, Ordering::SeqCst);
    let first = probe.cleanup().await;
    let state = probe.state();
    // Finish fixture cleanup before an assertion can fail.
    probe.cleanup().await?;
    std::fs::remove_file(path)?;
    let error = first.unwrap_err();
    let XmtpError::Unknown(details) = error else {
        panic!("cleanup lost the first error")
    };
    assert_eq!(details.message, "constructor cleanup end failed");
    assert!(
        state.client_closed,
        "fallback close did not close the client"
    );
    assert!(state.workers_stopped, "fallback close left workers active");
    assert!(
        !state.store_connected,
        "fallback close left the database connected"
    );
}
