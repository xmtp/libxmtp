//! Op and validator result types.

use std::time::Duration;

use owo_colors::OwoColorize;

pub enum Status {
    Pass,
    Fail,
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

    pub fn counts(&self) -> (usize, usize) {
        let failed = self
            .results
            .iter()
            .filter(|r| matches!(r.status, Status::Fail))
            .count();
        let passed = self.results.len() - failed;
        (passed, failed)
    }

    pub fn print_summary(&self) {
        let (passed, failed) = self.counts();
        println!();
        println!("== Summary ==");
        println!(
            "Total: {passed} passed, {failed} failed ({} total)",
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
    /// Print a one-line `[✓]`/`[✗]` summary to stdout and emit a structured
    /// tracing event on the `healthcheck.op` target.
    pub fn print(&self) {
        let mark = match self.status {
            Status::Pass => "[✓]".green().to_string(),
            Status::Fail => "[✗]".red().to_string(),
        };
        let duration_ms = self.duration.as_millis();
        let target = self.target.as_deref().unwrap_or("");
        let err = match &self.error {
            Some(e) => format!(" — error: {e:#}"),
            None => String::new(),
        };
        println!(
            "{mark} {name:<32} ({duration_ms:>5}ms) {target}{err}",
            name = self.op_name
        );

        match self.status {
            Status::Pass => tracing::info!(
                target: "healthcheck",
                op = self.op_name,
                status = "pass",
                target = %self.target.as_deref().unwrap_or(""),
                duration_ms = duration_ms as u64,
                "op passed"
            ),
            Status::Fail => tracing::error!(
                target: "healthcheck",
                op = self.op_name,
                status = "fail",
                target = %self.target.as_deref().unwrap_or(""),
                duration_ms = duration_ms as u64,
                error = ?self.error,
                "op failed"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(r.counts(), (2, 1));
        assert!(r.has_failures());
    }

    #[test]
    fn empty_report_has_no_failures() {
        let r = Report::new();
        assert!(!r.has_failures());
        assert_eq!(r.counts(), (0, 0));
    }
}
