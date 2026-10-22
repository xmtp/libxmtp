//! The legacy-group fence in `XmtpDb::init`: after migrations, a database
//! that holds a live group whose id is 16 bytes fails to open. A Restored
//! placeholder with a 16-byte id is imported history and opens. Native and
//! wasm.

use xmtp_common::{ErrorCode, RetryableError};
use xmtp_proto::types::GroupId;

use crate::group::{GroupMembershipState, QueryGroup, StoredGroup};
use crate::{StorageError, Store, TestDb, XmtpDb, XmtpTestDb};

fn group(id: GroupId, membership_state: GroupMembershipState) -> StoredGroup {
    StoredGroup::builder()
        .id(id)
        .created_at_ns(1)
        .membership_state(membership_state)
        .added_by_inbox_id("adder")
        .build()
        .unwrap()
}

/// Every membership state other than Restored makes a 16-byte group live,
/// and each refuses the database with the typed error. The rows stay.
#[xmtp_common::test(unwrap_try = true)]
async fn refuses_a_database_with_a_live_legacy_group() {
    let legacy = GroupId::from([1u8; 16]);
    for state in [
        GroupMembershipState::Allowed,
        GroupMembershipState::Rejected,
        GroupMembershipState::Pending,
        GroupMembershipState::PendingRemove,
    ] {
        let database = TestDb::create_database(None).await;
        database.init()?;
        group(GroupId::from([2u8; 32]), GroupMembershipState::Allowed).store(&database.conn())?;
        group(legacy, state).store(&database.conn())?;

        let error = match database.init() {
            Err(error) => error,
            Ok(()) => panic!("a database with a {state:?} 16-byte group opened"),
        };
        assert!(
            matches!(error, StorageError::LegacyGroupDatabase),
            "{error}"
        );
        assert_eq!(error.error_code(), "StorageError::LegacyGroupDatabase");
        assert!(!error.is_retryable());
        assert_eq!(
            error.to_string(),
            "This database holds groups from before keyed group ids. Keep a backup and create a new client database."
        );
        assert_eq!(
            database
                .db()
                .find_group(&legacy)?
                .map(|g| g.membership_state),
            Some(state)
        );
        // A second open gives the same answer.
        assert!(matches!(
            database.init(),
            Err(StorageError::LegacyGroupDatabase)
        ));
    }
}

/// A database whose only 16-byte group is a Restored placeholder opens,
/// with its keyed groups.
#[xmtp_common::test(unwrap_try = true)]
async fn opens_a_database_whose_legacy_groups_are_restored() {
    let database = TestDb::create_database(None).await;
    database.init()?;
    group(GroupId::from([1u8; 16]), GroupMembershipState::Restored).store(&database.conn())?;
    for state in [
        GroupMembershipState::Allowed,
        GroupMembershipState::Pending,
        GroupMembershipState::Restored,
    ] {
        let mut id = [3u8; 32];
        id[0] = state as u8;
        group(GroupId::from(id), state).store(&database.conn())?;
    }
    database.init()?;
    database.init()?;
}
