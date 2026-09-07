mod reads;

use super::*;

fn request(topic: Topic, cursor: Option<u64>) -> api::TopicQuery {
    api::TopicQuery {
        topic: Some(api::Topic {
            topic: topic.to_vec(),
        }),
        cursor: cursor.map(|sequence_id| api::Cursor { sequence_id }),
    }
}

#[xmtp_common::test(unwrap_try = true)]
fn duplicate_query_inputs_become_one_database_cursor_at_the_lowest_position() {
    let first = TopicKind::WelcomeMessagesV1.create([1; 32]);
    let second = TopicKind::WelcomeMessagesV1.create([2; 32]);
    let queries = coalesce_queries(
        vec![
            request(first.clone(), Some(20)),
            request(second.clone(), Some(30)),
            request(first.clone(), Some(10)),
            request(second.clone(), None),
        ],
        4,
    )?;
    assert_eq!(queries.len(), 2);
    let actual: HashMap<_, _> = queries
        .into_iter()
        .map(|query| (query.topic, query.cursor))
        .collect();
    assert_eq!(
        actual,
        HashMap::from([(first.to_vec(), 10), (second.to_vec(), 0)])
    );
}

#[xmtp_common::test(unwrap_try = true)]
fn duplicate_inputs_do_not_bypass_cursor_validation_or_original_count_limits() {
    let topic = TopicKind::WelcomeMessagesV1.create([1; 32]);
    let invalid_cursor = coalesce_queries(
        vec![
            request(topic.clone(), None),
            request(topic.clone(), Some(u64::MAX)),
        ],
        2,
    );
    assert!(matches!(
        invalid_cursor,
        Err(error) if error.code() == tonic::Code::InvalidArgument
    ));
    let excess = coalesce_queries(vec![request(topic.clone(), None), request(topic, None)], 1);
    assert!(matches!(excess, Err(error) if error.code() == tonic::Code::InvalidArgument));
}
