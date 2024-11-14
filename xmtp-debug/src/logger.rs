//! Logger for the CLI
use std::str::FromStr;

use clap_verbosity_flag::{InfoLevel, Verbosity};
use color_eyre::eyre;
use owo_colors::OwoColorize;
use tracing::Dispatch;
use tracing::{Event, Subscriber};
use tracing_subscriber::{
    fmt::{format, format::Writer, time, FmtContext, FormatEvent, FormatFields},
    registry::LookupSpan,
};
use tracing_subscriber::{prelude::*, EnvFilter};

use crate::args::LogOptions;

pub struct Logger {
    json: bool,
    show_fields: bool,
    pretty: bool,
    logfmt: bool,
    verbosity: Verbosity<InfoLevel>,
}

impl Default for Logger {
    fn default() -> Self {
        Self {
            json: false,
            show_fields: false,
            pretty: true,
            logfmt: false,
            verbosity: Default::default(),
        }
    }
}

impl<'a> From<&'a LogOptions> for Logger {
    fn from(options: &'a LogOptions) -> Self {
        let pretty = !(options.json || options.logfmt);
        Self {
            json: options.json,
            logfmt: options.logfmt,
            show_fields: options.show_fields.unwrap_or(false),
            verbosity: options.verbose.clone(),
            pretty,
        }
    }
}

impl Logger {
    pub fn init(&self) -> eyre::Result<()> {
        let Logger {
            show_fields,
            json,
            pretty,
            logfmt,
            ref verbosity,
        } = *self;

        eyre::ensure!(
            (json as u8) + (pretty as u8) + (logfmt as u8) == 1,
            "Only one of (json, pretty, logfmt) format may be enabled at once"
        );

        let verbosity = verbosity.log_level_filter() as usize;
        let verbosity = tracing_subscriber::filter::LevelFilter::from_str(&verbosity.to_string())
            .expect("constant strings should never fail");

        // prefer `RUST_LOG` variable if set
        // otherwise passed-in level filter
        let filter = EnvFilter::builder()
            .with_default_directive(verbosity.into())
            .from_env_lossy();

        let subscriber = tracing_subscriber::registry().with(filter);

        let subscriber = subscriber
            .with(json.then(|| {
                tracing_subscriber::fmt::layer()
                    .json()
                    .flatten_event(true)
                    .with_level(true)
                    .with_timer(time::ChronoLocal::new("%s".into()))
            }))
            // a `femme`-like format that puts K/V pairs on newlines
            // Fields are enabled only if `show_fields` is true
            .with(pretty.then(|| {
                tracing_subscriber::fmt::layer()
                    .without_time()
                    .map_event_format(|_| PrettyTarget)
                    .fmt_fields(
                        format::debug_fn(move |writer, field, value| {
                            if field.name() == "message" {
                                write!(writer, "{:?}", value.white())
                            } else {
                                if show_fields {
                                    write!(writer, "{} {:?}", field.bold(), value.white())?;
                                }
                                Ok(())
                            }
                        })
                        .delimited("\n\t"),
                    )
            }))
            .with(logfmt.then(|| tracing_logfmt::layer()));

        let _ = tracing::dispatcher::set_global_default(Dispatch::new(subscriber));
        Ok(())
    }
}

pub struct PrettyTarget;
impl<S, N> FormatEvent<S, N> for PrettyTarget
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    // Required method
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> std::fmt::Result {
        if writer.has_ansi_escapes() {
            write!(&mut writer, "{} ", event.metadata().target().green().bold())?;
        } else {
            write!(&mut writer, "{} ", event.metadata().target())?;
        }

        ctx.format_fields(writer.by_ref(), event)?;

        writeln!(writer)
    }
}
