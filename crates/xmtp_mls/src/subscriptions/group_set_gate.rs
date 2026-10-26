//! Skip a group rediscovery while the stored group set is unchanged.

/// The group-set generation (`QueryGroup::group_set_generation`) of an
/// operation's last successful discovery read.
///
/// Read the generation before the discovery query and record it only after
/// the query succeeds. A write that commits between the two reads then changes
/// the generation the next pass sees, so the next pass reads again. Reading the
/// generation after the query could cache an old set under a new generation.
#[derive(Debug, Default)]
pub(crate) struct GroupSetGate(Option<i64>);

impl GroupSetGate {
    /// Whether `current` differs from the recorded generation, or none is
    /// recorded, so the operation must read again.
    pub(crate) fn changed(&self, current: i64) -> bool {
        self.0 != Some(current)
    }

    /// Record the generation read before a discovery read that succeeded.
    pub(crate) fn record(&mut self, current: i64) {
        self.0 = Some(current);
    }
}
