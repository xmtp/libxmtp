use pest::Parser;
use pest_derive::Parser;

mod state;

#[derive(Parser)]
#[grammar = "parser/defs/log.pest"]
struct LogParser;

fn main() {
    println!("Hello, world!");
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
