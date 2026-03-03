use crate::state::{InstallationId, State, assertions::LogAssertion};
use anyhow::{Context, Result};
use std::{collections::HashMap, sync::atomic::Ordering};
use xmtp_common::Event;

pub struct AccountForDrift;

impl LogAssertion for AccountForDrift {
    fn assert(state: &State) -> Result<()> {
        let sources = state.sources.lock();

        // This is when the commits go out.
        let mut commit_timestamps = HashMap::new();
        let mut drift: HashMap<InstallationId, i64> = HashMap::new();

        // Collect when all of the commits went out.
        for (_source, events) in &*sources {
            for event in events {
                if !matches!(event.event, Event::GroupSyncStagedCommitPresent) {
                    continue;
                }

                let hash = event
                    .context("hash")
                    .context("hash field is missing when it should be present")?
                    .as_str()?;

                commit_timestamps.insert(hash.to_string(), event.time());
            }
        }

        // Now find all commits that were received, and push the timeline forward until
        // they are AFTER when the commit is sent.
        for (_source, events) in &*sources {
            for event in events {
                if !matches!(event.event, Event::MLSReceivedStagedCommit) {
                    continue;
                }

                let hash = event
                    .context("hash")
                    .context("hash field is missing when it should be present")?
                    .as_str()?;

                let Some(&sent_at) = commit_timestamps.get(hash) else {
                    continue;
                };

                if event.time() <= sent_at {
                    let drift = drift.entry(event.installation.clone()).or_default();

                    *drift = (*drift).max(sent_at - event.time() + 1);
                }
            }
        }

        for (_source, events) in &*sources {
            for event in events {
                let Some(&drift) = drift.get(&event.installation) else {
                    continue;
                };

                event.time.fetch_add(drift, Ordering::Relaxed);
            }
        }

        Ok(())
    }
}
