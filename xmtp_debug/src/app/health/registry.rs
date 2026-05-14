//! Generic topo-sort over self-registered entries.
//!
//! Both ops and validators share the same shape: a `*Entry` struct with a
//! name, a list of dependency names, and a `make` function pointer that
//! materializes a `Box<dyn Trait>`. This module factors out the sort so
//! each registry consumer just provides the entry → name/deps/make
//! projections via the `RegistryEntry` trait.

use std::collections::{BTreeMap, BTreeSet};

use crate::app::health::conditions::Conditions;

/// Projection trait. The `Kind` associated type lets the trait describe
/// itself in panic messages ("op", "validator").
pub trait RegistryEntry: 'static {
    /// Trait object materialized by `make`.
    type Item: ?Sized;

    /// Human label for panic messages, e.g. `"op"` or `"validator"`.
    const KIND: &'static str;

    fn name(&self) -> &'static str;
    fn depends_on(&self) -> &'static [&'static str];
    fn make(&self) -> Box<Self::Item>;

    /// Condition bits this entry needs to be runnable. Default
    /// `ALWAYS` (entry runs in any active set). Entries with non-empty
    /// `requires` are filtered out of `topo_sort` when the active set
    /// doesn't cover them, and recorded as `SkippedEntry` in the
    /// returned build.
    fn requires(&self) -> Conditions {
        Conditions::ALWAYS
    }
}

/// One entry that was filtered out of `topo_sort` because the active
/// condition set didn't cover its `requires`. Surfaced so the runner
/// can emit a `Status::Skipped` row.
pub struct SkippedEntry {
    pub name: &'static str,
    pub missing: Conditions,
}

/// Result of building a registry against an active condition set.
pub struct RegistryBuild<I: ?Sized> {
    /// Topologically-sorted runnable entries that passed the filter.
    pub items: Vec<Box<I>>,
    /// Entries filtered out, in alphabetical order by name.
    pub skipped: Vec<SkippedEntry>,
}

/// Collect entries by name, panicking on duplicate registrations.
fn index_by_name<E: RegistryEntry>(
    stream: impl IntoIterator<Item = &'static E>,
) -> BTreeMap<&'static str, &'static E> {
    let mut out: BTreeMap<&'static str, &'static E> = BTreeMap::new();
    for e in stream {
        let n = e.name();
        if out.insert(n, e).is_some() {
            panic!("duplicate {} name '{}'", E::KIND, n);
        }
    }
    out
}

/// Partition entries into runnable (filter passes) and skipped (filter
/// reports missing condition bits). Skipped entries come out in
/// alphabetical order — the input map is already a `BTreeMap`, and
/// `partition_map` preserves iteration order.
fn partition_by_conditions<E: RegistryEntry>(
    all: BTreeMap<&'static str, &'static E>,
    active: Conditions,
) -> (BTreeMap<&'static str, &'static E>, Vec<SkippedEntry>) {
    use itertools::{Either, Itertools};
    all.into_iter().partition_map(|(name, e)| {
        let missing = e.requires().missing_from(active);
        if missing.is_empty() {
            Either::Left((name, e))
        } else {
            Either::Right(SkippedEntry { name, missing })
        }
    })
}

/// Panic if any runnable entry depends on a name that either doesn't
/// exist or was filtered out as skipped.
fn validate_deps<E: RegistryEntry>(
    runnable: &BTreeMap<&'static str, &'static E>,
    skipped: &[SkippedEntry],
) {
    let bad = runnable
        .values()
        .flat_map(|e| e.depends_on().iter().map(move |d| (e, d)))
        .find(|(_, dep)| !runnable.contains_key(*dep));

    let Some((e, dep)) = bad else { return };
    if let Some(sk) = skipped.iter().find(|s| s.name == *dep) {
        panic!(
            "{} '{}' depends on {} '{}' which was skipped (missing {:?}). \
             Either gate '{}' on the same condition or drop the dependency.",
            E::KIND,
            e.name(),
            E::KIND,
            dep,
            sk.missing,
            e.name(),
        );
    }
    panic!(
        "{} '{}' depends on unknown {} '{}'",
        E::KIND,
        e.name(),
        E::KIND,
        dep,
    );
}

/// Run Kahn's algorithm over the validated runnable set. Returns the
/// topo-sorted name sequence; panics on cycle.
fn topo_order<E: RegistryEntry>(
    runnable: &BTreeMap<&'static str, &'static E>,
) -> Vec<&'static str> {
    let mut in_degree: BTreeMap<&'static str, usize> = runnable
        .iter()
        .map(|(n, e)| (*n, e.depends_on().len()))
        .collect();
    let mut ready: BTreeSet<&'static str> = in_degree
        .iter()
        .filter(|&(_, &d)| d == 0)
        .map(|(n, _)| *n)
        .collect();
    let mut order = Vec::with_capacity(runnable.len());
    while let Some(&name) = ready.iter().next() {
        ready.remove(name);
        order.push(name);
        for e in runnable.values() {
            if e.depends_on().contains(&name) {
                let d = in_degree.get_mut(e.name()).unwrap();
                *d -= 1;
                if *d == 0 {
                    ready.insert(e.name());
                }
            }
        }
    }
    if order.len() != runnable.len() {
        let remaining: Vec<&str> = in_degree
            .iter()
            .filter(|&(_, &d)| d > 0)
            .map(|(n, _)| *n)
            .collect();
        panic!("{} dependency cycle among: {remaining:?}", E::KIND);
    }
    order
}

/// Topologically sort a stream of entries by their `depends_on` edges,
/// filtering by the active condition set.
///
/// Ties (entries with the same satisfied-dep set) are broken
/// alphabetically by `name()` for deterministic output across runs.
/// Panics on dependency cycles, unknown `depends_on` references,
/// duplicate names, or a runnable entry that depends on a skipped
/// entry.
pub fn topo_sort<E>(
    stream: impl IntoIterator<Item = &'static E>,
    active: Conditions,
) -> RegistryBuild<E::Item>
where
    E: RegistryEntry,
{
    let all = index_by_name::<E>(stream);
    let (runnable, skipped) = partition_by_conditions::<E>(all, active);
    validate_deps::<E>(&runnable, &skipped);
    let order = topo_order::<E>(&runnable);
    let items = order.into_iter().map(|n| runnable[n].make()).collect();
    RegistryBuild { items, skipped }
}
