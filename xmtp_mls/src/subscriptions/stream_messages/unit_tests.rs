use std::task::Poll;

use futures::stream::StreamExt;

use crate::subscriptions::stream_messages::StreamGroupMessages;
use crate::{subscriptions::stream_messages::test_case_builder::*, test::mock::MockMlsGroup};
use futures::FutureExt;
use futures::Stream;
use rstest::*;

#[rstest]
#[case(vec![
        StreamSession::session(
            vec![1, 2, 3, 4],
            vec![
                MessageCase::found(10, 1, 15),
                MessageCase::not_found(15, 1, 20),
                MessageCase::found(20, 1, 25),
                MessageCase::found(25, 1, 30),
            ],
            vec![10, 20, 25]
        )
    ])]
#[case(vec![
        StreamSession::session(
            vec![1, 2, 3, 4],
            vec![
                MessageCase::found(10, 1, 15),
                MessageCase::not_found(15, 1, 20),
                MessageCase::not_found(20, 1, 25),
                MessageCase::found(25, 1, 30),
            ],
            vec![10, 25]
        )
    ])]
#[case::nothing_found(vec![
        StreamSession::session(
            vec![1, 2, 3, 4],
            vec![
                MessageCase::not_found(10, 1, 15),
                MessageCase::not_found(15, 1, 20),
                MessageCase::not_found(20, 1, 25),
                MessageCase::not_found(25, 1, 30),
            ],
            vec![]
        )
    ])]
#[case::first_is_found(vec![
        StreamSession::session(
            vec![1, 2, 3, 4],
            vec![
                MessageCase::found(10, 1, 15),
                MessageCase::not_found(15, 1, 20),
                MessageCase::not_found(20, 1, 25),
                MessageCase::not_found(25, 1, 30),

            ],
            vec![10]
        )
    ])]
#[case::out_of_order(vec![
        StreamSession::session(
            vec![1, 2, 3, 4],
            vec![
                MessageCase::found(15, 1, 20),
                MessageCase::retrieved(10, 1, true),
                MessageCase::not_found(20, 1, 25),
                MessageCase::found(25, 1, 30),
            ],
            vec![15, 10, 25]
        )
    ])]
#[case::out_of_order(vec![
        StreamSession::session(
            vec![1, 2, 3, 4],
            vec![
                MessageCase::found(25, 1, 30),
                MessageCase::retrieved(15, 1, true),
                MessageCase::retrieved(10, 1, true),
                MessageCase::retrieved(20, 1, false),
                MessageCase::found(9, 2, 25),
                MessageCase::found(31, 2, 25),
            ],
            vec![25, 15, 10, 9, 31]
        )
    ])]
#[xmtp_common::test]
async fn it_can_stream_messages(#[case] mut cases: Vec<StreamSession>) {
    let group_list = group_list_from_session(&cases);
    let mut sequence = StreamSequenceBuilder::default();
    for case in cases.iter().cloned() {
        sequence.session(case);
    }
    let (factory, finished) = sequence.finish();
    let mut stream = StreamGroupMessages::new_with_factory(&finished.context, group_list, factory)
        .await
        .unwrap();

    for session in cases.iter_mut() {
        session.expected.reverse();
        while !session.expected.is_empty() {
            let item = stream.next().await.unwrap().unwrap();
            assert_eq!(
                item.sequence_id,
                Some(session.expected.pop().unwrap() as i64)
            )
        }
    }

    // the stream should end
    #[allow(clippy::never_loop)]
    while stream.next().await.is_some() {
        panic!("nothing should be left on the stream");
    }
}

#[rstest]
#[case(vec![
        StreamSession::session(
            vec![1, 2, 3, 4],
            vec![
                MessageCase::found(1, 1, 0),
                MessageCase::found(2, 1, 2),
                MessageCase::found(3, 1, 3),
                MessageCase::found(4, 1, 5),
            ],
            vec![1, 2, 3, 4]
        ),
        StreamSession::session(
            vec![5],
            vec![
                MessageCase::found(5, 1, 0),
                MessageCase::found(6, 5, 7)
            ],
            vec![5, 6]
        )
    ])]
#[case(vec![
        StreamSession::session(
            vec![1, 2, 3, 4],
            vec![
                MessageCase::found(1, 1, 0),
                MessageCase::found(2, 1, 2),
                MessageCase::found(3, 1, 3),
                MessageCase::found(4, 1, 5),
            ],
            vec![1, 2, 3, 4]
        ),
        StreamSession::session(
            vec![5, 6, 7],
            vec![
                MessageCase::not_found(5, 1, 0),
                MessageCase::found(6, 1, 0),
                MessageCase::found(7, 5, 7)
            ],
            vec![6, 7]
        )
    ])]
