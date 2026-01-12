use anyhow::Result;
use pest::Parser;
use pest_derive::Parser;

use crate::ui::file_open::{file_selected, open_file_dialog};

mod state;
mod ui;

#[derive(Parser)]
#[grammar = "parser/defs/log.pest"]
struct LogParser;

slint::include_modules!();

fn main() -> Result<()> {
    let ui = AppWindow::new()?;

    ui.on_request_open_file({
        let ui_handle = ui.as_weak();
        move || open_file_dialog(ui_handle.clone())
    });

    ui.on_file_selected({
        let ui_handle = ui.as_weak();
        move |path| file_selected(ui_handle.clone(), path)
    });

    ui.run()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use pest::Parser;
    use tracing_subscriber::fmt;
    use xmtp_common::{Event, TestWriter};
    use xmtp_mls::tester;

    use crate::{LogParser, Rule, state::LogEvent};

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
        let lines: Vec<&str> = log.split("\n").collect();
        for line in lines {
            let Ok(event) = LogEvent::from(&line) else {
                continue;
            };
        }
    }
}
