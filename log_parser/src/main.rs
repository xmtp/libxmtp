use std::time::Duration;

use anyhow::Result;
use pest_derive::Parser;
use tokio::runtime::Runtime;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};
use xmtp_common::TestWriter;
use xmtp_mls::tester;

use crate::ui::file_open::{file_selected, open_file_dialog, open_log};

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

    ui.on_request_open_file({
        let ui_handle = ui.as_weak();
        move || open_file_dialog(ui_handle.clone())
    });
    ui.on_file_selected({
        let ui_handle = ui.as_weak();
        move |path| file_selected(ui_handle.clone(), path)
    });

    ui.on_build_log({
        let writer = writer.clone();
        let ui_handle = ui.as_weak();
        let runtime_handle = handle.clone();
        move || {
            writer.clear();

            runtime_handle
                .block_on(async {
                    tester!(bo);
                    tester!(alix);
                    tester!(caro);
                    bo.test_talk_in_dm_with(&alix).await?;
                    let (group, _) = bo.test_talk_in_new_group_with(&alix).await?;
                    group.add_members(&[caro.inbox_id()]).await?;
                    caro.sync_all_welcomes_and_groups(None).await?;
                    bo.sync_all_welcomes_and_groups(None).await?;

                    anyhow::Ok(())
                })
                .unwrap();

            std::thread::sleep(Duration::from_millis(500));

            open_log(ui_handle.clone(), &writer.as_string());
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

        while let Ok(_event) = LogEvent::from(&mut lines) {
            count += 1;
        }

        dbg!(count);
    }
}