//  #[case(vec![
//      StreamSession::session(
//          vec![1, 2, 3, 4],
//          vec![
//              MessageCase::found(1, 1, 2),
//              MessageCase::found(2, 1, 3),
//              MessageCase::found(3, 1, 4),
//              MessageCase::not_found(4, 1, 5),
//          ],
//          vec![1, 2, 3, 4]
//      ),
//      StreamSession::session(
//          vec![5, 6, 7],
//          vec![
//              MessageCase::not_found(5, 1, 6),
//              MessageCase::found(6, 1, 99),
//              MessageCase::found(7, 5, 7)
//          ],
//          vec![6, 7]
//      )
//  ])]
#[xmtp_common::test]
async fn test_adding_to_stream_works(#[case] cases: Vec<StreamSession>) {
    let group_list = group_list_from_session(&cases);
    let mut sequence = StreamSequenceBuilder::default();
    for case in cases.iter().cloned() {
        sequence.session(case);
    }
    let (factory, finished) = sequence.finish();
    let stream = StreamGroupMessages::new_with_factory(&finished.context, group_list, factory)
        .await
        .unwrap();
    let mut stream = std::pin::pin!(stream);

    let mut first = true;
    for mut session in cases {
        if !first {
            // if its not the first session, add the groups
            for group in session.groups {
                stream.as_mut().add(MockMlsGroup::new(
                    finished.context.clone(),
                    group_id(group.group_id).to_vec(),
                    None,
                    xmtp_common::time::now_ns(),
                ))
            }
        }
        first = false;
        session.expected.reverse();
        while !session.expected.is_empty() {
            let item = stream.next().await.unwrap().unwrap();
            let exp = session.expected.pop().unwrap();
            assert_eq!(item.sequence_id, Some(exp as i64));
        }
    }
}

#[rstest]
#[case(vec![
        StreamSession::session(
            vec![1, 2, 3, 4],
            vec![
                MessageCase::found(1, 1, 2),
                MessageCase::found(2, 1, 3),
                MessageCase::found(3, 1, 4),
                MessageCase::processing_for(4, 1, 5, 3),
            ],
            vec![1, 2, 3]
        ),
        StreamSession::session(
            vec![5],
            vec![
                MessageCase::found(5, 1, 0),
                MessageCase::found(6, 5, 7)
            ],
            vec![4, 5, 6]
        )
    ])]
#[case(vec![
    StreamSession::session(
        vec![1, 2, 3, 4],
        vec![
            MessageCase::found(1, 1, 2),
            MessageCase::found(2, 1, 3),
            MessageCase::found(3, 1, 4),
            MessageCase::processing_for(4, 1, 5, 3),
        ],
        vec![1, 2, 3]
    ),
    StreamSession::session(
        vec![5, 6, 7, 8],
        vec![
            MessageCase::found(5, 1, 99),
            MessageCase::processing_for(6, 8, 10, 3),
            MessageCase::found(9, 5, 10)
        ],
        vec![4, 5, 6, 9]
    )
])]
#[xmtp_common::test]
async fn it_can_add_to_stream_while_busy(#[case] mut cases: Vec<StreamSession>) {
    let group_list = group_list_from_session(&cases);
    let mut sequence = StreamSequenceBuilder::default();
    for case in cases.iter().cloned() {
        sequence.session(case);
    }
    let (factory, finished) = sequence.finish();
    let stream = StreamGroupMessages::new_with_factory(&finished.context, group_list, factory)
        .now_or_never()
        .unwrap()
        .unwrap();
    futures::pin_mut!(stream);

    let noop_waker = futures::task::noop_waker();
    let mut cx = std::task::Context::from_waker(&noop_waker);

    let mut first = true;
    let cases_length = cases.len();
    for (i, session) in cases.iter_mut().enumerate() {
        // if its not the first session, add the groups
        if !first {
            tracing::info!("new session!");
            let pending = stream.as_mut().poll_next(&mut cx);
            assert!(pending.is_pending());
            for group in &session.groups {
                stream.as_mut().add(MockMlsGroup::new(
                    finished.context.clone(),
                    group_id(group.group_id).to_vec(),
                    None,
                    xmtp_common::time::now_ns(),
                ))
            }
        }
        first = false;

        session.expected.reverse();
        tracing::info!("Expecting {:?} cursors", session.expected);
        while !session.expected.is_empty() {
            match stream.as_mut().poll_next(&mut cx) {
                Poll::Ready(Some(Ok(i))) => {
                    if let Some(e) = session.expected.pop() {
                        tracing::info!("got {}", e);
                        assert_eq!(i.sequence_id, Some(e as i64));
                    } else {
                        break;
                    }
                }
                Poll::Ready(None) => {
                    if cases_length >= i {
                        break;
                    } else {
                        panic!("stream should not finish");
                    }
                }
                Poll::Pending => {
                    tracing::trace!("session pending");
                    continue;
                }
                e => panic!("Unexpected {:?}", e),
            }
        }
    }
}
