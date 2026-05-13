//! Generic topo-sort over self-registered entries.
//!
//! Both ops and validators share the same shape: a `*Entry` struct with a
//! name, a list of dependency names, and a `make` function pointer that
//! materializes a `Box<dyn Trait>`. This module factors out the sort so
//! each registry consumer just provides the entry → name/deps/make
//! projections via the `RegistryEntry` trait.

use std::collections::{BTreeMap, BTreeSet};

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
}

/// Topologically sort a stream of entries by their `depends_on` edges.
///
/// Ties (entries with the same satisfied-dep set) are broken alphabetically
/// by `name()` for deterministic output across runs.
///
/// Panics on dependency cycles or unknown `depends_on` references.
pub fn topo_sort<E>(stream: impl IntoIterator<Item = &'static E>) -> Vec<Box<E::Item>>
where
    E: RegistryEntry,
{
    let entries: BTreeMap<&'static str, &'static E> =
        stream.into_iter().map(|e| (e.name(), e)).collect();

    // Validate dependency references.
    for e in entries.values() {
        for dep in e.depends_on() {
            if !entries.contains_key(dep) {
                panic!(
                    "{} '{}' depends on unknown {} '{}'",
                    E::KIND,
                    e.name(),
                    E::KIND,
                    dep
                );
            }
        }
    }

    let mut in_degree: BTreeMap<&'static str, usize> =
        entries.keys().map(|n| (*n, 0)).collect();
    for e in entries.values() {
        *in_degree.get_mut(e.name()).unwrap() = e.depends_on().len();
    }

    // BTreeSet keeps the ready set sorted for stable output.
    let mut ready: BTreeSet<&'static str> = in_degree
        .iter()
        .filter(|&(_, &d)| d == 0)
        .map(|(n, _)| *n)
        .collect();

    let mut order: Vec<&'static str> = Vec::with_capacity(entries.len());
    while let Some(&name) = ready.iter().next() {
        ready.remove(name);
        order.push(name);
        for e in entries.values() {
            if e.depends_on().contains(&name) {
                let d = in_degree.get_mut(e.name()).unwrap();
                *d -= 1;
                if *d == 0 {
                    ready.insert(e.name());
                }
            }
        }
    }

    if order.len() != entries.len() {
        let remaining: Vec<&str> = in_degree
            .iter()
            .filter(|&(_, &d)| d > 0)
            .map(|(n, _)| *n)
            .collect();
        panic!("{} dependency cycle among: {remaining:?}", E::KIND);
    }

    order.into_iter().map(|n| entries[n].make()).collect()
}
