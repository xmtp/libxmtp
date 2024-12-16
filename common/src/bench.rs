use once_cell::sync::OnceCell;
use std::sync::Once;
use tracing::{Metadata, Subscriber};
use tracing_flame::{FlameLayer, FlushGuard};
use tracing_subscriber::{
    layer::{Context, Filter, Layer, SubscriberExt},
    registry::LookupSpan,
    util::SubscriberInitExt,
    EnvFilter,
};
static INIT: Once = Once::new();

static LOGGER: OnceCell<FlushGuard<std::io::BufWriter<std::fs::File>>> = OnceCell::new();

pub const BENCH_ROOT_SPAN: &str = "xmtp-trace-bench";

/// initializes logging for benchmarks
/// - FMT logging is enabled by passing the normal `RUST_LOG` environment variable options.
/// - Generate a flamegraph from tracing data by passing `XMTP_FLAMEGRAPH=trace`
pub fn logger() {
    INIT.call_once(|| {
        let (flame_layer, guard) = FlameLayer::with_file("./tracing.folded").unwrap();
        let flame_layer = flame_layer
            .with_threads_collapsed(true)
            .with_module_path(true);
        // .with_empty_samples(false);

        tracing_subscriber::registry()
            .with(tracing_subscriber::fmt::layer().with_filter(EnvFilter::from_default_env()))
            .with(
                flame_layer
                    // .with_filter(BenchFilter)
                    .with_filter(EnvFilter::from_env("XMTP_FLAMEGRAPH")),
            )
            .init();

        LOGGER.set(guard).unwrap();
    })
}

/// criterion `batch_iter` surrounds the closure in a `Runtime.block_on` despite being a sync
/// function, even in the async 'to_async` setup. Therefore we do this (only _slightly_) hacky
/// workaround to allow us to async setup some groups.
pub fn bench_async_setup<F, T, Fut>(fun: F) -> T
where
    F: Fn() -> Fut,
    Fut: futures::future::Future<Output = T>,
{
    use tokio::runtime::Handle;
    tokio::task::block_in_place(move || Handle::current().block_on(async move { fun().await }))
}

/// Filters for only spans where the root span name is "bench"
pub struct BenchFilter;

impl<S> Filter<S> for BenchFilter
where
    S: Subscriber + for<'lookup> LookupSpan<'lookup> + std::fmt::Debug,
    for<'lookup> <S as LookupSpan<'lookup>>::Data: std::fmt::Debug,
{
    fn enabled(&self, meta: &Metadata<'_>, cx: &Context<'_, S>) -> bool {
        if meta.name() == BENCH_ROOT_SPAN {
            return true;
        }
        if let Some(id) = cx.current_span().id() {
            if let Some(s) = cx.span_scope(id) {
                if let Some(s) = s.from_root().take(1).collect::<Vec<_>>().first() {
                    return s.name() == BENCH_ROOT_SPAN;
                }
            }
        }
        false
    }
}
