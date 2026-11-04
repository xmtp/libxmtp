use std::sync::Arc;

use xmtp_logging::{Level, LogRecord, LogSinkTarget, SinkError, test_logging::LogCapture};
use xmtp_mls_validation::{
    commit::{CommitParticipant, Inbox, MembershipValidationInfo},
    group_permissions::{MembershipPolicies, PolicySet},
};

#[derive(Default)]
struct Capture(parking_lot::Mutex<Vec<LogRecord>>);

impl LogSinkTarget for Capture {
    fn on_record(&self, record: LogRecord) -> Result<(), SinkError> {
        self.0.lock().push(record);
        Ok(())
    }
}

#[rstest::rstest]
#[case::add_committer(false, false)]
#[case::add_proposer(false, true)]
#[case::remove_committer(true, false)]
#[case::remove_proposer(true, true)]
#[xmtp_common::test(unwrap_try = true)]
fn membership_rejection_log_omits_full_ids(#[case] remove: bool, #[case] has_proposer: bool) {
    let actor_id = (32_u8..64).collect::<Vec<_>>();
    let proposer_id = (64_u8..96).collect::<Vec<_>>();
    let actor = CommitParticipant {
        inbox_id: "sentinel-actor-inbox".to_string(),
        installation_id: actor_id.clone(),
        is_creator: false,
        is_admin: true,
        is_super_admin: false,
    };
    let proposer = CommitParticipant {
        inbox_id: "sentinel-proposer-inbox".to_string(),
        installation_id: proposer_id.clone(),
        is_creator: true,
        is_admin: false,
        is_super_admin: true,
    };
    let changes = [Inbox {
        inbox_id: "sentinel-subject-inbox".to_string(),
        is_creator: false,
        is_admin: true,
        is_super_admin: false,
        proposer: has_proposer.then_some(proposer),
    }];
    let policies = PolicySet {
        add_member_policy: MembershipPolicies::deny(),
        remove_member_policy: MembershipPolicies::deny(),
        ..PolicySet::default()
    };
    let membership = MembershipValidationInfo {
        actor: &actor,
        added_inboxes: if remove { &[] } else { &changes },
        removed_inboxes: if remove { &changes } else { &[] },
        dm_members: None,
    };
    let sink = Arc::new(Capture::default());
    let capture = LogCapture::with_sink(Level::Info, Some(sink.clone()));
    let accepted = tracing::dispatcher::with_default(&capture.dispatch(), || {
        policies.evaluate_membership(&membership)
    });
    assert!(!accepted);
    let json = capture.output();
    let records = sink.0.lock();
    assert_eq!(records.len(), 1);
    for sensitive in [
        hex::encode(&actor_id),
        hex::encode(&proposer_id),
        format!("{actor_id:?}"),
        format!("{proposer_id:?}"),
        actor.inbox_id.clone(),
        "sentinel-proposer-inbox".to_string(),
        changes[0].inbox_id.clone(),
    ] {
        assert!(
            !json.contains(&sensitive),
            "sensitive value reached JSON log"
        );
        assert!(
            !records[0].message.contains(&sensitive),
            "sensitive value reached sink message"
        );
        assert!(
            records[0]
                .fields
                .values()
                .all(|value| !value.contains(&sensitive)),
            "sensitive value reached sink fields"
        );
    }
    let json_record: serde_json::Value = serde_json::from_str(json.trim()).unwrap();
    assert_eq!(json_record["message"], "Membership policy rejected change");
    assert_eq!(records[0].message, "Membership policy rejected change");
    for (field, value) in [
        ("has_proposer", has_proposer),
        ("actor_is_creator", has_proposer),
        ("actor_is_admin", !has_proposer),
        ("actor_is_super_admin", has_proposer),
        ("subject_is_creator", false),
        ("subject_is_admin", true),
        ("subject_is_super_admin", false),
    ] {
        assert_eq!(json_record[field], value);
        assert_eq!(records[0].fields.get(field), Some(&value.to_string()));
    }
    assert_eq!(actor.installation_id, actor_id);
    if let Some(proposer) = &changes[0].proposer {
        assert_eq!(proposer.installation_id, proposer_id);
    }
}
