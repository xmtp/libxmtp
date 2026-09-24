//! Generic topo-sort over self-registered entries.
//!
//! Both ops and validators share the same shape: a `*Entry` struct with a
//! dependency list and a `&'static` reference to the trait object that
//! implements the work. Ops and validators are unit structs, so a single
//! `static` per impl lets us cheaply name and use them without boxing.

use std::collections::{BTreeMap, BTreeSet};

use crate::app::health::conditions::Conditions;

/// Projection trait. The `KIND` associated constant lets the trait describe
/// itself in panic messages ("op", "validator").
///
/// `Item` is the trait the ops/validators implement (e.g. `dyn HealthOp`).
/// `value()` returns a `'static` reference so callers can use the resulting
/// trait object without allocating.
pub trait RegistryEntry: 'static {
    type Item: ?Sized + 'static;

    /// Human label for panic messages, e.g. `"op"` or `"validator"`.
    const KIND: &'static str;

    fn depends_on(&self) -> &'static [&'static str];
    fn value(&self) -> &'static Self::Item;

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
pub struct RegistryBuild<I: ?Sized + 'static> {
    /// Topologically-sorted entries that passed the condition filter.
    pub items: Vec<&'static I>,
    /// Entries filtered out, in alphabetical order by name.
    pub skipped: Vec<SkippedEntry>,
}

