use anyhow::Result;
use pest_derive::Parser;
use tokio::runtime::Runtime;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};
use xmtp_common::TestWriter;
use xmtp_mls::tester;

use crate::{
    state::log::LogFile,
    ui::file_open::{file_selected, open_file_dialog, open_log},
};

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
                    bo.test_talk_in_dm_with(&alix).await?;
                    bo.test_talk_in_new_group_with(&alix).await?;
                    anyhow::Ok(())
                })
                .unwrap();

            let log = writer.as_string();
            let log = LogFile::from_str(&log).unwrap();
            open_log(ui_handle.clone(), log);
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
        std::fs::write("logs.txt", &log).unwrap();
        let lines: Vec<&str> = log.split("\n").collect();

        let mut count = 0;
        for line in lines {
            let Ok(event) = LogEvent::from(&line) else {
                continue;
            };
            count += 1;
        }
        dbg!(count);
    }
}
