package org.xmtp.android.example.messenger

import android.os.Build
import android.os.SystemClock
import androidx.lifecycle.ViewModelStore
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.first
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.xmtp.android.example.ExampleApp
import org.xmtp.android.example.shared.MessageRow
import uniffi.xmtp_sdk.*
import java.io.File
import java.security.SecureRandom
import java.util.UUID
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicLong

/** Opt in on the fixed Linux emulator. This test creates actual SDK messages. */
@RunWith(AndroidJUnit4::class)
class MessengerPerformanceInstrumentedTest {
    private val instrumentation get() = InstrumentationRegistry.getInstrumentation()
    private val context get() = instrumentation.targetContext.applicationContext
    private val arguments get() = InstrumentationRegistry.getArguments()
    private val root get() = File(context.filesDir, "messenger-performance")
    private val manifest get() = File(root, "workload.json")
    private val seedProgress get() = File(root, "seed-progress.jsonl")
    private val secrets by lazy { SecureSecretStore(context) }

    private val profile get() =
        BackendProfile(
            "performance",
            checkNotNull(arguments.getString("performanceBackendUrl")),
        )

    private fun options(label: String) =
        ClientOptions(
            backend =
                BackendSource.Options(
                    BackendOptions(url = checkNotNull(arguments.getString("performanceBackendUrl"))),
                ),
            storage =
                StorageOptions(
                    location =
                        if (label == "receiver") {
                            val paths = profile.paths(context.filesDir)
                            paths.database.parentFile!!.mkdirs()
                            StorageLocation.Explicit(paths.database.absolutePath, paths.attachments.absolutePath)
                        } else {
                            StorageLocation.Explicit(
                                File(root, "$label.db3").absolutePath,
                                File(root, "$label-attachments").absolutePath,
                            )
                        },
                    encryptionKey =
                        checkNotNull(
                            secrets.read(
                                "performance",
                                if (label ==
                                    "receiver"
                                ) {
                                    "database-key"
                                } else {
                                    "$label-database-key"
                                },
                            ),
                        ),
                ),
            deviceSync = false,
            allowOffline = false,
        )

    private fun expected(index: Int) =
        when {
            index == 0 -> 50_000
            index < 10 -> 1_000
            index < 420 -> 42
            else -> 41
        }

