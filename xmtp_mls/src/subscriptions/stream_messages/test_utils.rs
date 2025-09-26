use rstest::*;

pub mod cases {
    use xmtp_proto::types::GroupId;

    use super::*;

    // creates groups 1, 2, 3, 4
    #[fixture]
    pub fn group_list() -> Vec<GroupId> {
        vec![vec![1], vec![2], vec![3], vec![4]]
            .into_iter()
            .map(|mut i| {
                i.resize(31, 0);
                GroupId::from(i)
            })
            .collect::<Vec<GroupId>>()
    }
    /*
    #[fixture]
    pub fn msg_not_found() -> Vec<MessageCase> {
        vec![
            MessageCase {
                found: false,
                next_message: 10,
            },
            MessageCase {
                found: true,
                next_message: 20,
            },
            MessageCase {
                found: true,
                next_message: 25,
            },
            MessageCase {
                found: true,
                next_message: 999,
            },
            MessageCase {
                found: true,
                next_message: 30,
            },
            MessageCase {
                found: true,
                next_message: 999,
            },
        ]
    }
    */
}
