//! Op and validator result types.

use std::time::Duration;

use owo_colors::OwoColorize;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Status {
    Pass,
    Fail,
    /// Op was registered but its required conditions weren't all
    /// active for this run. Carries the bits that were missing so the
    /// output line can explain *why* it was skipped.
    Skipped(crate::app::health::conditions::Conditions),
}

pub struct OpResult {
    pub op_name: &'static str,
    pub target: Option<String>,
    pub status: Status,
    pub duration: Duration,
    pub error: Option<color_eyre::eyre::Report>,
}

pub struct Report {
    pub results: Vec<OpResult>,
}

impl Report {
    pub fn new() -> Self {
        Self {
            results: Vec::new(),
        }
    }

    pub fn push(&mut self, r: OpResult) {
        self.results.push(r);
    }

    pub fn has_failures(&self) -> bool {
        self.results
            .iter()
            .any(|r| matches!(r.status, Status::Fail))
    }

    /// Returns `(passed, failed, skipped)`.
    pub fn counts(&self) -> (usize, usize, usize) {
        let mut passed = 0;
        let mut failed = 0;
        let mut skipped = 0;
        for r in &self.results {
            match r.status {
                Status::Pass => passed += 1,
                Status::Fail => failed += 1,
                Status::Skipped(_) => skipped += 1,
            }
        }
        (passed, failed, skipped)
    }

    pub fn print_summary(&self) {
        let (passed, failed, skipped) = self.counts();
        println!();
        println!("== Summary ==");
        println!(
            "Total: {passed} passed, {failed} failed, {skipped} skipped ({} total)",
            self.results.len()
        );
        if self.has_failures() {
            println!("Result: {}", "FAIL".red().bold());
        } else {
            println!("Result: {}", "PASS".green().bold());
        }
    }
}

impl Default for Report {
    fn default() -> Self {
        Self::new()
    }
}

impl OpResult {
    /// Print a one-line `[✓]`/`[✗]` summary to stdout. Use [`Self::emit`]
    /// to additionally fire a structured `tracing` event.
    /// Construct a `Skipped` result for an op that wasn't run because
    /// its declared conditions weren't all active. `missing` is the
    /// difference between `requires` and the active set.
    pub fn skipped(
        op_name: &'static str,
        missing: crate::app::health::conditions::Conditions,
    ) -> Self {
        Self {
            op_name,
            target: None,
            status: Status::Skipped(missing),
            duration: Duration::ZERO,
            error: None,
        }
    }

    pub fn print(&self) {
        let mark = match self.status {
            Status::Pass => "[✓]".green().to_string(),
            Status::Fail => "[✗]".red().to_string(),
            Status::Skipped(_) => "[—]".yellow().to_string(),
        };
        let duration_ms = self.duration.as_millis();
        let target = self.target.as_deref().unwrap_or("");
        let suffix = match (&self.status, &self.error) {
            (Status::Skipped(missing), _) => format!(" — skipped: requires {missing:?}"),
            (_, Some(e)) => format!(" — error: {e:#}"),
            (_, None) => String::new(),
        };
        println!(
            "{mark} {name:<32} ({duration_ms:>5}ms) {target}{suffix}",
            name = self.op_name
        );
    }

    /// Emit a structured `tracing` event for this result on the
    /// `healthcheck` target. Independent of [`Self::print`] so log sinks
    /// and stdout can be controlled separately.
    pub fn emit(&self) {
        let duration_ms = self.duration.as_millis() as u64;
        match self.status {
            Status::Pass => tracing::info!(
                target: "healthcheck",
                op = self.op_name,
                status = "pass",
                target = %self.target.as_deref().unwrap_or(""),
                duration_ms,
                "op passed"
            ),
            Status::Fail => tracing::error!(
                target: "healthcheck",
                op = self.op_name,
                status = "fail",
                target = %self.target.as_deref().unwrap_or(""),
                duration_ms,
                error = ?self.error,
                "op failed"
            ),
            Status::Skipped(missing) => tracing::info!(
                target: "healthcheck",
                op = self.op_name,
                status = "skipped",
                target = %self.target.as_deref().unwrap_or(""),
                missing = ?missing,
                "op skipped"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::health::conditions::Conditions;

    fn mk(name: &'static str, status: Status) -> OpResult {
        OpResult {
            op_name: name,
            target: None,
            status,
            duration: Duration::from_millis(10),
            error: None,
        }
    }

    #[test]
    fn report_counts_and_failures() {
        let mut r = Report::new();
        r.push(mk("A", Status::Pass));
        r.push(mk("B", Status::Fail));
        r.push(mk("C", Status::Pass));
        assert_eq!(r.counts(), (2, 1, 0));
        assert!(r.has_failures());
    }

    #[test]
    fn empty_report_has_no_failures() {
        let r = Report::new();
        assert!(!r.has_failures());
        assert_eq!(r.counts(), (0, 0, 0));
    }

    #[test]
    fn skipped_counts_as_skipped_not_pass_or_fail() {
        let mut r = Report::new();
        r.push(mk("A", Status::Pass));
        r.push(OpResult::skipped("B", Conditions::STRICT_VERSIONING));
        r.push(OpResult::skipped("C", Conditions::STRICT_VERSIONING));
        assert_eq!(r.counts(), (1, 0, 2));
        assert!(!r.has_failures());
    }

    #[test]
    fn skipped_constructor_records_missing_conditions() {
        let r = OpResult::skipped("X", Conditions::STRICT_VERSIONING);
        assert_eq!(r.op_name, "X");
        match r.status {
            Status::Skipped(m) => assert_eq!(m, Conditions::STRICT_VERSIONING),
            _ => panic!("expected Skipped"),
        }
        assert!(r.error.is_none());
    }
}
