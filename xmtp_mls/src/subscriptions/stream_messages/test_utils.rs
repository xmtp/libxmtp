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
}
