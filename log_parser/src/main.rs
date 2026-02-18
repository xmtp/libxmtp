use crate::{
    state::LogState,
    ui::file_open::{file_selected, open_file_dialog},
};
use anyhow::Result;
use parking_lot::RwLock;
use pest_derive::Parser;
use std::{sync::Arc, time::Duration};
use tokio::runtime::Runtime;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};
use xmtp_common::TestWriter;
use xmtp_mls::tester;

mod state;
mod ui;

#[derive(Parser)]
#[grammar = "parser/defs/log.pest"]
struct LogParser;

slint::include_modules!();

fn main() -> Result<()> {
    let writer = TestWriter::new();

    let _rt = Runtime::new().unwrap();
    let handle = _rt.handle();

    tracing_subscriber::registry()
        .with(fmt::layer().with_writer(writer.clone()))
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .init();

    tracing::info!("Log parser starting up");
    let ui = AppWindow::new()?;

    // Global state storage that persists across callbacks
    let current_log_state: Arc<RwLock<Option<Arc<LogState>>>> = Arc::new(RwLock::new(None));

    ui.on_request_open_file({
        let ui_handle = ui.as_weak();
        move || open_file_dialog(ui_handle.clone())
    });
    ui.on_file_selected({
        let ui_handle = ui.as_weak();
        let log_state_ref = current_log_state.clone();
        move |path| file_selected(ui_handle.clone(), path, log_state_ref.clone())
    });

    ui.on_build_log({
        let writer = writer.clone();
        let ui_handle = ui.as_weak();
        let runtime_handle = handle.clone();
        let log_state_ref = current_log_state.clone();
        move || {
            writer.clear();

            runtime_handle
                .block_on(async {
                    tester!(bo, stream);
                    tester!(alix, stream);
                    tester!(caro, stream);
                    bo.test_talk_in_dm_with(&alix).await?;
                    let (group, _) = bo.test_talk_in_new_group_with(&alix).await?;
                    group.add_members(&[caro.inbox_id()]).await?;

                    alix.save_snapshot_to_file("alix.db3");
                    tester!(alix2, snapshot_file: "alix.db3", stream);

                    group.update_group_name("Fellows".into()).await?;
                    caro.sync_all_welcomes_and_groups(None).await?;
                    bo.sync_all_welcomes_and_groups(None).await?;

                    anyhow::Ok(())
                })
                .unwrap();

            std::thread::sleep(Duration::from_millis(500));

            let file = writer.as_string();
            std::fs::write("logs.txt", &file).unwrap();

            let lines = file.split('\n').peekable();

            let state = LogState::new();
            state.ingest_all(lines);
            state.clone().update_ui(&ui_handle);

            // Store the state for later use by state-clicked callback
            *log_state_ref.write() = Some(state);
        }
    });

    ui.on_state_clicked({
        let ui_handle = ui.as_weak();
        let log_state_ref = current_log_state.clone();
        move |group_id, installation_id, unique_id| {
            let group_id = group_id.to_string();
            let installation_id = installation_id.to_string();
            let unique_id = unique_id as u64;

            tracing::info!(
                "State clicked: group={}, installation={}, unique_id={}",
                group_id,
                installation_id,
                unique_id
            );

            let log_state_guard = log_state_ref.read();
            if let Some(ref log_state) = *log_state_guard {
                if let Some(group_state) =
                    log_state.find_group_state_by_id(&installation_id, &group_id, unique_id)
                {
                    let detail = group_state.ui_group_state_detail(&installation_id);
                    if let Some(ui) = ui_handle.upgrade() {
                        ui.set_selected_state_detail(detail);
                        ui.set_show_state_detail(true);
                    }
                } else {
                    tracing::warn!("Could not find GroupState with unique_id={}", unique_id);
                }
            } else {
                tracing::warn!("No log state available");
            }
        }
    });

    ui.on_close_state_detail({
        let ui_handle = ui.as_weak();
        move || {
            if let Some(ui) = ui_handle.upgrade() {
                ui.set_show_state_detail(false);
            }
        }
    });

    ui.run()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::state::LogEvent;
    use tracing_subscriber::fmt;
    use xmtp_common::TestWriter;
    use xmtp_mls::tester;

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_log_parsing() {
        let writer = TestWriter::new();

        let subscriber = fmt::Subscriber::builder()
            .with_writer(writer.clone())
            .with_level(true)
            .with_ansi(false)
            .finish();

        let _guard = tracing::subscriber::set_default(subscriber);

        tester!(bo);
        tester!(alix);
        bo.test_talk_in_dm_with(&alix).await?;

        let log = writer.as_string();
        let mut lines = log.split("\n").peekable();

        let mut count = 0;
        let mut line_count: usize = 0;

        while let Ok(_event) = LogEvent::from(&mut lines, &mut line_count) {
            count += 1;
        }

        dbg!(count);
    }
}
