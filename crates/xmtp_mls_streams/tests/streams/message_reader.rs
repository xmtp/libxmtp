#[cfg(not(target_arch = "wasm32"))]
mod preflight;

use xmtp_common::time::Duration;
use xmtp_mls::prelude::*;
use xmtp_mls::subscriptions::{
    local_delivery::{DeliveryScope, LocalDeliveryError, LocalDeliveryFilter},
    message_reader::*,
};
use xmtp_mls::tester;
use xmtp_mls_engine::groups::send_message_opts::SendMessageOpts;

// verifies: DMS-009
#[xmtp_common::test(unwrap_try = true)]
async fn one_dm_reader_receives_a_later_explicitly_joined_duplicate() {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let first = alix
        .find_or_create_dm(bo.inbox_id().to_string(), None)
        .await?;
    let mut reader = MessageReader::new(
        alix.context.clone(),
        DeliveryScope::Groups(vec![first.group_id]),
        LocalDeliveryFilter::default(),
        None,
    )?;
    let second = bo
        .find_or_create_dm(alix.inbox_id().to_string(), None)
        .await?;
    assert_ne!(first.group_id, second.group_id);
    // This operation owns Welcome interest only until it returns.
    alix.sync_welcomes().await?;
    let installed: Option<xmtp_db::group::StoredGroup> =
        xmtp_db::Fetch::fetch(&alix.context.db(), &second.group_id)?;
    assert!(installed.is_some());
    let message_id = second
        .send_message(b"later duplicate", SendMessageOpts::default())
        .await?;

    let observed_group = xmtp_common::time::timeout(Duration::from_secs(15), async {
        loop {
            let item = reader
                .next_delivery()
                .await?
                .expect("the reader must remain open");
            let matched = item.message.id == message_id;
            let group_id = item.message.group_id;
            item.acknowledgement.acknowledge()?;
            if matched {
                break Ok::<_, LocalDeliveryError>(group_id);
            }
        }
    })
    .await??;
    assert_eq!(observed_group, second.group_id);
}

// verifies: CONF-075
#[xmtp_common::test(unwrap_try = true)]
async fn blocked_connection_ends_a_pending_message_read_with_its_cause() {
    use xmtp_mls_server_config::server_configuration::BlockedConnection;
    tester!(alix, disable_workers);
    let mut reader = MessageReader::new(
        alix.context.clone(),
        DeliveryScope::All,
        LocalDeliveryFilter::default(),
        None,
    )?;
    let error = {
        let pending = reader.next_delivery();
        futures::pin_mut!(pending);
        assert!(futures::poll!(&mut pending).is_pending());
        alix.context.server_configuration().block_connection(
            BlockedConnection::ClientVersionTooOld {
                client: "1.0.0".into(),
                minimum: "9999.0.0".into(),
            },
        );
        alix.context.cancellation_token().cancel();
        pending
            .await
            .err()
            .expect("blocked connection must fail the read")
    };
    assert!(matches!(
        error,
        LocalDeliveryError::Blocked(BlockedConnection::ClientVersionTooOld { ref minimum, .. })
            if minimum == "9999.0.0"
    ));
    assert!(reader.next_delivery().await?.is_none());
}
