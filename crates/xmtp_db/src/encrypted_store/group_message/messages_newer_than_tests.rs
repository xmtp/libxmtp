use super::tests::generate_message;
use super::*;
use crate::{Store, group::tests::generate_group, test_utils::with_connection};

#[rstest::rstest]
#[case(0, vec![5, 10, 20])]
#[case(5, vec![10, 20])]
#[case(10, vec![20])]
#[case(20, vec![])]
#[case(30, vec![])]
#[xmtp_common::test(unwrap_try = true)]
async fn messages_newer_than_scalar(#[case] floor: u64, #[case] expected: Vec<u64>) {
    with_connection(|conn| {
        let group = generate_group(None);
        group.store(conn).unwrap();
        for sequence in [5, 10, 20] {
            let mut message =
                generate_message(None, Some(&group.id), Some(sequence), None, None, None);
            message.sequence_id = sequence;
            message.store(conn).unwrap();
        }
        let mut found: Vec<_> = conn
            .messages_newer_than(&HashMap::from([(group.id.to_vec(), Cursor(floor))]))
            .unwrap()
            .into_iter()
            .map(|(id, cursor)| {
                assert_eq!(id, group.id);
                cursor.0
            })
            .collect();
        found.sort_unstable();
        assert_eq!(found, expected);
    });
}

#[xmtp_common::test(unwrap_try = true)]
fn messages_newer_than_keeps_group_positions_separate() {
    with_connection(|conn| {
        let mut cursors = HashMap::new();
        let mut expected = Vec::new();
        for index in 0..150 {
            let group = generate_group(None);
            group.store(conn).unwrap();
            let floor = index * 10 + 1;
            cursors.insert(group.id.to_vec(), Cursor(floor as u64));
            for sequence in [floor - 1, floor, floor + 1] {
                let mut message =
                    generate_message(None, Some(&group.id), Some(sequence), None, None, None);
                message.sequence_id = sequence;
                message.store(conn).unwrap();
            }
            expected.push((group.id, Cursor((floor + 1) as u64)));
        }
        let mut found = conn.messages_newer_than(&cursors).unwrap();
        found.sort_unstable();
        expected.sort_unstable();
        assert_eq!(found, expected);
        assert!(
            conn.messages_newer_than(&HashMap::new())
                .unwrap()
                .is_empty()
        );
        let empty = generate_group(None);
        empty.store(conn).unwrap();
        assert!(
            conn.messages_newer_than(&HashMap::from([(empty.id.to_vec(), Cursor(0))]))
                .unwrap()
                .is_empty()
        );
    });
}
