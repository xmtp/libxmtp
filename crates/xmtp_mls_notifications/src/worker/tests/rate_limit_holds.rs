//! Notification worker tests: rate limit holds.

use super::*;

/// A rate limit maps to the retryable `Api` error, also through the preflight
/// wrapper. The detail on another code keeps that code's PUSH-267 row.
// verifies: PUSH-267, RATE-041
#[xmtp_common::test(unwrap_try = true)]
fn notification_rate_limits_map_to_api() {
    use xmtp_api_grpc::error::{GrpcError, rate_limited_status};
    use xmtp_proto::api::ApiClientError;
    let delay = Duration::from_secs(5);
    let preflight = xmtp_api::ApiError::Preflight(xmtp_api::preflight::PreflightError::new(
        ApiClientError::client(GrpcError::Status(rate_limited_status("request", delay))),
    ));
    let mapped = NotificationError::from(preflight);
    assert!(matches!(mapped, NotificationError::Api(_)));
    assert_eq!(mapped.rate_limit_delay(), Some(delay));

    let limited = rate_limited_status("request", delay);
    let denied = tonic::Status::with_details(
        tonic::Code::PermissionDenied,
        limited.message(),
        limited.details().to_vec().into(),
    );
    let mapped = NotificationError::from(xmtp_api::dyn_err(ApiClientError::client(
        GrpcError::Status(denied),
    )));
    assert!(matches!(mapped, NotificationError::PermissionDenied));
}

/// A refused Unregister records its delay even though the client is now
/// disabled, so the task waits it out after a later enable.
// verifies: RATE-041
#[xmtp_common::test(unwrap_try = true)]
async fn notification_a_refused_unregister_records_its_delay() {
    let (client, peer) = support::peer().await;
    client.enable_notifications(config()).await?;
    peer.state.lock().rate_limit = Some((Call::Unregister, Duration::from_secs(600)));
    assert!(client.disable_notifications().await.is_err());
    let record = client.db().notification_record()?;
    assert!(deadlines(&record)?.registration_rate_limited_until_ns > time::now_ns());
}

/// A hold that cannot be stored does not replace the refusal: the caller
/// still sees the rate limit and its delay.
// verifies: RATE-041
#[xmtp_common::test(unwrap_try = true)]
async fn notification_a_failed_hold_write_keeps_the_rate_limit() {
    let (client, peer) = support::peer().await;
    client.enable_notifications(config()).await?;
    let delay = Duration::from_secs(600);
    {
        let mut state = peer.state.lock();
        state.rate_limit = Some((Call::Unregister, delay));
        state.pause_next = true;
    }
    let disabling = client.clone();
    let disable = xmtp_common::spawn(None, async move { disabling.disable_notifications().await });
    timeout(Duration::from_secs(5), peer.entered.notified()).await?;
    let mut record = client.db().notification_record()?;
    record.push_deadlines = Some(vec![0xff]);
    client.db().save_notification_record(&record)?;
    peer.release.notify_one();
    let error = disable.join().await?.unwrap_err();
    assert_eq!(error.rate_limit_delay(), Some(delay));
}

/// A registration hold does not stop subscription updates while an existing
/// registration's renewal waits for the hold to end, and the turn after does
/// not spin.
// verifies: RATE-041
#[xmtp_common::test(unwrap_try = true)]
async fn notification_a_held_renewal_leaves_updates_running() {
    let (client, peer) = support::peer().await;
    let mut config = config();
    config.include_welcomes = false;
    client.enable_notifications(config).await?;
    run(&client.context).await?;
    let hold = time::now_ns() + 600 * xmtp_common::NS_IN_SEC;
    let mut record = client.db().notification_record()?;
    let mut held = deadlines(&record)?;
    held.renewal_ns = time::now_ns() - 1;
    held.registration_rate_limited_until_ns = hold;
    record.push_deadlines = Some(encode(&held)?);
    client.db().save_notification_record(&record)?;
    client.create_group(None, None)?;
    let (registers, updates) = (peer.calls(Call::Register), peer.calls(Call::Update));
    run(&client.context).await?;
    assert_eq!(peer.calls(Call::Update), updates + 1, "updates go on");
    assert_eq!(
        peer.calls(Call::Register),
        registers,
        "no renewal inside the hold"
    );
    // With nothing left to send and the renewal still held, the task waits
    // for the hold instead of running again at once.
    let mut record = client.db().notification_record()?;
    let mut held = deadlines(&record)?;
    held.renewal_ns = time::now_ns() - 1;
    record.push_deadlines = Some(encode(&held)?);
    client.db().save_notification_record(&record)?;
    let TaskOutcome::RescheduleAt(at) = run(&client.context).await? else {
        panic!("the task waits");
    };
    assert!(at > time::now_ns(), "the next turn is not due at once");
    assert!(at <= hold, "the renewal runs when the hold ends");
    // The same holds inside an update hold.
    let mut record = client.db().notification_record()?;
    let mut held = deadlines(&record)?;
    held.rate_limited_until_ns = time::now_ns() + 300 * xmtp_common::NS_IN_SEC;
    record.push_deadlines = Some(encode(&held)?);
    client.db().save_notification_record(&record)?;
    let TaskOutcome::RescheduleAt(at) = run(&client.context).await? else {
        panic!("the task waits");
    };
    assert!(at > time::now_ns(), "the next turn is not due at once");
    assert!(at <= held.rate_limited_until_ns);
}