    private suspend fun seed(): JSONObject {
        root.mkdirs()
        seedProgress.writeText("")
        val started = SystemClock.elapsedRealtime()
        val receiverKey = SecureRandom().generateSeed(32)
        val senderKey = SecureRandom().generateSeed(32)
        secrets.write("performance", "wallet", receiverKey)
        secrets.write("performance", "sender", senderKey)
        secrets.write("performance", "database-key", SecureRandom().generateSeed(32))
        secrets.write("performance", "sender-database-key", SecureRandom().generateSeed(32))
        val receiver = SDKClient.create(context, localSignerFromPrivateKey(receiverKey), options("receiver"))
        var sender: SDKClient? = null
        try {
            sender = SDKClient.create(context, localSignerFromPrivateKey(senderKey), options("sender"))
            val peer = checkNotNull(sender)
            val ids = JSONArray()
            var total = 0
            for (index in 0 until 1_000) {
                val group =
                    peer.conversations.createGroup(
                        listOf(receiver.inboxId()),
                        CreateGroupOptions(name = "Performance $index"),
                    )
                receiver.conversations.sync()
                val local = checkNotNull(receiver.conversations.getById(group.id()))
                local.updateConsentState(ConsentState.ALLOWED)
                var published = 0
                var sendStarted = SystemClock.elapsedRealtime()

                suspend fun checkpoint(
                    rows: Int,
                    phase: String,
                    publish: Boolean,
                ) {
                    val publishStarted = SystemClock.elapsedRealtime()
                    val record =
                        JSONObject()
                            .put("phase", phase)
                            .put("group", index)
                            .put("expectedPublished", rows)
                            .put("pendingApplicationRows", rows - published)
                            .put("sendMs", publishStarted - sendStarted)
                            .put("operation", "publish")
                    try {
                        check(rows - published in 0..256) { "Seed publication backlog exceeded 256 texts" }
                        if (publish) group.publishMessages()
                        val syncStarted = SystemClock.elapsedRealtime()
                        record.put("publishMs", syncStarted - publishStarted)
                        record.put("operation", "sync")
                        val received =
                            verifySeedCheckpoint(
                                rows.toULong(),
                                sync = {
                                    try {
                                        local.sync()
                                    } finally {
                                        record.put("syncMs", SystemClock.elapsedRealtime() - syncStarted)
                                    }
                                },
                                count = {
                                    val countStarted = SystemClock.elapsedRealtime()
                                    record.put("operation", "count")
                                    try {
                                        local.countMessages(publishedSelection()).also {
                                            record.put("receiverPublished", it.toString())
                                        }
                                    } finally {
                                        record.put("countMs", SystemClock.elapsedRealtime() - countStarted)
                                    }
                                },
                            )
                        record.put("receiverPublished", received.toString()).put("status", "complete")
                        published = rows
                    } catch (error: Throwable) {
                        record.put("status", "failed").put("errorType", error.javaClass.simpleName)
                        val details = (error as? XmtpException.Unknown)?.v1?.streamFailure
                        if (details != null) {
                            record.put("code", details.code).put("retryable", details.retryable)
                            val barriers = JSONArray()
                            for (barrier in details.barriers) {
                                for (topic in barrier.unfinished) {
                                    barriers.put(
                                        JSONObject()
                                            .put("reason", barrier.reason.name)
                                            .put("target", topic.target?.toString())
                                            .put("received", topic.received.toString())
                                            .put("processed", topic.processed.toString())
                                            .put("cause", topic.cause?.kind?.name),
                                    )
                                }
                            }
                            record.put("barriers", barriers)
                        }
                        throw error
                    } finally {
                        record.put("elapsedMs", SystemClock.elapsedRealtime() - started)
                        record.put("checkpointMs", SystemClock.elapsedRealtime() - publishStarted)
                        seedProgress.appendText("$record\n")
                        println("MESSENGER_PERFORMANCE_SEED checkpoint=$record")
                    }
                    sendStarted = SystemClock.elapsedRealtime()
                }
                for (row in 0 until expected(index)) {
                    // Each row has a distinct body. All rows pass through the public codec.
                    // A publish batch can share a timestamp. The newest 100 heavy rows
                    // use separate SDK publications for the two measured 50-row pages.
                    if (index == 0 && row == 49_900) checkpoint(row, "heavy-direct-flush", publish = true)
                    val optimistic = index != 0 || row < 49_900
                    group.sendText("$index/$row " + "m".repeat(256), SendOptions(optimistic = optimistic))
                    if (!optimistic) {
                        checkpoint(row + 1, "heavy-direct", publish = (row + 1) % 256 == 0)
                    } else if ((row + 1) % 256 == 0) {
                        checkpoint(row + 1, "batch", publish = true)
                        if (index == 0 && (row + 1) % 4096 == 0) {
                            println(
                                "MESSENGER_PERFORMANCE_SEED heavyMessages=${row + 1} " +
                                    "elapsedMs=${SystemClock.elapsedRealtime() - started}",
                            )
                        }
                    }
                }
                // Fixture preparation may sync. Measured screen reads never do.
                checkpoint(expected(index), "group-tail", publish = true)
                ids.put(group.id())
                total += expected(index)
                println(
                    "MESSENGER_PERFORMANCE_SEED groups=${index + 1} messages=$total " +
                        "elapsedMs=${SystemClock.elapsedRealtime() - started}",
                )
            }
            assertEquals(100_000, total)
            // Drain fixture delivery before normal AppSession opens its default collector.
            // The real app collectors and session guards remain enabled after restore.
            coroutineScope {
                val drained = CompletableDeferred<Unit>()
                var received = 0
                val reader =
                    launch {
                        receiver.conversations
                            .streamAllMessages(
                                MessageStreamOptions(consentStates = listOf(ConsentState.ALLOWED)),
                            ).collect {
                                if (it.standardContent() !is MessageContent.Text) return@collect
                                received += 1
                                if (received == total) drained.complete(Unit)
                                if (received % 4096 == 0) {
                                    println(
                                        "MESSENGER_PERFORMANCE_SEED drainedMessages=$received " +
                                            "elapsedMs=${SystemClock.elapsedRealtime() - started}",
                                    )
                                }
                            }
                    }
                try {
                    drained.await()
                    // Let the sequential collector request its next read and acknowledge its tail.
                    delay(250)
                } finally {
                    reader.cancelAndJoin()
                }
            }
            return JSONObject()
                .put(
                    "workloadId",
                    UUID.randomUUID().toString(),
                ).put("receiverInbox", receiver.inboxId())
                .put("ids", ids)
                .put("receiverIdentity", receiver.identity().identifier)
                .put("backend", profile.backend)
                .put("seedMs", SystemClock.elapsedRealtime() - started)
                .put("messages", total)
                .also { manifest.writeText(it.toString()) }
        } finally {
            var failure: Throwable? = null

            suspend fun attempt(step: suspend () -> Unit) {
                try {
                    step()
                } catch (error: Throwable) {
                    if (failure == null) {
                        failure = error
                    } else if (failure !== error) {
                        failure?.addSuppressed(error)
                    }
                }
            }
            withContext(NonCancellable) {
                attempt { sender?.end() }
                attempt { receiver.end() }
            }
            failure?.let { throw it }
        }
    }

