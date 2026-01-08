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

    use crate::{LogParser, Rule};

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
            let Ok(line) = LogParser::parse(Rule::line, &line) else {
                continue;
            };
            // There should only ever be one event per line.
            let line = line.last()?;
            let mut line_inner = line.into_inner();
            let event = line_inner.find(|e| matches!(e.as_rule(), Rule::event))?;

            let event_str = event.as_str().trim();
            dbg!(event_str);

            // An object should always follow an event.
            let data = line_inner.nth(0)?;
            let event = Event::METADATA.iter().find(|m| m.doc == event_str)?;

            dbg!(event);
        }
    }
}
