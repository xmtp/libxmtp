use crate::state::{State, assertions::LogAssertion};
use anyhow::Result;
use std::collections::BTreeMap;

pub struct BuildGroupOrder;

impl LogAssertion for BuildGroupOrder {
    fn assert(state: &State) -> Result<()> {
        let mut order = BTreeMap::new();
        let groups = state.groups.lock();

        for (group_id, group) in &*groups {
            let group = group.lock();
            let Some(event) = group.timeline.last() else {
                continue;
            };

            order.insert(event.time(), group_id.clone());
        }

        *state.group_order.lock() = order;

        Ok(())
    }
}
