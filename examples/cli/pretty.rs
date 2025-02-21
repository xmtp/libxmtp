use owo_colors::OwoColorize;
use tracing::{Event, Subscriber};
use tracing_subscriber::{
    fmt::{FmtContext, FormatEvent, FormatFields, format::Writer},
    registry::LookupSpan,
};

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
