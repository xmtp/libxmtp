//! Local send timestamps; never used for deadlines or backend confirmation.

use parking_lot::Mutex;

#[derive(Default)]
pub(crate) struct MessageClock {
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    last: Option<i64>,
    #[cfg(test)]
    frozen_wall_ns: Option<i64>,
}

impl MessageClock {
    pub(crate) fn now_ns(&self) -> i64 {
        let mut state = self.state.lock();
        let wall_ns = xmtp_common::time::now_ns();
        #[cfg(test)]
        let wall_ns = state.frozen_wall_ns.unwrap_or(wall_ns);
        // Date.now() can repeat or move backward. Keep acceptance order without
        // changing the clock used by deadlines or the backend timestamp.
        let timestamp = state
            .last
            .map_or(wall_ns, |last| wall_ns.max(last.saturating_add(1)));
        state.last = Some(timestamp);
        timestamp
    }

    #[cfg(test)]
    pub(crate) fn freeze(&self, wall_ns: i64) {
        self.state.lock().frozen_wall_ns = Some(wall_ns);
    }

    #[cfg(all(test, not(target_arch = "wasm32")))]
    pub(crate) fn is_frozen(&self) -> bool {
        self.state.lock().frozen_wall_ns.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[xmtp_common::test(unwrap_try = true)]
    fn message_clock_handles_repeated_and_backward_wall_time() {
        let clock = MessageClock::default();
        clock.freeze(1_000_000);
        assert_eq!(clock.now_ns(), 1_000_000);
        assert_eq!(clock.now_ns(), 1_000_001);
        clock.freeze(0);
        assert_eq!(clock.now_ns(), 1_000_002);
        clock.freeze(2_000_000);
        assert_eq!(clock.now_ns(), 2_000_000);
    }
}
