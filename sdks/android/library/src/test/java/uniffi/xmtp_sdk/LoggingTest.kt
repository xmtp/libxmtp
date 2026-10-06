package uniffi.xmtp_sdk

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertTrue
import org.junit.Test
import java.util.Collections
import java.util.concurrent.atomic.AtomicBoolean
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
                        client.conversations.createGroup(emptyList())
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

    // verifies: LOG-008
    // A Kotlin sink can end a live client and clear itself from inside its
    // callback. Rust drives only a bare SinkQueue with no client
    // (logging/sink/tests.rs::callback_can_emit_and_clear_itself). The record
    // comes from a call that holds no client: the requirement keeps its gap
    // waiver for an emitting call that holds the client it ends. A deadlock in
    // end() also blocks the cleanup end in withClients, so the JUnit timeout
    // reports it.
    @Test(timeout = 90_000L)
    fun sinkCanEndAClientAndClearItself() =
        runBlocking {
            withTimeout(60_000) {
                withClients {
                    val client = create(options = liveOptions().copy(registration = RegistrationOptions(auto = false)))
                    val armed = AtomicBoolean(false)
                    val ended = CompletableDeferred<Unit>()
                    initLogging(LoggingOptions(level = LogLevel.ERROR))
                    try {
                        setLogSink(
                            object : LogSink {
                                override suspend fun log(record: LogRecord) {
                                    if (!armed.compareAndSet(true, false)) return
                                    try {
                                        client.end()
                                        setLogSink(null)
                                        ended.complete(Unit)
                                    } catch (error: Throwable) {
                                        ended.completeExceptionally(error)
                                    }
                                }
                            },
                        )
                        armed.set(true)
                        assertTrue(runCatching { localSignerFromPrivateKey(ByteArray(31)) }.isFailure)
                        withTimeout(30_000) { ended.await() }
                        val failure = runCatching { client.isRegistered() }.exceptionOrNull()
                        assertTrue("The sink left the client open: $failure", failure is XmtpException.ClientClosed)
                    } finally {
                        setLogSink(null)
                        initLogging(LoggingOptions(level = LogLevel.WARN))
                    }
                }
            }
        }
}