/// A registration hold recorded after the task read its record, as an inline
/// enable records one, still holds the task's Register.
// verifies: RATE-041
#[xmtp_common::test(unwrap_try = true)]
async fn notification_a_hold_recorded_before_the_lock_holds_the_register() {
    let (client, peer) = support::peer().await;
    client.enable_notifications(config()).await?;
    let mut record = client.db().notification_record()?;
    let mut due = deadlines(&record)?;
    due.renewal_ns = time::now_ns() - 1;
    record.push_deadlines = Some(encode(&due)?);
    client.db().save_notification_record(&record)?;
    let hold = time::now_ns() + 600 * xmtp_common::NS_IN_SEC;
    let holding = client.clone();
    let _hook = support::on_before_request(move || {
        let mut record = holding.db().notification_record().unwrap();
        let mut held = deadlines(&record).unwrap();
        held.registration_rate_limited_until_ns = hold;
        record.push_deadlines = Some(encode(&held).unwrap());
        holding.db().save_notification_record(&record).unwrap();
    });
    let registers = peer.calls(Call::Register);
    run(&client.context).await?;
    run(&client.context).await?;
    assert_eq!(
        peer.calls(Call::Register),
        registers,
        "no Register inside the hold"
    );
}

/// A Register hold that cannot be stored does not replace the refusal: the
/// app still sees the rate limit and its delay.
// verifies: RATE-041
#[xmtp_common::test(unwrap_try = true)]
async fn notification_a_failed_registration_hold_write_keeps_the_rate_limit() {
    let (client, peer) = support::peer().await;
    let delay = Duration::from_secs(600);
    {
        let mut state = peer.state.lock();
        state.rate_limit = Some((Call::Register, delay));
        state.pause_next = true;
    }
    let enabling = client.clone();
    let enable = xmtp_common::spawn(None, async move {
        enabling.enable_notifications(config()).await
    });
    timeout(Duration::from_secs(5), peer.entered.notified()).await?;
    let mut record = client.db().notification_record()?;
    record.push_deadlines = Some(vec![0xff]);
    client.db().save_notification_record(&record)?;
    peer.release.notify_one();
    let error = enable.join().await?.unwrap_err();
    assert_eq!(error.rate_limit_delay(), Some(delay));
}

/// An update hold does not postpone a registration renewal that falls due
/// inside it: the task wakes for the renewal.
// verifies: RATE-041
#[xmtp_common::test(unwrap_try = true)]
async fn notification_an_update_hold_keeps_the_renewal_due() {
    let (client, peer) = support::peer().await;
    let mut config = config();
    config.include_welcomes = false;
    client.enable_notifications(config).await?;
    run(&client.context).await?;
    client.create_group(None, None)?;
    peer.state.lock().rate_limit = Some((Call::Update, Duration::from_secs(600)));
    assert!(run(&client.context).await.is_err());
    let renewal = time::now_ns() + 10 * xmtp_common::NS_IN_SEC;
    let mut record = client.db().notification_record()?;
    let mut held = deadlines(&record)?;
    held.renewal_ns = renewal;
    record.push_deadlines = Some(encode(&held)?);
    client.db().save_notification_record(&record)?;
    let TaskOutcome::RescheduleAt(at) = run(&client.context).await? else {
        panic!("the task waits");
    };
    assert!(
        at <= renewal,
        "the renewal is not postponed by the update hold"
    );
}

