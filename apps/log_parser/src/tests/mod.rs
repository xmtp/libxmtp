use crate::state::*;
use anyhow::{Result, bail};
use std::{sync::Arc, time::Duration};
use tracing_subscriber::fmt;
use xmtp_common::{Event, TestWriter};
use xmtp_mls::{client::ClientError, tester};

pub(crate) async fn capture_events<F, Fut>(f: F) -> Result<Vec<Arc<LogEvent>>, ClientError>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<(), ClientError>>,
{
    capture_events_with_error(f).await
}

async fn capture_events_with_error<F, Fut, E>(f: F) -> Result<Vec<Arc<LogEvent>>, E>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<(), E>>,
{
    let writer = TestWriter::new();
    let subscriber = fmt::Subscriber::builder()
        .with_writer(writer.clone())
        .with_level(true)
        .with_ansi(false)
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);

    f().await?;

    tokio::time::sleep(Duration::from_millis(500)).await;

    let log = writer.as_string();
    let lines = log.split('\n').peekable();
    let events = LogEvent::parse(lines);

    Ok(events)
}

pub(crate) trait EventLogs {
    // Get the installation id from the logs for the given name.
    // Only works if a name is associated with an AssociateName event.
    fn inst(&self, name: &str) -> Option<&String>;
    fn inbox(&self, name: &str) -> Option<&str>;
    fn events(&self, name: &str, event: Event) -> Vec<&Arc<LogEvent>>;
    fn n_events(&self, name: &str, event: Event, count: usize) -> Result<Vec<&Arc<LogEvent>>>;
    // Returns an error if there is more or less than one event.
    fn one_event(&self, name: &str, event: Event) -> Result<&Arc<LogEvent>>;
}
impl EventLogs for Vec<Arc<LogEvent>> {
    fn inst(&self, name: &str) -> Option<&String> {
        self.iter()
            .filter(|e| matches!(e.event, Event::AssociateName))
            .find_map(|e| (e.context("name")?.as_str().ok()? == name).then(|| &e.installation))
    }

    fn inbox(&self, name: &str) -> Option<&str> {
        let inst = self.inst(name)?;
        self.iter()
            .find(|e| matches!(e.event, Event::ClientCreated) && e.installation == *inst)
            .and_then(|e| e.context("inbox_id").and_then(|v| v.as_str().ok()))
    }

    fn events(&self, name: &str, event: Event) -> Vec<&Arc<LogEvent>> {
        let inst = self.inst(name).expect("Installation not found");
        self.iter()
            .filter(|e| e.event == event && e.installation == *inst)
            .collect()
    }

    fn one_event(&self, name: &str, event: Event) -> Result<&Arc<LogEvent>> {
        let mut events = self.events(name, event);
        let Some(event) = events.pop() else {
            bail!("No events");
        };
        if !events.is_empty() {
            bail!("More than one event. ({} events)", events.len() + 1);
        }

        Ok(event)
    }

    fn n_events(&self, name: &str, event: Event, count: usize) -> Result<Vec<&Arc<LogEvent>>> {
        let events = self.events(name, event);
        if events.len() != count {
            bail!(
                "Expected {count} events, got {} for {name} and {event}",
                events.len()
            );
        }

        Ok(events)
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_log_and_parse_dm_creation() {
    let events = capture_events(|| async {
        tester!(alix, disable_workers);
        tester!(bo, disable_workers);

        alix.test_talk_in_dm_with(&bo).await?;

        Ok(())
    })
    .await?;

    events.one_event("alix", Event::ClientCreated)?;
    events.one_event("alix", Event::ClientDropped)?;
    events.one_event("bo", Event::ClientCreated)?;
    events.one_event("bo", Event::ClientDropped)?;
    events.one_event("alix", Event::CreatedDM)?;

    let welcome = events.one_event("bo", Event::ReceivedWelcome)?;
    assert_eq!(welcome.context("conversation_type")?.as_str()?, "dm");
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_log_group_updates() {
    let events = capture_events(|| async {
        tester!(alix, disable_workers);
        tester!(bo, disable_workers);
        tester!(caro, disable_workers);

        let (alix_group, _) = alix.test_talk_in_new_group_with(&bo).await?;
        alix_group.add_members(&[caro.inbox_id()]).await?;

        bo.sync_all_welcomes_and_groups(None).await?;
        caro.sync_all_welcomes_and_groups(None).await?;

        alix_group.remove_members(&[caro.inbox_id()]).await?;

        bo.sync_all_welcomes_and_groups(None).await?;
        caro.sync_all_welcomes_and_groups(None).await?;

        Ok(())
    })
    .await?;

    // Check for the log that says we added caro's inbox to the group.
    let bo_events = events.n_events("bo", Event::MLSProcessedStagedCommit, 2)?;

    let caro_inbox = events.inbox("caro")?;

    // Caro was added
    let added_inboxes = bo_events[0].added_inboxes()?;
    assert!(added_inboxes.contains(&caro_inbox));

    // Caro was removed
    let removed_inboxes = bo_events[1].removed_inboxes()?;
    assert!(removed_inboxes.contains(&caro_inbox));

    // Caro created a group from welcome
    let caro_event = events.one_event("caro", Event::ReceivedWelcome)?;
    assert_eq!(caro_event.context("conversation_type")?.as_str()?, "group");
}
