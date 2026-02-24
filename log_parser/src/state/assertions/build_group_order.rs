use crate::state::{State, assertions::LogAssertion};
use anyhow::Result;
use std::collections::BTreeMap;

pub struct BuildGroupOrder;

impl LogAssertion for BuildGroupOrder {
    fn assert(state: &State) -> Result<()> {
        let timeline = state.timeline.lock();
        let mut order = BTreeMap::new();

        for (group_id, events) in &*timeline {
            let Some(event) = events.last() else {
                continue;
            };

            order.insert(event.time(), group_id.clone());
        }

        *state.group_order.lock() = order;

        Ok(())
    }
}