/// A refused owed Unregister holds registration only: once an enable has
/// registered, the task still sends subscription updates.
// verifies: RATE-041
#[xmtp_common::test(unwrap_try = true)]
async fn notification_a_registration_hold_leaves_updates_running() {
    let (client, peer) = support::peer().await;
    client.enable_notifications(config()).await?;
    run(&client.context).await?;
    peer.state.lock().rate_limit = Some((Call::Unregister, Duration::from_secs(600)));
    assert!(client.disable_notifications().await.is_err());
    client.enable_notifications(config()).await?;
    client.create_group(None, None)?;
    wake(&client.context)?;
    let updates = peer.calls(Call::Update);
    run(&client.context).await?;
    assert_eq!(peer.calls(Call::Update), updates + 1);
}

/// An update hold ends with its delay: the next turn after it sends.
// verifies: RATE-041
#[xmtp_common::test(unwrap_try = true)]
async fn notification_an_update_hold_ends_with_its_delay() {
    let (client, peer) = support::peer().await;
    let mut config = config();
    config.include_welcomes = false;
    client.enable_notifications(config).await?;
    run(&client.context).await?;
    client.create_group(None, None)?;
    peer.state.lock().rate_limit = Some((Call::Update, Duration::from_secs(600)));
    assert!(run(&client.context).await.is_err());
    let updates = peer.calls(Call::Update);
    assert!(matches!(
        run(&client.context).await?,
        TaskOutcome::RescheduleAt(_)
    ));
    assert_eq!(peer.calls(Call::Update), updates, "held");
    // The delay ends.
    let mut record = client.db().notification_record()?;
    let mut held = deadlines(&record)?;
    held.rate_limited_until_ns = time::now_ns() - 1;
    record.push_deadlines = Some(encode(&held)?);
    client.db().save_notification_record(&record)?;
    run(&client.context).await?;
    assert_eq!(peer.calls(Call::Update), updates + 1);
}

/// Enabling again, or disabling and enabling, does not end a pending delay:
/// the task still sends no subscription update inside it.
// verifies: RATE-041
#[xmtp_common::test(unwrap_try = true)]
async fn notification_reenable_keeps_a_pending_rate_limit_delay() {
    let delay = Duration::from_secs(600);
    let (client, peer) = support::peer().await;
    let mut config = config();
    config.include_welcomes = false;
    client.enable_notifications(config.clone()).await?;
    run(&client.context).await?;
    client.create_group(None, None)?;
    peer.state.lock().rate_limit = Some((Call::Update, delay));
    let before = time::now_ns();
    assert!(run(&client.context).await.is_err());
    let updates = peer.calls(Call::Update);

    client.enable_notifications(config.clone()).await?;
    wake(&client.context)?;
    let TaskOutcome::RescheduleAt(next) = run(&client.context).await? else {
        panic!("enabled task must remain");
    };
    assert_eq!(peer.calls(Call::Update), updates);
    assert!(next >= before + delay.as_nanos() as i64);

    client.disable_notifications().await?;
    client.enable_notifications(config).await?;
    wake(&client.context)?;
    let TaskOutcome::RescheduleAt(next) = run(&client.context).await? else {
        panic!("enabled task must remain");
    };
    assert_eq!(peer.calls(Call::Update), updates);
    assert!(next >= before + delay.as_nanos() as i64);
}

/// The owed reset and the registration share one 30-second deadline. A
/// registration refused with a delay that ends past it returns the rate limit
/// at once, instead of waiting until the turn times out and losing the delay.
// verifies: RATE-041, PUSH-267
#[xmtp_common::test(unwrap_try = true)]
async fn notification_registration_after_a_slow_reset_keeps_its_rate_limit() {
    let delay = Duration::from_secs(28);
    let (client, peer) = support::peer().await;
    client.enable_notifications(config()).await?;
    peer.state.lock().next_error = Some(tonic::Code::Unavailable);
    assert!(client.disable_notifications().await.is_err());
    assert!(client.db().notification_record()?.push_unregister_owed);
    {
        let mut state = peer.state.lock();
        state.slow = Some((Call::Unregister, Duration::from_secs(3)));
        state.rate_limit = Some((Call::Register, delay));
    }
    let started = xmtp_common::time::Instant::now();
    let error = client.enable_notifications(config()).await.unwrap_err();
    assert!(started.elapsed() < Duration::from_secs(20));
    assert!(matches!(error, NotificationError::Api(_)));
    assert_eq!(error.rate_limit_delay(), Some(delay));
    let record = client.db().notification_record()?;
    assert!(deadlines(&record)?.registration_rate_limited_until_ns > time::now_ns());
}
