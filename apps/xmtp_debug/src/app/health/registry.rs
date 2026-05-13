//! Generic topo-sort over self-registered entries.
//!
//! Both ops and validators share the same shape: a `*Entry` struct with a
//! dependency list and a `&'static` reference to the trait object that
//! implements the work. Ops and validators are unit structs, so a single
//! `static` per impl lets us cheaply name and use them without boxing.

use std::collections::{BTreeMap, BTreeSet};

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
}

/// Topologically sort a stream of entries by their `depends_on` edges.
///
/// Each entry must implement `Named` (via `value()`) so the sort can read
/// names through the trait object. Ties (entries with the same satisfied-
/// dep set) are broken alphabetically by name.
///
/// Panics on dependency cycles, unknown `depends_on` references, or
/// duplicate names.
pub fn topo_sort<E>(stream: impl IntoIterator<Item = &'static E>) -> Vec<&'static E::Item>
where
    E: RegistryEntry,
    E::Item: Named,
{
    let mut entries: BTreeMap<&'static str, &'static E> = BTreeMap::new();
    for e in stream {
        let n = e.value().name();
        if entries.insert(n, e).is_some() {
            panic!("duplicate {} name '{}'", E::KIND, n);
        }
    }

    // Validate dependency references.
    for e in entries.values() {
        for dep in e.depends_on() {
            if !entries.contains_key(dep) {
                panic!(
                    "{} '{}' depends on unknown {} '{}'",
                    E::KIND,
                    e.value().name(),
                    E::KIND,
                    dep
                );
            }
        }
    }

    let mut in_degree: BTreeMap<&'static str, usize> = entries.keys().map(|n| (*n, 0)).collect();
    for e in entries.values() {
        *in_degree.get_mut(e.value().name()).unwrap() = e.depends_on().len();
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
                let d = in_degree.get_mut(e.value().name()).unwrap();
                *d -= 1;
                if *d == 0 {
                    ready.insert(e.value().name());
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

    order.into_iter().map(|n| entries[n].value()).collect()
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
        assert!(topo_sort::<TestEntry>(entries).is_empty());
    }

    #[test]
    fn single_root_no_deps() {
        static A: TestEntry = TestEntry {
            item: &A_ITEM,
            deps: &[],
        };
        let out = topo_sort::<TestEntry>(vec![&A]);
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
        let out = topo_sort::<TestEntry>(vec![&C, &A, &B]);
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
        let out = names(topo_sort::<TestEntry>(vec![&A, &B, &C, &D]));
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
        let out = names(topo_sort::<TestEntry>(vec![&Z, &A]));
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
        let _ = topo_sort::<TestEntry>(vec![&A1, &A2]);
    }

    #[test]
    #[should_panic(expected = "depends on unknown test 'X'")]
    fn unknown_dep_panics() {
        static A: TestEntry = TestEntry {
            item: &A_ITEM,
            deps: &["X"],
        };
        let _ = topo_sort::<TestEntry>(vec![&A]);
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
        let _ = topo_sort::<TestEntry>(vec![&A, &B]);
    }
}
