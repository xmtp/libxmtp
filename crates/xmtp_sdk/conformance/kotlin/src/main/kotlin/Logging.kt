import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.withTimeout
import uniffi.xmtp_sdk.*
import java.util.concurrent.atomic.AtomicInteger

// verifies: LOG-002, LOG-003, LOG-004, LOG-005, LOG-007, LOG-008, LOG-009
// verifies: LOG-011, LOG-012, LOG-013
internal suspend fun loggingConformance(options: ClientOptions) = withTimeout(30_000) {
    initLogging(LoggingOptions(level = LogLevel.ERROR))
    loggingSecretsConformance(options)
    val records = Channel<LogRecord>(Channel.UNLIMITED)
    val responses = Channel<Boolean>(Channel.UNLIMITED)
    val active = AtomicInteger()
    val maximum = AtomicInteger()
    setLogSink(object : LogSink {
        override suspend fun log(record: LogRecord) {
            val count = active.incrementAndGet()
            maximum.accumulateAndGet(count, ::maxOf)
            try {
                records.send(record)
                if (!responses.receive()) throw object : Throwable() {
                    override val message: String get() = error("hostile log error formatting")
                }
            } finally { active.decrementAndGet() }
        }
    })
    sdkConformanceEmit(1u)
    check(records.receive().droppedRecords == 0uL)
    sdkConformanceEmit(4099u)
    responses.send(true)
    val second = records.receive()
    check(second.fields["sequence"] == "0" && second.droppedRecords == 3uL) { "log capacity or order failed" }
    sdkConformanceEmit(3u)
    responses.send(false)
    val third = records.receive()
    check(third.fields["sequence"] == "1" && third.droppedRecords == 5uL) { "failed callback lost drops" }
    sdkConformanceEmit(2u)
    responses.send(true)
    check(records.receive().droppedRecords == 1uL) { "success erased later drops" }
    check(maximum.get() == 1) { "log callbacks overlapped" }

    val replaced = Channel<Int>(Channel.UNLIMITED)
    for (generation in 1..100) {
        setLogSink(null)
        setLogSink(object : LogSink {
            override suspend fun log(record: LogRecord) {
                check(active.get() == 0) { "new sink overlapped old callback" }
                replaced.send(generation)
            }
        })
        sdkConformanceEmit(1u)
    }
    check(active.get() == 1) { "replacement cancelled the active callback" }
    responses.send(false)
    check(replaced.receive() == 100) { "replaced generation was handed off" }
    setLogSink(null)

    val finished = CompletableDeferred<Unit>()
    val next = CompletableDeferred<Boolean>()
    setLogSink(object : LogSink {
        override suspend fun log(record: LogRecord) {
            setLogSink(object : LogSink {
                override suspend fun log(record: LogRecord) { next.complete(finished.isCompleted) }
            })
            sdkConformanceEmit(1u)
            finished.complete(Unit)
        }
    })
    sdkConformanceEmit(1u)
    check(next.await()) { "reentrant replacement overlapped callbacks" }
    setLogSink(null)
    val client = SDKClient.create(generateLocalSigner(), options.copy(
        storage = StorageOptions(location = StorageLocation.InMemory),
        registration = RegistrationOptions(auto = false),
    ))
    loggingEndConformance(client)
    println("Kotlin logging: async queue, overflow, failure, generations and reentry passed")
}

// verifies: LOG-008
internal suspend fun loggingEndConformance(client: SDKClient) = withTimeout(10_000) {
    initLogging(LoggingOptions(level = LogLevel.ERROR))
    val ended = CompletableDeferred<Unit>()
    setLogSink(object : LogSink {
        override suspend fun log(record: LogRecord) {
            try {
                client.end()
                setLogSink(null)
                ended.complete(Unit)
            } catch (error: Throwable) { ended.completeExceptionally(error) }
        }
    })
    sdkConformanceEmit(1u)
    ended.await()
    check(runCatching { client.isRegistered() }.exceptionOrNull() is XmtpException.ClientClosed) {
        "log callback left the client open"
    }
}

// verifies: LOG-010
private suspend fun loggingSecretsConformance(options: ClientOptions) {
    val credential = "LOG_CREDENTIAL_SENTINEL_89d42"
    val signing = "LOG_SIGNING_KEY_SENTINEL_89d42!!!".toByteArray()
    val database = "LOG_DATABASE_KEY_SENTINEL_89d42".toByteArray()
    val forbidden = listOf(credential) + listOf(signing, database).flatMap { bytes ->
        listOf(bytes.decodeToString(), bytes.joinToString("") { "%02x".format(it) }, bytes.contentToString())
    }
    val observed = Channel<LogRecord>(Channel.UNLIMITED)
    setLogSink(object : LogSink {
        override suspend fun log(record: LogRecord) { observed.send(record) }
    })
    check(runCatching { Backend.connect(BackendOptions(url = "http://127.0.0.1:1", credential = Credential(null, "Bearer $credential\n", 0))) }.isFailure)
    check(runCatching { localSignerFromPrivateKey(signing) }.isFailure)
    check(runCatching { SDKClient.create(generateLocalSigner(), options.copy(storage = StorageOptions(location = StorageLocation.InMemory, encryptionKey = database))) }.isFailure)
    sdkConformanceEmit(1u)
    do {
        val record = observed.receive()
        val text = (listOf(record.message) + record.fields.values).joinToString("\n")
        check(forbidden.none { text.contains(it) }) { "app log exposed a secret" }
    } while (record.target != "xmtp_sdk::conformance")
    setLogSink(null)
}
