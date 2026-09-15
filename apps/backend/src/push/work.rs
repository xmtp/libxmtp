//! Retained deliveries, channel admission, and completed-first-attempt tracking.

use std::collections::{BTreeMap, VecDeque};
use xmtp_common::time::{Duration, Instant};

use super::channel::{Delivery, DeliveryConfig, MAX_RETRY_DELAY, Outcome, RETRY_DELAY};

pub(crate) const RETAINED_LIMIT: usize = 10_000;
pub(crate) const CHANNEL_PERMITS: usize = 256;

#[cfg(test)]
mod tests;

pub(crate) struct Attempt {
    pub delivery: Delivery,
    pub window: i64,
    pub count: i64,
    pub all_gone: bool,
    pub due: Instant,
}

struct Window {
    floor: i64,
    pending: usize,
    loaded: bool,
}

pub(crate) struct Work {
    windows: BTreeMap<i64, Window>,
    queue: VecDeque<Attempt>,
    active: [usize; 3],
    reserved: usize,
    pub read_position: i64,
}

pub(crate) enum Completion {
    Done(&'static str),
    Retry,
    Dead(DeliveryConfig),
}

impl Work {
    pub fn new(position: i64) -> Self {
        Self {
            windows: BTreeMap::new(),
            queue: VecDeque::new(),
            active: [0; 3],
            reserved: 0,
            read_position: position,
        }
    }

    pub fn room_for_page(&self) -> bool {
        self.queue.len() + self.reserved + super::window::PAGE_SIZE <= RETAINED_LIMIT
    }

    pub fn reserve_page(&mut self) {
        self.reserved += super::window::PAGE_SIZE;
    }

    pub fn release_page(&mut self) {
        self.reserved -= super::window::PAGE_SIZE;
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty() && self.active.iter().all(|count| *count == 0)
    }

    /// Page admission is atomic. An incomplete window retains its floor even
    /// when every delivery in its already loaded pages has finished.
    pub fn add_page(&mut self, first: i64, last: i64, full: bool, deliveries: Vec<Delivery>) {
        let window = self.windows.entry(first).or_insert(Window {
            floor: self.read_position,
            pending: 0,
            loaded: false,
        });
        window.pending += deliveries.len();
        self.queue
            .extend(deliveries.into_iter().map(|delivery| Attempt {
                delivery,
                window: first,
                count: 0,
                all_gone: true,
                due: Instant::now(),
            }));
        if !full {
            window.loaded = true;
            self.read_position = last;
        }
        self.retire_windows();
    }

    fn retire_windows(&mut self) {
        self.windows
            .retain(|_, window| !window.loaded || window.pending != 0);
    }

    pub fn low_watermark(&self) -> i64 {
        self.windows
            .first_key_value()
            .map_or(self.read_position, |(_, window)| window.floor)
    }

    /// Select any due channel with capacity. A queued HTTPS attempt cannot
    /// block an APNs or FCM attempt later in the queue.
    pub fn next(&mut self, now: Instant) -> Option<Attempt> {
        let index = self.queue.iter().position(|attempt| {
            attempt.due <= now
                && self.active[attempt.delivery.config.channel as usize - 1] < CHANNEL_PERMITS
        })?;
        let mut attempt = self.queue.remove(index)?;
        self.active[attempt.delivery.config.channel as usize - 1] += 1;
        attempt.count += 1;
        Some(attempt)
    }

    pub fn next_delay(&self, now: Instant) -> Duration {
        self.queue
            .iter()
            .filter(|attempt| {
                self.active[attempt.delivery.config.channel as usize - 1] < CHANNEL_PERMITS
            })
            .map(|attempt| attempt.due.saturating_duration_since(now))
            .min()
            .unwrap_or(super::channel::ATTEMPT_TIMEOUT)
    }

    fn first_completed(&mut self, attempt: &Attempt) {
        if attempt.count == 1 {
            if let Some(window) = self.windows.get_mut(&attempt.window) {
                window.pending -= 1;
            }
            self.retire_windows();
        }
    }

    /// Release the channel before queuing a retry. Only the first completed
    /// attempt changes the durable low-water mark; retries are disposable.
    pub fn complete(
        &mut self,
        mut attempt: Attempt,
        outcome: Outcome,
        max_attempts: i64,
    ) -> Completion {
        self.active[attempt.delivery.config.channel as usize - 1] -= 1;
        self.first_completed(&attempt);
        attempt.all_gone &= outcome == Outcome::GoneTransient;
        let delay = match outcome {
            Outcome::Delivered => return Completion::Done("delivered"),
            Outcome::Rejected => return Completion::Done("rejected"),
            Outcome::Mismatch => return Completion::Done("mismatch"),
            Outcome::Terminal => return Completion::Dead(attempt.delivery.config),
            Outcome::GoneTransient if attempt.count >= max_attempts && attempt.all_gone => {
                return Completion::Dead(attempt.delivery.config);
            }
            Outcome::GoneTransient => RETRY_DELAY,
            Outcome::Transient { retry_after } => retry_after
                .unwrap_or(RETRY_DELAY)
                .clamp(RETRY_DELAY, MAX_RETRY_DELAY),
        };
        if attempt.count >= max_attempts || self.queue.len() + self.reserved >= RETAINED_LIMIT {
            return Completion::Done("failed");
        }
        attempt.due = Instant::now() + delay;
        self.queue.push_back(attempt);
        Completion::Retry
    }

    /// Discard queued work only for the deleted configuration. A concurrent
    /// registration with different delivery fields keeps its pending work.
    pub fn deleted(&mut self, config: &DeliveryConfig) {
        let mut kept = VecDeque::new();
        while let Some(mut attempt) = self.queue.pop_front() {
            if &attempt.delivery.config == config {
                if attempt.count == 0 {
                    attempt.count = 1;
                    self.first_completed(&attempt);
                }
            } else {
                kept.push_back(attempt);
            }
        }
        self.queue = kept;
    }
}
