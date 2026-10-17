//! Keep round reports bounded while databases retain the complete records.
use serde_json::{Value, json};

const REPORTED_COMMITS: usize = 16;

pub(crate) fn bound_history(report: &mut Value) {
    if let Some(installations) = report["installations"].as_array_mut() {
        for installation in installations {
            if let Some(groups) = installation["groups"].as_array_mut() {
                for group in groups {
                    let omitted = if let Some(commits) = group["commits"].as_array_mut() {
                        let omitted = commits.len().saturating_sub(REPORTED_COMMITS);
                        commits.drain(..omitted);
                        omitted
                    } else {
                        0
                    };
                    group["omitted_commit_count"] =
                        json!(group["omitted_commit_count"].as_u64().unwrap_or(0) + omitted as u64);
                }
            }
        }
    }
}
