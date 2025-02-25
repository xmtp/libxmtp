//! Logger for the CLI
use std::path::PathBuf;

use clap_verbosity_flag::{InfoLevel, Verbosity};
use color_eyre::eyre;
use owo_colors::OwoColorize;
use tracing::Dispatch;
use tracing::{Event, Subscriber};
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::{
    fmt::{format, format::Writer, time, FmtContext, FormatEvent, FormatFields},
    registry::LookupSpan,
    Layer,
};
use tracing_subscriber::{prelude::*, EnvFilter};

use crate::args::LogOptions;

#[derive(Default)]
pub struct Logger {
    json: bool,
    show_fields: bool,
    human: bool,
    logfmt: bool,
    verbosity: Verbosity<InfoLevel>,
    guards: Vec<tracing_appender::non_blocking::WorkerGuard>,
}

impl<'a> From<&'a LogOptions> for Logger {
    fn from(options: &'a LogOptions) -> Self {
        // let pretty = !(options.json || options.logfmt);
        Self {
            json: options.json,
            logfmt: options.logfmt,
            show_fields: options.show_fields,
            verbosity: options.verbose.clone(),
            human: options.human,
            guards: Vec::new(),
        }
    }
}

impl Logger {
    pub fn init(&mut self) -> eyre::Result<()> {
        let Logger {
            show_fields,
            json,
            human,
            logfmt,
            ref verbosity,
            ref mut guards,
        } = *self;

        let verbosity = verbosity.log_level_filter() as usize;

        // prefer `RUST_LOG` variable if set
        // otherwise passed-in level filter
        let app_filter = || {
            EnvFilter::try_from_default_env()
                .unwrap_or(EnvFilter::builder().parse_lossy(format!("xdbg={verbosity}")))
        };
        let file_filter = || {
            EnvFilter::builder().parse_lossy(
                "xmtp_mls=DEBUG,xmtp_id=DEBUG,xmtp_cryptography=DEBUG,xmtp_api_grpc=DEBUG",
            )
        };
        let subscriber = tracing_subscriber::registry();
        let now = chrono::Local::now();
        let log_file_name = PathBuf::from(format!("./xdbg_log-{}", now));

        let subscriber = subscriber
            // default, always-on layer
            .with(human_layer(app_filter(), true, std::io::stdout))
            .with(json.then(|| {
                let mut json = log_file_name.clone();
                json.set_extension("json");
                let file = std::fs::File::create_new(json).unwrap();
                let (appender, guard) = tracing_appender::non_blocking(file);
                guards.push(guard);
                tracing_subscriber::fmt::layer()
                    .json()
                    .flatten_event(true)
                    .with_level(true)
                    .with_timer(time::ChronoLocal::new("%s".into()))
                    .with_writer(appender)
                    .with_filter(file_filter())
            }))
            // Fields are enabled only if `show_fields` is true
            .with(human.then(|| {
                let mut human = log_file_name.clone();
                human.set_extension("log");
                let file = std::fs::File::create_new(human).unwrap();
                let (appender, guard) = tracing_appender::non_blocking(file);
                guards.push(guard);
                human_layer(file_filter(), show_fields, appender)
            }))
            .with(logfmt.then(|| {
                let mut logfmt = log_file_name.clone();
                logfmt.set_extension("logfmt");
                let file = std::fs::File::create_new(logfmt).unwrap();
                let (appender, guard) = tracing_appender::non_blocking(file);
                guards.push(guard);
                tracing_logfmt::builder()
                    .layer()
                    .with_writer(appender)
                    .with_filter(file_filter())
            }));

        let _ = tracing::dispatcher::set_global_default(Dispatch::new(subscriber));
        Ok(())
    }
}

fn human_layer<S, W>(filter: EnvFilter, show_fields: bool, writer: W) -> impl Layer<S>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    W: for<'writer> MakeWriter<'writer> + Send + Sync + 'static,
{
    let layer = tracing_subscriber::fmt::layer()
        .without_time()
        .map_event_format(|_| PrettyTarget)
        .fmt_fields({
            let fun = format::debug_fn(move |writer, field, value| {
                if field.name() == "message" {
                    write!(writer, "{:?}", value.white())
                } else {
                    if show_fields {
                        write!(writer, "{} {:?}", field.bold(), value.white())?;
                    }
                    Ok(())
                }
            });
            if show_fields {
                fun.delimited("\n\t")
            } else {
                fun.delimited("")
            }
        });

    layer.with_writer(writer).with_filter(filter)
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
            let target = event.metadata().target();
            match event.metadata().level().as_str() {
                "ERROR" => write!(&mut writer, "{} ", target.red().bold())?,
                "WARN" => write!(&mut writer, "{} ", target.yellow().bold())?,
                "INFO" => write!(&mut writer, "{} ", target.green().bold())?,
                "DEBUG" => write!(&mut writer, "{} ", target.blue().bold())?,
                "TRACE" => write!(&mut writer, "{} ", target.purple().bold())?,
                _ => (),
            }
        } else {
            write!(&mut writer, "{} ", event.metadata().target())?;
        }

        ctx.format_fields(writer.by_ref(), event)?;

        writeln!(writer)
    }
}
