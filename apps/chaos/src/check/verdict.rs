use std::fmt;

use serde::{Deserialize, Serialize};

use super::MembershipRecord;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Verdict {
    #[default]
    Pass,
    Warn,
    Stall,
    Brick,
    Fork,
    Harness,
}

impl fmt::Display for Verdict {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Pass => "PASS",
            Self::Warn => "WARN",
            Self::Stall => "STALL",
            Self::Brick => "BRICK",
            Self::Fork => "FORK",
            Self::Harness => "HARNESS",
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Finding {
    pub verdict: Verdict,
    pub instance: Option<usize>,
    pub group_id: Option<String>,
    pub topic: Option<String>,
    pub detail: String,
    pub repeated_rounds: u64,
    pub escalated: bool,
}

impl Finding {
    pub(super) fn group(
        verdict: Verdict,
        instance: usize,
        group_id: &str,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            verdict,
            instance: Some(instance),
            group_id: Some(group_id.to_owned()),
            topic: None,
            detail: detail.into(),
            repeated_rounds: 0,
            escalated: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CheckResult {
    pub verdict: Verdict,
    pub violation: bool,
    pub findings: Vec<Finding>,
    pub membership: Vec<MembershipRecord>,
}

impl CheckResult {
    pub(super) fn new(
        findings: Vec<Finding>,
        membership: Vec<MembershipRecord>,
        strict: bool,
    ) -> Self {
        let verdict = findings
            .iter()
            .map(|finding| finding.verdict)
            .max()
            .unwrap_or_default();
        let violation = findings.iter().any(|finding| {
            matches!(finding.verdict, Verdict::Fork | Verdict::Brick)
                || (strict && finding.verdict == Verdict::Warn)
        });
        Self {
            verdict,
            violation,
            findings,
            membership,
        }
    }
}
