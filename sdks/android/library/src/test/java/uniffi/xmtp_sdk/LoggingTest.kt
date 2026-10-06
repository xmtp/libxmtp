package uniffi.xmtp_sdk

import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertTrue
import org.junit.Test
import java.util.Collections
import java.util.concurrent.atomic.AtomicInteger

// The generated Kotlin LogSink callback is patched to ask Rust for admission
// before it calls the app sink (apps/xmtp_sdk_bindgen/src/logging_admission.rs).
// Rust owns the queue, overflow, generation and redaction rules:
// xmtp_logging/src/sink_queue/tests.rs, xmtp_sdk/src/logging/sink/tests.rs
// (redaction: ::client_log_secrets_are_redacted).
class LoggingTest {
    @Test
    fun kotlinSinkReceivesSdkLogsAfterAThrowingCall() =
        runBlocking {
            withTimeout(60_000) {
                val calls = AtomicInteger()
                val records = Collections.synchronizedList(mutableListOf<LogRecord>())
                initLogging(LoggingOptions(level = LogLevel.DEBUG))
                setLogSink(
                    object : LogSink {
                        override suspend fun log(record: LogRecord) {
                            if (calls.incrementAndGet() == 1) throw IllegalStateException("sink failed")
                            records.add(record)
                        }
                    },
                )
                try {
                    withClients {
                        val client = create()
                        client.conversations().createGroup(emptyList())
                        assertTrue(
                            "No SDK log reached the Kotlin sink after ${calls.get()} calls",
                            eventually(
                                30_000,
                            ) { synchronized(records) { records.any { it.target.startsWith("xmtp") } } },
                        )
                    }
                } finally {
                    setLogSink(null)
                    initLogging(LoggingOptions(level = LogLevel.WARN))
                }
            }
        }
}
