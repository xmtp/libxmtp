use rstest::*;

use xmtp_proto::types::GroupId;
pub mod cases {

    use super::*;

    // creates groups 1, 2, 3, 4
    #[fixture]
    pub fn group_list() -> Vec<GroupId> {
        [0x01u8, 0x02, 0x03, 0x04]
            .iter()
            .map(|i| GroupId::from([*i; 16]))
            .collect::<Vec<GroupId>>()
    }
}
