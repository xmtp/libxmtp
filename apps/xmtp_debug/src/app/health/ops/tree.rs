//! Cosmetic: render the resolved op execution order as staged layers. Not
//! load-bearing — purely human-readable preflight output before a run.
//!
//! Stage N contains every op whose longest dependency chain from a root
//! has length N. Ops in the same stage can run in any order (they have no
//! mutual dependencies). Stages run sequentially.

use super::{HealthOp, OpEntry};
use std::collections::BTreeMap;
use std::fmt::Write;

pub fn render_order_tree() -> String {
    let entries: BTreeMap<&'static str, &'static OpEntry> = inventory::iter::<OpEntry>
        .into_iter()
        .map(|e| (HealthOp::name(e.op), e))
        .collect();

    // stage[name] = longest dep-chain length from any root, computed via
    // memoized DFS. A node with no deps is stage 0. Otherwise it is
    // 1 + max(stage of each dep).
    let mut stage: BTreeMap<&'static str, usize> = BTreeMap::new();
    for name in entries.keys() {
        compute_stage(name, &entries, &mut stage);
    }

    // Group by stage. BTreeMap keys sort numerically; BTreeSet groups
    // alphabetically within each stage.
    let mut by_stage: BTreeMap<usize, Vec<&'static str>> = BTreeMap::new();
    for (name, s) in &stage {
        by_stage.entry(*s).or_default().push(*name);
    }
    for ops in by_stage.values_mut() {
        ops.sort();
    }

    let mut out = String::new();
    out.push_str(
        "healthcheck op stages (stages run sequentially; within a stage, \
         ops are independent and shown alphabetically):\n",
    );
    for (s, ops) in &by_stage {
        let _ = writeln!(out, "  stage {s}:");
        for op in ops {
            let _ = writeln!(out, "    - {op}");
        }
    }
    out
}

fn compute_stage(
    name: &'static str,
    entries: &BTreeMap<&'static str, &'static OpEntry>,
    stage: &mut BTreeMap<&'static str, usize>,
) -> usize {
    if let Some(&s) = stage.get(name) {
        return s;
    }
    let Some(entry) = entries.get(name) else {
        return 0;
    };
    let s = if entry.depends_on.is_empty() {
        0
    } else {
        entry
            .depends_on
            .iter()
            .map(|dep| compute_stage(dep, entries, stage) + 1)
            .max()
            .unwrap_or(0)
    };
    stage.insert(name, s);
    s
}
