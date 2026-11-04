//! Counts of the discovery work conversation streams do on this thread.

use std::cell::Cell;

std::thread_local! {
    /// Discovery passes of conversation streams polled on this thread.
    pub(super) static PASSES: Cell<usize> = const { Cell::new(0) };
    /// Groups-table reads by those passes.
    pub(super) static GROUPS_READS: Cell<usize> = const { Cell::new(0) };
}

pub(super) fn count_pass() {
    PASSES.with(|passes| passes.set(passes.get() + 1));
}

pub(super) fn count_groups_read() {
    GROUPS_READS.with(|reads| reads.set(reads.get() + 1));
}
