#![recursion_limit = "256"]
use crate::{
    state::{LogEvent, State},
    ui::file_open::{file_selected, open_file_dialog},
};
use anyhow::Result;
use arboard::Clipboard;
use pest_derive::Parser;
use std::{
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};
use tokio::runtime::Runtime;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};
use xmtp_common::TestWriter;
use xmtp_mls::tester;

mod state;
#[cfg(test)]
mod tests;
mod ui;

#[derive(Parser)]
#[grammar = "parser/defs/log.pest"]
struct LogParser;

slint::include_modules!();

static EXAMPLE_COUNT: AtomicUsize = AtomicUsize::new(1);

fn main() -> Result<()> {
    let writer = TestWriter::new();

    let rt = Runtime::new()?;
    let handle = rt.handle();

    tracing_subscriber::registry()
        .with(fmt::layer().with_writer(writer.clone()))
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .init();

    tracing::info!("Log parser starting up");
    let ui = AppWindow::new()?;
    let state = State::new(Some(ui.as_weak()));

    if let Some(path) = std::env::args().nth(1) {
        match std::fs::read_to_string(&path) {
            Ok(log) => {
                let lines = log.split('\n').peekable();
                let events = LogEvent::parse(lines);
                state.add_source(path, events);
            }
            Err(e) => tracing::error!("Failed to read log file {path}: {e}"),
        }
    };

    ui.on_request_open_file({
        let ui_handle = ui.as_weak();
        move || open_file_dialog(ui_handle.clone())
    });
    ui.on_file_selected({
        let state = state.clone();
        move |path| file_selected(path, state.clone())
    });

    ui.on_build_log({
        let writer = writer.clone();
        let runtime_handle = handle.clone();
        let state = state.clone();
        move || {
            writer.clear();

            let result = runtime_handle.block_on(async {
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
            });

            if let Err(err) = result {
                tracing::error!("{err}");
            }

            std::thread::sleep(Duration::from_millis(500));

            let file = writer.as_string();
            if let Err(err) = std::fs::write("logs.txt", &file) {
                tracing::error!("{err:?}");
            }

            let lines = file.split('\n').peekable();
            let events = LogEvent::parse(lines);
            state.add_source(
                format!("Example {}", EXAMPLE_COUNT.fetch_add(1, Ordering::Relaxed)),
                events,
            );
        }
    });

    ui.on_state_clicked({
        let ui_handle = ui.as_weak();
        let state = state.clone();
        move |group_id, installation_id, unique_id| {
            let group_id = group_id.to_string();
            let unique_id = unique_id as u64;

            tracing::info!(
                "State clicked: group={}, installation={}, unique_id={}",
                group_id,
                installation_id,
                unique_id
            );

            if let Some(group_state) = state.find_group_state_by_id(&group_id, unique_id) {
                let detail = group_state.ui_group_state_detail(&installation_id);
                if let Some(ui) = ui_handle.upgrade() {
                    ui.set_selected_state_detail(detail);
                    ui.set_show_state_detail(true);
                }
            } else {
                tracing::warn!("Could not find GroupState with unique_id={}", unique_id);
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

    ui.on_copy_intermediate({
        move |text| {
            let text = text.to_string();

            match Clipboard::new() {
                Ok(mut ctx) => {
                    if let Err(e) = ctx.set_text(text.clone()) {
                        tracing::error!("Failed to copy to clipboard: {}", e);
                    } else {
                        tracing::info!(
                            "Copied intermediate logs to clipboard ({} chars)",
                            text.len()
                        );
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to access clipboard: {}", e);
                }
            }
        }
    });

    ui.on_remove_source({
        let state = state.clone();
        move |source_name| {
            let source_name = source_name.to_string();
            tracing::info!("Removing source: {}", source_name);
            state.remove_source(&source_name);
        }
    });

    ui.on_events_page_changed({
        let state = state.clone();
        move |page| {
            tracing::info!("Events page changed to: {}", page);
            state.set_events_page(page as u32);
        }
    });

    ui.on_groups_page_changed({
        let state = state.clone();
        move |page| {
            tracing::info!("Groups page changed to: {}", page);
            state.set_groups_page(page as u32);
        }
    });

    ui.on_show_errors_only_changed({
        let state = state.clone();
        move |checked| {
            tracing::info!("Show errors only changed to: {}", checked);
            state.set_show_errors_only(checked);
        }
    });

    ui.on_focus_group({
        let state = state.clone();
        move |group_id| {
            let group_id = group_id.to_string();
            tracing::info!("Focus group: {}", group_id);
            state.focus_group(group_id);
        }
    });

    ui.on_unfocus_group({
        let state = state.clone();
        move |group_id| {
            let group_id = group_id.to_string();
            tracing::info!("Unfocus group: {}", group_id);
            state.unfocus_group(&group_id);
        }
    });

    ui.on_clear_focused_groups({
        let state = state.clone();
        move || {
            tracing::info!("Clear focused groups");
            state.clear_focused_groups();
        }
    });

    ui.run()?;
    Ok(())
}
