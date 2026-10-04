import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.isActive
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeout
import uniffi.xmtp_sdk.*
import java.nio.ByteBuffer
import java.nio.file.Files
import java.util.Base64
import java.util.concurrent.atomic.AtomicInteger

// Exercises the real adapter exposed only by the conformance reader seam.
internal suspend fun checkReaderReadFailuresEndExactlyOnce(owner: SDKClient) {
    for (failure in listOf(IllegalStateException("read failed"), AssertionError("native read failed"))) {
        val reads = AtomicInteger()
        val ends = AtomicInteger()
        val closes = mutableListOf<SDKStreamCloseReason>()
        val values = mutableListOf<Int>()
        val flow =
            readerFlow<Int, Unit>(
                owner = owner,
                open = { Unit },
                next = {
                    reads.incrementAndGet()
                    throw failure
                },
                end = {
                    check(currentCoroutineContext().isActive) { "reader teardown ran in a cancelled context" }
                    ends.incrementAndGet()
                    Unit
                },
                connectionState = { ConnectionState.CONNECTED },
                connectionStateChanged = { _, _ -> awaitCancellation() },
                onClose = { closes.add(it) },
                onConnectionStateChange = null,
            )
        check(runCatching { withTimeout(5_000) { flow.collect { values.add(it) } } }.exceptionOrNull() === failure)
        check(reads.get() == 1 && ends.get() == 1 && values.isEmpty())
        check(closes.size == 1 && (closes.single() as? SDKStreamCloseReason.Failed)?.error === failure)
    }
    println("Kotlin retained reader read failures preserve Throwable and close once without handoff")
}

// verifies: PROC-028, PROC-041
internal suspend fun checkReaderCollectorCloseReasons(owner: SDKClient) {
    for (failure in listOf(IllegalStateException("collector failed"), AssertionError("collector assertion failed"))) {
        val reads = AtomicInteger()
        val ends = AtomicInteger()
        val closes = mutableListOf<SDKStreamCloseReason>()
        val values = mutableListOf<Int>()
        val flow =
            readerFlow<Int, Unit>(
                owner = owner,
                open = { Unit },
                next = { reads.incrementAndGet() },
                end = {
                    check(currentCoroutineContext().isActive) { "reader teardown ran in a cancelled context" }
                    ends.incrementAndGet()
                    Unit
                },
                connectionState = { ConnectionState.CONNECTED },
                connectionStateChanged = { _, _ -> awaitCancellation() },
                onClose = { closes.add(it) },
                onConnectionStateChange = null,
            )
        val thrown =
            runCatching {
                withTimeout(5_000) {
                    flow.collect {
                        values.add(it)
                        throw failure
                    }
                }
            }.exceptionOrNull()
        check(thrown === failure && values == listOf(1) && reads.get() == 1 && ends.get() == 1)
        check(closes.size == 1 && (closes.single() as? SDKStreamCloseReason.Failed)?.error === failure) {
            "collector failure did not close once with the original error"
        }
    }
    for (earlyExit in listOf(false, true)) {
        val reads = AtomicInteger()
        val ends = AtomicInteger()
        val closes = mutableListOf<SDKStreamCloseReason>()
        val values = mutableListOf<Int>()
        val flow =
            readerFlow<Int, Unit>(
                owner = owner,
                open = { Unit },
                next = { if (reads.incrementAndGet() == 1) 1 else null },
                end = {
                    check(currentCoroutineContext().isActive) { "reader teardown ran in a cancelled context" }
                    ends.incrementAndGet()
                    Unit
                },
                connectionState = { ConnectionState.CONNECTED },
                connectionStateChanged = { _, _ -> awaitCancellation() },
                onClose = { closes.add(it) },
                onConnectionStateChange = null,
            )
        withTimeout(5_000) {
            if (earlyExit) values.add(flow.first()) else flow.collect { values.add(it) }
        }
        check(values == listOf(1) && reads.get() == (if (earlyExit) 1 else 2) && ends.get() == 1)
        check(closes == listOf(SDKStreamCloseReason.Closed)) { "normal collector exit did not close once" }
    }
    println("Kotlin reader collector errors keep the original Throwable; normal exits close once")
}

