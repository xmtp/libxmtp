//! Consent records written before 8.0. Before 8.0, a DM's consent was its
//! group's record, and inbox records decided no DM. A pre-8.0 time is the time
//! a row was first inserted, not the time of its latest state, so it cannot
//! say which of a peer's rows is newer.

use xmtp_db::group::DmIdExt;
use xmtp_proto::xmtp::device_sync::consent_backup::{ConsentSave, ConsentTypeSave};

/// The one inbox record that stands for a peer's pre-8.0 DM rows and inbox
/// row. Its state is the strongest of them, denied over allowed over unknown,
/// and its time the greatest, so it replaces each of them.
// implements: CONS-061, MIG-009
pub fn pre8_peer_consent(peer: &str, rows: &[ConsentSave]) -> Option<ConsentSave> {
    let state = rows.iter().map(ConsentSave::state).max()?;
    let consented_at_ns = rows.iter().map(|row| row.consented_at_ns).max()?;
    Some(ConsentSave {
        entity_type: ConsentTypeSave::InboxId as i32,
        entity: peer.to_ascii_lowercase(),
        state: state as i32,
        consented_at_ns,
    })
}

/// The other member of a DM, when `own_inbox` is one of its two members.
pub fn dm_peer(dm_id: &str, own_inbox: &str) -> Option<String> {
    dm_id.to_string().other_inbox_id(own_inbox)
}

#[cfg(test)]
mod tests {
    use super::*;
    use xmtp_proto::xmtp::device_sync::consent_backup::ConsentStateSave::{
        self, Allowed, Denied, Unknown,
    };

    fn row(entity_type: ConsentTypeSave, state: ConsentStateSave, at: i64) -> ConsentSave {
        ConsentSave {
            entity_type: entity_type as i32,
            entity: "ignored".into(),
            state: state as i32,
            consented_at_ns: at,
        }
    }

    // verifies: CONS-061, MIG-009
    #[xmtp_common::test]
    fn pre8_peer_record_keeps_the_strongest_state_at_the_greatest_time() {
        use ConsentTypeSave::{ConversationId as Dm, InboxId as Inbox};
        let cases = [
            // A DM deny beats a later-created inbox allow.
            (
                vec![row(Dm, Denied, 10), row(Inbox, Allowed, 20)],
                Denied,
                20,
            ),
            // An inbox deny is kept over a DM allow.
            (
                vec![row(Dm, Allowed, 30), row(Inbox, Denied, 20)],
                Denied,
                30,
            ),
            // An explicit allow is not wiped by an unknown DM row.
            (
                vec![row(Dm, Unknown, 40), row(Inbox, Allowed, 5)],
                Allowed,
                40,
            ),
            (vec![row(Dm, Allowed, 1), row(Dm, Unknown, 2)], Allowed, 2),
            (vec![row(Inbox, Unknown, 0)], Unknown, 0),
        ];
        for (rows, state, at) in cases {
            for rows in [rows.clone(), rows.into_iter().rev().collect()] {
                let record = pre8_peer_consent("PEER", &rows).unwrap();
                assert_eq!(record.entity, "peer");
                assert_eq!(record.entity_type(), ConsentTypeSave::InboxId);
                assert_eq!(
                    (record.state(), record.consented_at_ns),
                    (state, at),
                    "{rows:?}"
                );
            }
        }
        assert!(pre8_peer_consent("peer", &[]).is_none());
        assert_eq!(dm_peer("dm:me:peer", "me").as_deref(), Some("peer"));
        assert_eq!(dm_peer("dm:a:b", "me"), None);
    }
}
