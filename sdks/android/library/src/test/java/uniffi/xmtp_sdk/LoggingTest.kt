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

private class LogMailbox<T> {
    private val channel = kotlinx.coroutines.channels.Channel<T>(kotlinx.coroutines.channels.Channel.UNLIMITED)

    fun push(value: T) {
        check(channel.trySend(value).isSuccess)
    }

    suspend fun next(): T = withTimeout(30_000) { channel.receive() }

    suspend fun waitForRelease(): T = channel.receive()
}

private suspend fun emitContractLogs(count: Int) {
    repeat(count) { index ->
        if (index % 2 == 0) {
            check(runCatching { localSignerFromPrivateKey(ByteArray(31)) }.isFailure)
        } else {
            check(runCatching { generateInboxId(PublicIdentity("invalid", PublicIdentityKind.ETHEREUM)) }.isFailure)
        }
    }
}

class LogDispatchContractTest {
    // verifies: LOG-002, LOG-003, LOG-004, LOG-005, LOG-007, LOG-011, LOG-012, LOG-013
    @Test(timeout = 180_000L)
    fun generatedKotlinCallbacksKeepQueueOrderBoundAndGeneration() =
        runBlocking {
            withTimeout(150_000) {
                for (oldSuccess in listOf(false, true)) {
                    val records = LogMailbox<LogRecord>()
                    val replies = LogMailbox<Boolean>()
                    val active = AtomicInteger()
                    val maximum = AtomicInteger()
                    initLogging(LoggingOptions(level = LogLevel.ERROR))
                    try {
                        setLogSink(
                            object : LogSink {
                                override suspend fun log(record: LogRecord) {
                                    val now = active.incrementAndGet()
                                    maximum.updateAndGet { maxOf(it, now) }
                                    records.push(record)
                                    try {
                                        check(replies.waitForRelease())
                                    } finally {
                                        active.decrementAndGet()
                                    }
                                }
                            },
                        )
                        emitContractLogs(1)
                        org.junit.Assert.assertEquals(0uL, records.next().droppedRecords)
                        emitContractLogs(4099)
                        replies.push(true)
                        val second = records.next()
                        org.junit.Assert.assertEquals(3uL, second.droppedRecords)
                        assertTrue(second.fields["error"].orEmpty().contains("invalid local signer private key"))
                        emitContractLogs(3)
                        replies.push(false)
                        val third = records.next()
                        org.junit.Assert.assertEquals(5uL, third.droppedRecords)
                        assertTrue(third.fields["error"].orEmpty().contains("invalid public identity"))
                        emitContractLogs(2)
                        replies.push(true)
                        org.junit.Assert.assertEquals(1uL, records.next().droppedRecords)
                        org.junit.Assert.assertEquals(1, maximum.get())
                        val replacement = LogMailbox<Pair<Int, LogRecord>>()
                        for (generation in 1..4) {
                            setLogSink(null)
                            setLogSink(
                                object : LogSink {
                                    override suspend fun log(record: LogRecord) {
                                        check(active.get() == 0)
                                        replacement.push(generation to record)
                                    }
                                },
                            )
                            emitContractLogs(4099)
                        }
                        org.junit.Assert.assertEquals(1, active.get())
                        replies.push(oldSuccess)
                        val (generation, record) = replacement.next()
                        org.junit.Assert.assertEquals(4, generation)
                        org.junit.Assert.assertEquals(3uL, record.droppedRecords)
                        assertTrue(record.fields["error"].orEmpty().contains("invalid local signer private key"))
                    } finally {
                        setLogSink(null)
                        initLogging(LoggingOptions(level = LogLevel.WARN))
                    }
                }
            }
        }
}
