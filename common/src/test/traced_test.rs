/// Tests that can assert on tracing logs in a tokio threaded context
use std::{io, sync::Arc};

use parking_lot::Mutex;
use tracing_subscriber::fmt;

thread_local! {
    pub static LOG_BUFFER: TestWriter = TestWriter::new();
}

/// Thread local writer which stores logs in memory
#[derive(Default)]
pub struct TestWriter(Arc<Mutex<Vec<u8>>>);

impl TestWriter {
    pub fn new() -> Self {
        Self(Arc::new(Mutex::new(vec![])))
    }

    pub fn as_string(&self) -> String {
        let buf = self.0.lock();
        String::from_utf8(buf.clone()).expect("Not valid UTF-8")
    }

    pub fn clear(&self) {
        let mut buf = self.0.lock();
        buf.clear();
    }
    pub fn flush(&self) {
        let mut buf = self.0.lock();
        std::io::Write::flush(&mut *buf).unwrap();
    }
}

impl io::Write for TestWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut this = self.0.lock();
        // still print logs for tests
        print!("{}", String::from_utf8_lossy(buf));
        Vec::<u8>::write(&mut this, buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        let mut this = self.0.lock();
        Vec::<u8>::flush(&mut this)
    }
}

impl Clone for TestWriter {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl fmt::MakeWriter<'_> for TestWriter {
    type Writer = TestWriter;

    fn make_writer(&self) -> Self::Writer {
        self.clone()
    }
}
/*
/// Only works with current-thread
#[inline]
pub fn traced_test<Fut>(f: impl Fn() -> Fut)
where
    Fut: futures::Future<Output = ()>,
{
    LOG_BUFFER.with(|buf| {
        let rt = tokio::runtime::Builder::new_current_thread()
            .thread_name("tracing-test")
            .enable_time()
            .enable_io()
            .build()
            .unwrap();
        buf.clear();

        let subscriber = fmt::Subscriber::builder()
            .with_env_filter(format!("{}=debug", env!("CARGO_PKG_NAME")))
            .with_writer(buf.clone())
            .with_level(true)
            .with_ansi(false)
            .finish();

        let dispatch = tracing::Dispatch::new(subscriber);
        tracing::dispatcher::with_default(&dispatch, || {
            rt.block_on(f());
        });

        buf.clear();
    });
}
*/

#[macro_export]
macro_rules! traced_test {
    ( $f:expr ) => {{
        use tracing_subscriber::fmt;
        use $crate::traced_test::TestWriter;

        $crate::traced_test::LOG_BUFFER.with(|buf| {
            let rt = tokio::runtime::Builder::new_current_thread()
                .thread_name("tracing-test")
                .enable_time()
                .enable_io()
                .build()
                .unwrap();
            buf.clear();

            let subscriber = fmt::Subscriber::builder()
                .with_env_filter(format!("{}=debug", env!("CARGO_PKG_NAME")))
                .with_writer(buf.clone())
                .with_level(true)
                .with_ansi(false)
                .finish();

            let dispatch = tracing::Dispatch::new(subscriber);
            tracing::dispatcher::with_default(&dispatch, || {
                rt.block_on($f);
            });

            buf.clear();
        });
    }};
}

/// macro that can assert logs in tests.
/// Note: tests that use this must be used in `traced_test` function
/// and only with tokio's `current` runtime.
#[macro_export]
macro_rules! assert_logged {
    ( $search:expr , $occurrences:expr ) => {
        $crate::traced_test::LOG_BUFFER.with(|buf| {
            let lines = {
                buf.flush();
                buf.as_string()
            };
            let lines = lines.lines();
            let actual = lines.filter(|line| line.contains($search)).count();
            if actual != $occurrences {
                panic!(
                    "Expected '{}' to be logged {} times, but was logged {} times instead",
                    $search, $occurrences, actual
                );
            }
        })
    };
}
