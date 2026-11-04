//! The leaves of `groups/tests/test_membership.rs` that need the client.

use crate::groups::GroupError;
use crate::prelude::*;
use crate::tester;
use xmtp_db::group::GroupQueryArgs;

// verifies: GMOD-056
#[xmtp_common::test(unwrap_try = true)]
async fn a_create_whose_add_cannot_resolve_stores_nothing() {
    use xmtp_db::consent_record::QueryConsentRecord;
    tester!(alix, disable_workers);
    let events = alix.context.events().subscribe(
        xmtp_events::EventFilter::new([xmtp_events::EventKind::ConversationJoined]),
        Some(10),
    );
    let missing_inbox = hex::encode(xmtp_common::rand_vec::<32>());
    let result = alix
        .create_group_with_members(&[missing_inbox], None, None)
        .await;
    assert!(
        matches!(result, Err(crate::client::ClientError::Group(error))
        if matches!(*error, GroupError::MissingSequenceId))
    );
    assert!(alix.find_groups(GroupQueryArgs::default())?.is_empty());
    assert!(alix.context.db().consent_records()?.is_empty());
    assert!(events.drain().is_empty());
}