// verifies: PROC-028, PROC-041
internal suspend fun checkReaderCollectorBoundarySurvivesDatabaseReopen(backend: BackendOptions) =
    coroutineScope {
        withTimeout(30_000) {
            val directory = Files.createTempDirectory("kotlin-retained-reader-")
            val database = directory.resolve("client.db3").toString()
            val options =
                ClientOptions(
                    backend = BackendSource.Options(backend),
                    storage = StorageOptions(location = StorageLocation.Explicit(database, "$database-attachments")),
                    deviceSync = false,
                )
            val signer = generateLocalSigner()
            val identity = signer.identity()
            var owner = SDKClient.create(signer, options)
            try {
                val inbox = owner.inboxId()
                val group = owner.conversations().createGroup(emptyList())
                val groupId = group.id()
                val firstId = group.sendText("held collector A")
                val secondId = group.sendText("held collector B")
                val initial = owner.conversations().sdkConformanceDeliveryPosition(groupId)
                val first = group.messages().first { it.id == firstId }
                val cursor = checkNotNull(first.deliveryCursor)
                val bytes = Base64.getUrlDecoder().decode(cursor.removePrefix("dc1_"))
                check(cursor.startsWith("dc1_") && bytes.size == 24)
                val firstPosition = ByteBuffer.wrap(bytes).getLong(16).toString()
                val entered = CompletableDeferred<Unit>()
                val closes = mutableListOf<SDKStreamCloseReason>()
                val delivered = mutableListOf<MessageId>()
                val collection =
                    async {
                        owner.messages(group, onClose = { closes.add(it) }).collect {
                            delivered.add(it.id)
                            entered.complete(Unit)
                            awaitCancellation()
                        }
                    }
                try {
                    entered.await()
                    check(delivered == listOf(firstId)) { "a held collector received an extra value" }
                    check(owner.conversations().sdkConformanceDeliveryPosition(groupId) == initial) {
                        "handoff acknowledged a value before the collector returned"
                    }
                } finally {
                    withContext(NonCancellable) { collection.cancelAndJoin() }
                }
                check(closes == listOf(SDKStreamCloseReason.Closed))
                withContext(NonCancellable) { owner.end() }
                owner = SDKClient.build(identity, options, inbox)
                check(owner.conversations().sdkConformanceDeliveryPosition(groupId) == initial)
                val restored = (checkNotNull(owner.conversations().getById(groupId)) as Conversation.Group).group
                check(owner.messages(restored).first().id == firstId) {
                    "cancelled collector lost A after database reopen"
                }

                val appFailure = AssertionError("collector stops on B")
                val secondCloses = mutableListOf<SDKStreamCloseReason>()
                val consumed = mutableListOf<MessageId>()
                val thrown =
                    runCatching {
                        owner.messages(restored, onClose = { secondCloses.add(it) }).collect {
                            consumed.add(it.id)
                            if (it.id == secondId) throw appFailure
                        }
                    }.exceptionOrNull()
                check(thrown === appFailure && consumed == listOf(firstId, secondId))
                check(
                    secondCloses.size == 1 &&
                        (secondCloses.single() as? SDKStreamCloseReason.Failed)?.error === appFailure,
                ) {
                    "collector failure did not close once with the original error"
                }
                check(owner.conversations().sdkConformanceDeliveryPosition(groupId) == firstPosition) {
                    "collector failure changed the last completed acknowledgement"
                }
                withContext(NonCancellable) { owner.end() }
                owner = SDKClient.build(identity, options, inbox)
                check(owner.conversations().sdkConformanceDeliveryPosition(groupId) == firstPosition)
                val replay = (checkNotNull(owner.conversations().getById(groupId)) as Conversation.Group).group
                check(owner.messages(replay).first().id == secondId) { "failed collector lost B after database reopen" }
            } finally {
                withContext(NonCancellable) { owner.end() }
                directory.toFile().deleteRecursively()
            }
        }
        println("Kotlin retained reader collector boundary keeps cancelled and failed values after database reopen")
    }

internal suspend fun checkEndedClientCannotHandOffReaderValues(backend: BackendOptions) {
    val owner =
        SDKClient.create(
            generateLocalSigner(),
            ClientOptions(
                backend = BackendSource.Options(backend),
                storage = StorageOptions(location = StorageLocation.InMemory),
                deviceSync = false,
            ),
        )
    try {
        val group = owner.conversations().createGroup(emptyList())
        group.sendText("value owned by an ended client")
        withContext(NonCancellable) { owner.end() }
        val closes = mutableListOf<SDKStreamCloseReason>()
        val delivered = mutableListOf<MessageId>()
        val failure =
            runCatching {
                withTimeout(5_000) {
                    owner.messages(group, onClose = { closes.add(it) }).collect { delivered.add(it.id) }
                }
            }.exceptionOrNull()
        check(failure is XmtpException.ClientClosed)
        check(delivered.isEmpty() && closes.size == 1)
        check((closes.single() as? SDKStreamCloseReason.Failed)?.error === failure)
    } finally {
        withContext(NonCancellable) { owner.end() }
    }
    println("Kotlin retained ended owner returns ClientClosed without a reader handoff")
}