/// Collect entries by name, panicking on duplicate registrations.
fn index_by_name<E: RegistryEntry>(
    stream: impl IntoIterator<Item = &'static E>,
) -> BTreeMap<&'static str, &'static E>
where
    E::Item: Named,
{
    let mut out: BTreeMap<&'static str, &'static E> = BTreeMap::new();
    for e in stream {
        let n = e.value().name();
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

/// Panic if any runnable entry depends on a name that doesn't exist
/// in the registry at all. Deps on entries that *were* skipped by the
/// active condition set are treated as advisory: the dep is honored
/// when the upstream runs, dropped when it doesn't. Lets read-only
/// ops declare "after AddMembers if AddMembers runs" without forcing
/// AddMembers to run.
fn validate_deps<E: RegistryEntry>(
    runnable: &BTreeMap<&'static str, &'static E>,
    skipped: &[SkippedEntry],
) where
    E::Item: Named,
{
    let skipped_names: std::collections::BTreeSet<&str> = skipped.iter().map(|s| s.name).collect();
    let bad = runnable
        .values()
        .flat_map(|e| e.depends_on().iter().map(move |d| (e, d)))
        .find(|(_, dep)| !runnable.contains_key(*dep) && !skipped_names.contains(*dep));

    let Some((e, dep)) = bad else { return };
    panic!(
        "{} '{}' depends on unknown {} '{}'",
        E::KIND,
        e.value().name(),
        E::KIND,
        dep,
    );
}

/// Run Kahn's algorithm over the validated runnable set. Returns the
/// topo-sorted name sequence; panics on cycle. Ties are broken
/// alphabetically because the ready set is a `BTreeSet`.
fn topo_order<E: RegistryEntry>(runnable: &BTreeMap<&'static str, &'static E>) -> Vec<&'static str>
where
    E::Item: Named,
{
    // Only count deps that are themselves in the runnable set; deps on
    // skipped entries are advisory and don't gate ordering here.
    let mut in_degree: BTreeMap<&'static str, usize> = runnable
        .iter()
        .map(|(n, e)| {
            let count = e
                .depends_on()
                .iter()
                .filter(|d| runnable.contains_key(*d))
                .count();
            (*n, count)
        })
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
                let d = in_degree.get_mut(e.value().name()).unwrap();
                *d -= 1;
                if *d == 0 {
                    ready.insert(e.value().name());
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
/// alphabetically by name. Panics on dependency cycles, unknown
/// `depends_on` references, duplicate names, or a runnable entry that
/// depends on a skipped entry.
pub fn topo_sort<E>(
    stream: impl IntoIterator<Item = &'static E>,
    active: Conditions,
) -> RegistryBuild<E::Item>
where
    E: RegistryEntry,
    E::Item: Named,
{
    let all = index_by_name::<E>(stream);
    let (runnable, skipped) = partition_by_conditions::<E>(all, active);
    validate_deps::<E>(&runnable, &skipped);
    let order = topo_order::<E>(&runnable);
    let items = order.into_iter().map(|n| runnable[n].value()).collect();
    RegistryBuild { items, skipped }
}

/// Items materialized through a `RegistryEntry` must expose a static name.
/// Both `HealthOp` and `Validator` already declare a `fn name()` method;
/// declaring the requirement here lets `topo_sort` read names from the
/// trait object without the entry struct duplicating the string.
pub trait Named {
    fn name(&self) -> &'static str;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestItem(&'static str);

    impl Named for TestItem {
        fn name(&self) -> &'static str {
            self.0
        }
    }

    struct TestEntry {
        item: &'static TestItem,
        deps: &'static [&'static str],
    }

    impl RegistryEntry for TestEntry {
        type Item = TestItem;
        const KIND: &'static str = "test";

        fn depends_on(&self) -> &'static [&'static str] {
            self.deps
        }
        fn value(&self) -> &'static TestItem {
            self.item
        }
    }

    fn names(out: Vec<&'static TestItem>) -> Vec<&'static str> {
        out.iter().map(|i| i.name()).collect()
    }

    static A_ITEM: TestItem = TestItem("A");
    static B_ITEM: TestItem = TestItem("B");
    static C_ITEM: TestItem = TestItem("C");
    static D_ITEM: TestItem = TestItem("D");
    static Z_ITEM: TestItem = TestItem("Z");

    #[test]
    fn empty_input_returns_empty() {
        let entries: Vec<&'static TestEntry> = Vec::new();
        assert!(
            topo_sort::<TestEntry>(entries, Conditions::empty())
                .items
                .is_empty()
        );
    }

    #[test]
    fn single_root_no_deps() {
        static A: TestEntry = TestEntry {
            item: &A_ITEM,
            deps: &[],
        };
        let out = topo_sort::<TestEntry>(vec![&A], Conditions::empty()).items;
        assert_eq!(names(out), vec!["A"]);
    }

    #[test]
    fn linear_chain_orders_by_dependency() {
        static A: TestEntry = TestEntry {
            item: &A_ITEM,
            deps: &[],
        };
        static B: TestEntry = TestEntry {
            item: &B_ITEM,
            deps: &["A"],
        };
        static C: TestEntry = TestEntry {
            item: &C_ITEM,
            deps: &["B"],
        };
        let out = topo_sort::<TestEntry>(vec![&C, &A, &B], Conditions::empty()).items;
        assert_eq!(names(out), vec!["A", "B", "C"]);
    }

    #[test]
    fn diamond_dag_orders_correctly() {
        static A: TestEntry = TestEntry {
            item: &A_ITEM,
            deps: &[],
        };
        static B: TestEntry = TestEntry {
            item: &B_ITEM,
            deps: &["A"],
        };
        static C: TestEntry = TestEntry {
            item: &C_ITEM,
            deps: &["A"],
        };
        static D: TestEntry = TestEntry {
            item: &D_ITEM,
            deps: &["B", "C"],
        };
        let out = names(topo_sort::<TestEntry>(vec![&A, &B, &C, &D], Conditions::empty()).items);
        assert_eq!(out, vec!["A", "B", "C", "D"]);
    }

    #[test]
    fn ties_break_alphabetically() {
        static A: TestEntry = TestEntry {
            item: &A_ITEM,
            deps: &[],
        };
        static Z: TestEntry = TestEntry {
            item: &Z_ITEM,
            deps: &[],
        };
        let out = names(topo_sort::<TestEntry>(vec![&Z, &A], Conditions::empty()).items);
        assert_eq!(out, vec!["A", "Z"]);
    }

    #[test]
    #[should_panic(expected = "duplicate test name 'A'")]
    fn duplicate_name_panics() {
        static A1: TestEntry = TestEntry {
            item: &A_ITEM,
            deps: &[],
        };
        static A2: TestEntry = TestEntry {
            item: &A_ITEM,
            deps: &[],
        };
        let _ = topo_sort::<TestEntry>(vec![&A1, &A2], Conditions::empty());
    }

    #[test]
    #[should_panic(expected = "depends on unknown test 'X'")]
    fn unknown_dep_panics() {
        static A: TestEntry = TestEntry {
            item: &A_ITEM,
            deps: &["X"],
        };
        let _ = topo_sort::<TestEntry>(vec![&A], Conditions::empty());
    }

    #[test]
    #[should_panic(expected = "test dependency cycle")]
    fn cycle_panics() {
        static A: TestEntry = TestEntry {
            item: &A_ITEM,
            deps: &["B"],
        };
        static B: TestEntry = TestEntry {
            item: &B_ITEM,
            deps: &["A"],
        };
        let _ = topo_sort::<TestEntry>(vec![&A, &B], Conditions::empty());
    }

    static GATED_ITEM: TestItem = TestItem("Gated");

    /// A TestEntry that requires STRICT_VERSIONING — used to exercise
    /// the condition-filter path without polluting the default
    /// TestEntry shape used by every other test.
    struct GatedEntry {
        item: &'static TestItem,
        deps: &'static [&'static str],
    }

    impl RegistryEntry for GatedEntry {
        type Item = TestItem;
        const KIND: &'static str = "test";

        fn depends_on(&self) -> &'static [&'static str] {
            self.deps
        }
        fn value(&self) -> &'static TestItem {
            self.item
        }
        fn requires(&self) -> Conditions {
            Conditions::STRICT_VERSIONING
        }
    }

    #[test]
    fn gated_entry_skipped_when_condition_inactive() {
        static G: GatedEntry = GatedEntry {
            item: &GATED_ITEM,
            deps: &[],
        };
        let build = topo_sort::<GatedEntry>(vec![&G], Conditions::empty());
        assert!(build.items.is_empty());
        assert_eq!(build.skipped.len(), 1);
        assert_eq!(build.skipped[0].name, "Gated");
        assert_eq!(build.skipped[0].missing, Conditions::STRICT_VERSIONING);
    }

    #[test]
    fn gated_entry_runs_when_condition_active() {
        static G: GatedEntry = GatedEntry {
            item: &GATED_ITEM,
            deps: &[],
        };
        let build = topo_sort::<GatedEntry>(vec![&G], Conditions::STRICT_VERSIONING);
        assert_eq!(names(build.items), vec!["Gated"]);
        assert!(build.skipped.is_empty());
    }
}