    private suspend fun gcHeap(): Long {
        repeat(3) {
            Runtime.getRuntime().gc()
            System.runFinalization()
            delay(100)
        }
        return Runtime.getRuntime().totalMemory() - Runtime.getRuntime().freeMemory()
    }

    private suspend fun measured(block: suspend () -> Unit): Double {
        val started = SystemClock.elapsedRealtimeNanos()
        block()
        return (SystemClock.elapsedRealtimeNanos() - started) / 1_000_000.0
    }

    private fun p95(samples: List<Double>) = samples.sorted()[28]

    @Test fun fixedWorkloadMeetsLocalQueryAndManagedHeapBudgets() =
        runBlocking {
            assumeTrue("Use the explicit performance recipe", arguments.getString("messengerPerformance") == "true")
            assertEquals(34, Build.VERSION.SDK_INT)
            assertEquals("x86_64", Build.SUPPORTED_ABIS.first())
            assertTrue(Build.HARDWARE == "ranchu" || Build.HARDWARE == "goldfish")
            assertEquals(4, Runtime.getRuntime().availableProcessors())
            val memoryKb =
                File(
                    "/proc/meminfo",
                ).readLines().first { it.startsWith("MemTotal:") }.split(Regex("\\s+"))[1].toLong()
            assertTrue("The guest must have 4 GiB RAM", memoryKb in 3_800_000L..4_300_000L)
            val previousLifecycle = AndroidStreamLifecycle.enabled
            AndroidStreamLifecycle.enabled = false
            val application = context as ExampleApp
            val session = application.session
            val store = ViewModelStore()
            try {
                session.signOut()
                val workload = if (manifest.exists()) JSONObject(manifest.readText()) else seed()
                assertEquals(profile.backend, workload.getString("backend"))
                session.preferences.setActive(
                    profile.copy(
                        inboxId = workload.getString("receiverInbox"),
                        identity = workload.getString("receiverIdentity"),
                    ),
                )
                session.preferences.setSignedIn(true)
                val replayRows = AtomicInteger()
                val replayHandling = AtomicInteger()
                val lastReplayFinished = AtomicLong(SystemClock.elapsedRealtimeNanos())
                val viewModel =
                    withContext(Dispatchers.Main) {
                        MessengerViewModel(application).also {
                            store.put("performance", it)
                            val productionHandler = session.onMessage
                            session.onMessage = { owner, message ->
                                if (message.standardContent() is MessageContent.Text) replayRows.incrementAndGet()
                                replayHandling.incrementAndGet()
                                try {
                                    productionHandler(owner, message)
                                } finally {
                                    lastReplayFinished.set(SystemClock.elapsedRealtimeNanos())
                                    replayHandling.decrementAndGet()
                                }
                            }
                        }
                    }
                // Normal restore owns the client and activates its generation guard.
                val active = withTimeout(120_000) { session.active.filterNotNull().first() }
                assertTrue(session.accepts(active.key))
                withTimeout(120_000) { viewModel.state.first { it.conversations.size == 50 } }
                // Verify the fixture drain through the real app callback before timing.
                lastReplayFinished.set(SystemClock.elapsedRealtimeNanos())
                withTimeout(30_000) {
                    while (replayHandling.get() != 0 ||
                        SystemClock.elapsedRealtimeNanos() - lastReplayFinished.get() < 1_000_000_000L
                    ) {
                        assertTrue("Fixture seed backlog was not drained", replayRows.get() <= 1)
                        delay(50)
                    }
                }
                val startupReplayRows = replayRows.get()
                assertTrue("Fixture seed backlog was not drained", startupReplayRows <= 1)
                val owner = active.client
                val ids = workload.getJSONArray("ids")
                assertEquals(1_000, ids.length())
                // Validate every retained count before accepting a reused dataset.
                var actual = 0uL
                for (index in 0 until ids.length()) {
                    val chat = checkNotNull(owner.conversations.getById(ids.getString(index)))
                    val count = chat.countMessages(publishedSelection())
                    assertEquals(expected(index).toULong(), count)
                    actual += count
                }
                assertEquals(100_000uL, actual)
                assertEquals(
                    1_000,
                    owner.conversations
                        .list(
                            ListConversationsOptions(limit = 1_001u, consentStates = listOf(ConsentState.ALLOWED)),
                        ).size,
                )
                val heavy = checkNotNull(owner.conversations.getById(ids.getString(0)))
                val own = owner.inboxId()
                var maxHistoryReadRows = 0L
                val productionRead = viewModel.historyPageRead
                viewModel.historyPageRead = { chat, selection, before, after ->
                    assertTrue("Published SDK page request exceeds 50 rows", checkNotNull(selection.limit) <= 50u)
                    productionRead(chat, selection, before, after).also {
                        maxHistoryReadRows =
                            maxOf(maxHistoryReadRows, it.messages.size.toLong() + it.skippedCount.toLong())
                    }
                }
                assertEquals("Seed replay resumed before measurement", startupReplayRows, replayRows.get())
                assertEquals("A seed callback is still running", 0, replayHandling.get())
                val firstSamples = mutableListOf<Double>()
                val olderSamples = mutableListOf<Double>()
                val listSamples = mutableListOf<Double>()
                val tieRuns = JSONArray()
                var run = 0
                while (firstSamples.size < 30 && run < 200) {
                    lateinit var first: HistoryWindow<Message>
                    val firstMs =
                        measured {
                            first = viewModel.performancePage(heavy, null)
                            first.rows.map { it.toRow(own) }
                        }
                    lateinit var older: HistoryWindow<Message>
                    val olderMs =
                        measured {
                            older = viewModel.performancePage(heavy, checkNotNull(first.last))
                            older.rows.map { it.toRow(own) }
                        }
                    assertNull(first.notice)
                    assertNull(older.notice)
                    assertTrue(first.rows.isNotEmpty())
                    assertTrue(older.rows.isNotEmpty())
                    val listMs =
                        measured {
                            val rows = viewModel.performanceList(active)
                            assertEquals(50, rows.size)
                            assertTrue(
                                rows.all {
                                    it.title.isNotBlank() && it.preview.isNotBlank() &&
                                        it.unread.toULong() > 0uL
                                },
                            )
                        }
                    // Record nonordinary pages separately from the required 50-row samples.
                    if (first.rows.size != 50 || older.rows.size != 50) {
                        tieRuns.put(
                            JSONObject()
                                .put("run", run)
                                .put("firstMs", firstMs)
                                .put("olderMs", olderMs)
                                .put("firstRows", first.rows.size)
                                .put("olderRows", older.rows.size),
                        )
                    } else if (run >= 5) {
                        firstSamples += firstMs
                        olderSamples += olderMs
                    }
                    if (run >= 5 && listSamples.size < 30) listSamples += listMs
                    run += 1
                }
                viewModel.performanceClearCache()
                val baseline = gcHeap()
                var maxRows = 0
                var maxCacheRows = 0
                var maxPageRows = 0
                var maxCacheTranscripts = 0
                var currentRows = emptyList<MessageRow>()
                for (index in 0 until 10) {
                    val chat = checkNotNull(owner.conversations.getById(ids.getString(index)))
                    var before: MessageHistoryPosition? = null
                    var loaded = 0
                    do {
                        val page = viewModel.performancePage(chat, before)
                        maxPageRows = maxOf(maxPageRows, page.rows.size)
                        assertNull(page.notice)
                        assertTrue(page.rows.isNotEmpty())
                        before = page.last
                        loaded += page.rows.size
                        val retained = viewModel.performanceRetain(chat.id(), page)
                        currentRows = retained.map { it.toRow(own) }
                        maxRows = maxOf(maxRows, retained.size)
                        val visitedIds = (0 until 10).map { ids.getString(it) }
                        maxCacheRows = maxOf(maxCacheRows, viewModel.performanceCacheRows(visitedIds))
                        maxCacheTranscripts =
                            maxOf(
                                maxCacheTranscripts,
                                visitedIds.count { viewModel.performanceCacheRows(listOf(it)) > 0 },
                            )
                        assertNotNull("SDK history continuation is missing", before)
                    } while (loaded < 1_000)
                }
                val heapAfter = gcHeap()
                val heapDelta = (heapAfter - baseline).coerceAtLeast(0)
                assertEquals(500, currentRows.size)
                val report =
                    JSONObject()
                        .put("api", Build.VERSION.SDK_INT)
                        .put("abi", Build.SUPPORTED_ABIS.first())
                        .put(
                            "hardware",
                            Build.HARDWARE,
                        ).put("cores", Runtime.getRuntime().availableProcessors())
                        .put("memoryKb", memoryKb)
                        .put("sdkVersion", sdkVersion())
                        .put("groups", 1_000)
                        .put("messages", actual.toString())
                        .put("heavyMessages", 50_000)
                        .put("bodyAsciiBytes", 256)
                        .put("warmups", 5)
                        .put("measuredRuns", 30)
                        .put(
                            "startupReplayRows",
                            startupReplayRows,
                        ).put("replayRowsAfterMeasurements", replayRows.get())
                        .put(
                            "workloadId",
                            workload.getString("workloadId"),
                        ).put("visitedTranscripts", 10)
                        .put("maxCacheTranscripts", maxCacheTranscripts)
                        .put(
                            "seedMs",
                            workload.getLong("seedMs"),
                        ).put("firstMs", JSONArray(firstSamples))
                        .put("olderMs", JSONArray(olderSamples))
                        .put("listMs", JSONArray(listSamples))
                        .put("tieRuns", tieRuns)
                        .put("heapDeltaBytes", heapDelta)
                        .put("maxTranscriptRows", maxRows)
                        .put("maxCacheRows", maxCacheRows)
                        .put("maxPageRows", maxPageRows)
                        .put("maxHistoryReadRows", maxHistoryReadRows)
                        .put("heapBeforeBytes", baseline)
                        .put("heapAfterBytes", heapAfter)
                File(root, "result.json").writeText(report.toString(2))
                println("MESSENGER_PERFORMANCE_RESULT $report")
                assertEquals("Seed replay overlapped measurement", startupReplayRows, replayRows.get())
                assertEquals("Thirty runs outside tie retries are required", 30, firstSamples.size)
                assertEquals(30, olderSamples.size)
                assertTrue("First transcript p95 exceeds 300 ms", p95(firstSamples) <= 300)
                assertTrue("Older transcript p95 exceeds 250 ms", p95(olderSamples) <= 250)
                assertTrue("First list p95 exceeds 1000 ms", p95(listSamples) <= 1_000)
                assertTrue("Ten transcripts exceed 64 MiB heap delta", heapDelta <= 64L * 1024 * 1024)
                assertTrue("Transcript row cap was removed", maxRows <= 500)
                assertTrue("Published page row cap was removed", maxPageRows <= 500)
                assertTrue("Published SDK page read bound was removed", maxHistoryReadRows <= 50)
                assertTrue("Transcript cache trimming was removed", maxCacheRows <= 1_500 && maxCacheTranscripts <= 3)
            } finally {
                var failure: Throwable? = null

                suspend fun attempt(step: suspend () -> Unit) {
                    try {
                        step()
                    } catch (error: Throwable) {
                        if (failure == null) {
                            failure = error
                        } else if (failure !== error) {
                            failure?.addSuppressed(error)
                        }
                    }
                }
                try {
                    withContext(NonCancellable) {
                        attempt { withContext(Dispatchers.Main) { store.clear() } }
                        attempt { session.onMessage = { _, _ -> } }
                        attempt { session.onInvalidated = {} }
                        attempt { session.signOut() }
                    }
                    failure?.let { throw it }
                } finally {
                    AndroidStreamLifecycle.enabled = previousLifecycle
                }
            }
        }
}
