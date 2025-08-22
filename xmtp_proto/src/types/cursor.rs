#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Cursor {
    pub sequence_id: u64,
    pub originator_id: u32,
}
