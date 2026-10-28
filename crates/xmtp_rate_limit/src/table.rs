//! The fixed store: an 8-way set-associative table of caller states.
//!
//! A slot holds a keyed 64-bit hash of (category, caller key) and that key's
//! TATs. One multiply-shift of the hash's high bits picks its set among all
//! sets. Sets sit in contiguous ranges, one range per shard lock, and the whole
//! check and charge of a spend runs under one shard lock. The table is
//! allocated once and never grows, rehashes, or sweeps: a slot whose TATs are
//! all past is at full allowance, the same as no slot, so it is reused in
//! place. When a set has no such slot, the slot with the smallest largest TAT,
//! the caller that has spent least, is forgotten.

use std::hash::Hasher;
use std::sync::atomic::{AtomicU64, Ordering};

use siphasher::sip::SipHasher13;

use crate::gcra::{self, FULL_ALLOWANCE, Tats};
use crate::{Decision, Limit, MAX_LIMITS, Spend};

/// Slots per set.
const WAYS: usize = 8;

/// The most shards a table has; [`layout`] says when it has fewer.
const MAX_SHARDS: usize = 1024;

/// The shard locks and the rounding down to whole sets together take at most
/// this fraction of a budget, 1/20, so the slots fill at least 95% of it.
const OVERHEAD_DIVISOR: usize = 20;

/// One caller key's state in one category.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub(crate) struct Slot {
    /// The keyed hash of the category and caller key, or 0 for a free slot.
    hash: u64,
    tats: Tats,
}

const _: () = assert!(size_of::<Slot>() == 32);

impl Slot {
    const FREE: Self = Self {
        hash: 0,
        tats: FULL_ALLOWANCE,
    };

    /// When every spend recorded here is earned back; 0 for a free slot.
    fn latest(&self) -> u64 {
        self.tats.iter().copied().max().unwrap_or(0)
    }
}

/// The slots one hash can occupy, aligned to whole cache lines.
#[derive(Debug, Clone, Copy)]
#[repr(C, align(64))]
pub(crate) struct Set([Slot; WAYS]);

const _: () = assert!(size_of::<Set>() == WAYS * size_of::<Slot>());

impl Set {
    const EMPTY: Self = Self([Slot::FREE; WAYS]);

    /// Decides and charges one spend for the caller whose slot carries `hash`.
    ///
    /// Returns the decision, as [`gcra::decide`] gives it, and whether
    /// recording the caller forgot a slot that was not at full allowance.
    fn spend(
        &mut self,
        hash: u64,
        now: u64,
        limits: &[Limit],
        cost: u32,
    ) -> (Result<(), u64>, bool) {
        if let Some(slot) = self.0.iter_mut().find(|slot| slot.hash == hash) {
            let outcome = gcra::decide(&slot.tats, limits, cost, now).map(|tats| slot.tats = tats);
            return (outcome, false);
        }
        // A caller with no slot is at full allowance.
        let tats = match gcra::decide(&FULL_ALLOWANCE, limits, cost, now) {
            Ok(tats) => tats,
            Err(wait) => return (Err(wait), false),
        };
        if tats.iter().all(|&tat| tat <= now) {
            // Still at full allowance: nothing to remember.
            return (Ok(()), false);
        }
        // Free slots (0) and slots at full allowance (at most `now`) sort
        // before every slot with spend left to earn back. Ties go to the
        // first way.
        let victim = (0..WAYS)
            .min_by_key(|&way| self.0[way].latest())
            .unwrap_or(0);
        let slot = &mut self.0[victim];
        let evicted = slot.hash != 0 && slot.latest() > now;
        *slot = Slot { hash, tats };
        (Ok(()), evicted)
    }
}

/// A lock around one shard's sets. The table is generic over it so that the
/// interleaving tests can run it under a model checker's mutex.
pub(crate) trait ShardLock: Send + Sync {
    fn new(sets: Box<[Set]>) -> Self;

    /// Runs `f` on the shard's sets while holding the lock.
    fn with<R>(&self, f: impl FnOnce(&mut [Set]) -> R) -> R;
}

/// A spend allocates nothing of its own. `parking_lot` does, once per thread:
/// the first time a thread waits for a contended lock it allocates the
/// thread's wait state, and its global wait table grows with the number of
/// threads.
impl ShardLock for parking_lot::Mutex<Box<[Set]>> {
    fn new(sets: Box<[Set]>) -> Self {
        parking_lot::Mutex::new(sets)
    }

    fn with<R>(&self, f: impl FnOnce(&mut [Set]) -> R) -> R {
        f(&mut self.lock())
    }
}

/// How many shards and sets a budget of `budget` bytes buys, when one shard
/// costs `shard_bytes` besides its sets.
///
/// The shards take at most a 20th of the budget less one set, and the set count
/// need not be a power of two, so the slots fill at least 95% of any budget
/// from about 5.6 KB up. That leaves room for [`MAX_SHARDS`] shards from about
/// 485 KiB up, which covers every budget the backend configuration accepts; a
/// smaller budget gets fewer shards. Every shard holds at least one set. A
/// budget too small for one set and its shard still gets one, because a table
/// with no set could record no spend.
pub(crate) fn layout(budget: usize, shard_bytes: usize) -> (usize, usize) {
    let set_bytes = size_of::<Set>();
    let shard_budget = (budget / OVERHEAD_DIVISOR).saturating_sub(set_bytes);
    let shards = (shard_budget / shard_bytes.max(1)).clamp(1, MAX_SHARDS);
    let sets = (budget.saturating_sub(shards * shard_bytes) / set_bytes).max(shards);
    (shards, sets)
}

/// The fixed store.
pub(crate) struct Table<L> {
    /// The 128-bit SipHash key, drawn when the table is made.
    keys: (u64, u64),
    /// Contiguous ranges of sets, one per lock: the first `long_shards` hold
    /// `per_shard + 1` sets, the rest `per_shard`. The locks are not padded to
    /// a cache line: at the smallest budget, 1 MiB, padding 1024 of them would
    /// take 6% of it from the slots.
    shards: Box<[L]>,
    per_shard: usize,
    long_shards: usize,
    sets: usize,
    evictions: AtomicU64,
}

impl<L: ShardLock> Table<L> {
    /// The bytes one shard costs besides its sets.
    pub(crate) const SHARD_BYTES: usize = size_of::<L>();

    /// Allocates a table that fits `budget` bytes. Spends allocate nothing of
    /// their own; see the `parking_lot` lock for its one exception.
    // implements: RATE-050
    pub(crate) fn new(budget: usize) -> Self {
        let (shards, sets) = layout(budget, Self::SHARD_BYTES);
        Self::with_layout(shards, sets)
    }

    /// Allocates `sets` sets spread over `shards` contiguous shards.
    pub(crate) fn with_layout(shards: usize, sets: usize) -> Self {
        let shards = shards.max(1);
        let sets = sets.max(shards);
        let (per_shard, long_shards) = (sets / shards, sets % shards);
        let shards: Box<[L]> = (0..shards)
            .map(|shard| {
                let len = per_shard + usize::from(shard < long_shards);
                L::new(vec![Set::EMPTY; len].into_boxed_slice())
            })
            .collect();
        Self {
            keys: hash_keys(),
            shards,
            per_shard,
            long_shards,
            sets,
            evictions: AtomicU64::new(0),
        }
    }

    /// Decides one spend at `now`, nanoseconds since the limiter's start.
    pub(crate) fn spend(&self, now: u64, spend: &Spend<'_>) -> Decision {
        debug_assert!(
            spend.limits.len() <= MAX_LIMITS,
            "a category carries at most {MAX_LIMITS} limits"
        );
        let limits = &spend.limits[..spend.limits.len().min(MAX_LIMITS)];
        debug_assert!(
            limits.iter().all(|limit| !limit.window.is_zero()),
            "a limit's window is at least 1 ns"
        );
        if spend.cost == 0 || limits.is_empty() {
            return Decision::Allowed;
        }
        let hash = self.hash(spend.category, spend.key);
        let (shard, set) = self.locate(hash);
        let (outcome, evicted) =
            self.shards[shard].with(|sets| sets[set].spend(hash, now, limits, spend.cost));
        if evicted {
            self.evictions.fetch_add(1, Ordering::Relaxed);
        }
        match outcome {
            Ok(()) => Decision::Allowed,
            Err(wait) => Decision::Denied {
                retry_after: gcra::retry_after(wait),
            },
        }
    }

    pub(crate) fn slots(&self) -> usize {
        self.sets * WAYS
    }

    /// Slots forgotten while their caller was not at full allowance.
    pub(crate) fn evictions(&self) -> u64 {
        self.evictions.load(Ordering::Relaxed)
    }

    /// SipHash-1-3 of the category byte and the key bytes. The category has a
    /// fixed width, so no two (category, key) pairs feed the same bytes. A hash
    /// of 0 marks a free slot, so 0 becomes 1.
    fn hash(&self, category: u8, key: &[u8]) -> u64 {
        let mut hasher = SipHasher13::new_with_keys(self.keys.0, self.keys.1);
        hasher.write_u8(category);
        hasher.write(key);
        hasher.finish().max(1)
    }

    /// The shard a hash falls in and its set within the shard.
    ///
    /// One multiply-shift of the hash's high bits picks the set among all
    /// sets, so every set takes an equal share of hashes whatever the shard
    /// lengths; the shard is the contiguous range that holds that set.
    fn locate(&self, hash: u64) -> (usize, usize) {
        let set = ((u128::from(hash) * self.sets as u128) >> 64) as usize;
        let long = self.per_shard + 1;
        let long_sets = self.long_shards * long;
        if set < long_sets {
            (set / long, set % long)
        } else {
            let set = set - long_sets;
            (
                self.long_shards + set / self.per_shard,
                set % self.per_shard,
            )
        }
    }

    /// The TATs the table holds for a caller key in a category.
    #[cfg(test)]
    pub(crate) fn tats(&self, category: u8, key: &[u8]) -> Option<Tats> {
        let hash = self.hash(category, key);
        let (shard, set) = self.locate(hash);
        self.shards[shard].with(|sets| {
            sets[set]
                .0
                .iter()
                .find(|slot| slot.hash == hash)
                .map(|slot| slot.tats)
        })
    }
}

/// A 128-bit SipHash key from the operating system's random source, so a
/// caller cannot choose a key that shares a victim's slot or set.
///
/// # Panics
///
/// When the operating system has no random source, at construction.
fn hash_keys() -> (u64, u64) {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).expect("the operating system's random source failed");
    let key = u128::from_le_bytes(bytes);
    (key as u64, (key >> 64) as u64)
}

#[cfg(test)]
mod tests;
