package org.xmtp.android.example.messenger

import android.os.Build
import android.os.SystemClock
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import java.io.File
import java.security.SecureRandom
import kotlinx.coroutines.*
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.xmtp.android.example.ExampleApp
import uniffi.xmtp_sdk.*

/** Opt in on the fixed Linux emulator. This test creates actual SDK messages. */
@RunWith(AndroidJUnit4::class)
class MessengerPerformanceInstrumentedTest {
    private val instrumentation get() = InstrumentationRegistry.getInstrumentation()
    private val context get() = instrumentation.targetContext.applicationContext
    private val arguments get() = InstrumentationRegistry.getArguments()
    private val root get() = File(context.filesDir, "messenger-performance")
    private val manifest get() = File(root, "workload.json")
    private val secrets by lazy { SecureSecretStore(context) }

    private fun options(label: String) = ClientOptions(
        backend = BackendSource.Options(BackendOptions(url = checkNotNull(arguments.getString("performanceBackendUrl")))),
        storage = StorageOptions(location = StorageLocation.Explicit(File(root, "$label.db3").absolutePath, File(root, "$label-attachments").absolutePath), encryptionKey = checkNotNull(secrets.read("performance", "$label-database-key"))),
        deviceSync = false,
        allowOffline = true,
    )

    private fun expected(index: Int) = when {
        index == 0 -> 50_000
        index < 10 -> 1_000
        index < 420 -> 42
        else -> 41
    }

    private suspend fun seed(): JSONObject {
        root.mkdirs()
        val started = SystemClock.elapsedRealtime()
        val receiverKey = SecureRandom().generateSeed(32)
        val senderKey = SecureRandom().generateSeed(32)
        secrets.write("performance", "receiver", receiverKey)
        secrets.write("performance", "sender", senderKey)
        secrets.write("performance", "receiver-database-key", SecureRandom().generateSeed(32))
        secrets.write("performance", "sender-database-key", SecureRandom().generateSeed(32))
        val receiver = SDKClient.create(context, localSignerFromPrivateKey(receiverKey), options("receiver"))
        var sender: SDKClient? = null
        try {
            sender = SDKClient.create(context, localSignerFromPrivateKey(senderKey), options("sender"))
            val peer = checkNotNull(sender)
            val ids = JSONArray()
            var total = 0
            for (index in 0 until 1_000) {
                val group = peer.conversations.createGroup(listOf(receiver.inboxId()), CreateGroupOptions(name = "Performance $index"))
                receiver.conversations.sync()
                val local = checkNotNull(receiver.conversations.getById(group.id()))
                local.updateConsentState(ConsentState.ALLOWED)
                for (row in 0 until expected(index)) {
                    // Each row has a distinct body. All rows pass through the public codec.
                    // A publish batch can share a timestamp. The newest 100 heavy rows
                    // use separate SDK publications for the two measured 50-row pages.
                    if (index == 0 && row == 49_900) group.publishMessages()
                    val optimistic = index != 0 || row < 49_900
                    group.sendText("$index/$row " + "m".repeat(256), SendOptions(optimistic = optimistic))
                    if ((row + 1) % 256 == 0) group.publishMessages()
                }
                group.publishMessages()
                // Fixture preparation may sync. Measured screen reads never do.
                local.sync()
                assertEquals(expected(index).toULong(), local.countMessages(publishedSelection()))
                ids.put(group.id())
                total += expected(index)
                println("MESSENGER_PERFORMANCE_SEED groups=${index + 1} messages=$total elapsedMs=${SystemClock.elapsedRealtime() - started}")
            }
            assertEquals(100_000, total)
            return JSONObject().put("receiverInbox", receiver.inboxId()).put("ids", ids)
                .put("seedMs", SystemClock.elapsedRealtime() - started).put("messages", total)
                .also { manifest.writeText(it.toString()) }
        } finally {
            withContext(NonCancellable) { sender?.end(); receiver.end() }
        }
    }

    private suspend fun gcHeap(): Long {
        repeat(3) { Runtime.getRuntime().gc(); System.runFinalization(); delay(100) }
        return Runtime.getRuntime().totalMemory() - Runtime.getRuntime().freeMemory()
    }

    private suspend fun measured(block: suspend () -> Unit): Double {
        val started = SystemClock.elapsedRealtimeNanos()
        block()
        return (SystemClock.elapsedRealtimeNanos() - started) / 1_000_000.0
    }

    private fun p95(samples: List<Double>) = samples.sorted()[28]

    @Test fun fixedWorkloadMeetsLocalQueryAndManagedHeapBudgets() = runBlocking {
        assumeTrue("Use the explicit performance recipe", arguments.getString("messengerPerformance") == "true")
        assertEquals(34, Build.VERSION.SDK_INT)
        assertEquals("x86_64", Build.SUPPORTED_ABIS.first())
        assertTrue(Build.HARDWARE == "ranchu" || Build.HARDWARE == "goldfish")
        assertEquals(4, Runtime.getRuntime().availableProcessors())
        val memoryKb = File("/proc/meminfo").readLines().first { it.startsWith("MemTotal:") }.split(Regex("\\s+"))[1].toLong()
        assertTrue("The guest must have 4 GiB RAM", memoryKb in 3_800_000L..4_300_000L)
        val previousLifecycle = AndroidStreamLifecycle.enabled
        AndroidStreamLifecycle.enabled = false
        var client: SDKClient? = null
        try {
            val workload = if (manifest.exists()) JSONObject(manifest.readText()) else seed()
            val signer = localSignerFromPrivateKey(checkNotNull(secrets.read("performance", "receiver")))
            client = SDKClient.build(context, signer.identity(), options("receiver"), workload.getString("receiverInbox"))
            val owner = checkNotNull(client)
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
            assertEquals(1_000, owner.conversations.list(ListConversationsOptions(limit = 1_001u, consentStates = listOf(ConsentState.ALLOWED))).size)
            val heavy = checkNotNull(owner.conversations.getById(ids.getString(0)))
            val own = owner.inboxId()
            val viewModel = withContext(Dispatchers.Main) { MessengerViewModel(context as ExampleApp) }
            val profile = BackendProfile("performance", checkNotNull(arguments.getString("performanceBackendUrl")))
            val active = ActiveSession(SessionKey(profile.id, 0), profile, profile.paths(context.filesDir), owner)
            val firstSamples = mutableListOf<Double>()
            val olderSamples = mutableListOf<Double>()
            val listSamples = mutableListOf<Double>()
            val tieRuns = JSONArray()
            var run = 0
            while (firstSamples.size < 30 && run < 200) {
                lateinit var first: BucketPage<Message>
                val firstMs = measured { first = viewModel.performancePage(heavy, null); first.rows.map { it.toRow(own) } }
                lateinit var older: BucketPage<Message>
                val olderMs = measured { older = viewModel.performancePage(heavy, first.nextBeforeNs); older.rows.map { it.toRow(own) } }
                assertNull(first.notice); assertNull(older.notice)
                assertTrue(first.rows.isNotEmpty()); assertTrue(older.rows.isNotEmpty())
                val listMs = measured {
                    val rows = viewModel.performanceList(active)
                    assertEquals(50, rows.size)
                    assertTrue(rows.all { it.title.isNotBlank() && it.preview.isNotBlank() })
                }
                // Complete timestamp buckets can require a larger query. Keep them separate.
                if (first.rows.size != 50 || older.rows.size != 50) {
                    tieRuns.put(JSONObject().put("run", run).put("firstMs", firstMs).put("olderMs", olderMs)
                        .put("firstRows", first.rows.size).put("olderRows", older.rows.size))
                } else if (run >= 5) { firstSamples += firstMs; olderSamples += olderMs }
                if (run >= 5 && listSamples.size < 30) listSamples += listMs
                run += 1
            }
            viewModel.performanceClearCache()
            val baseline = gcHeap()
            var maxRows = 0
            var maxCacheRows = 0
            var maxPageRows = 0
            for (index in 0 until 10) {
                val chat = checkNotNull(owner.conversations.getById(ids.getString(index)))
                var before: Long? = null
                var loaded = 0
                do {
                    val page = viewModel.performancePage(chat, before)
                    maxPageRows = maxOf(maxPageRows, page.rows.size)
                    assertNull(page.notice)
                    assertTrue(page.rows.isNotEmpty())
                    before = page.nextBeforeNs
                    loaded += page.rows.size
                    val retained = viewModel.performanceRetain(chat.id(), page.rows)
                    retained.map { it.toRow(own) }
                    maxRows = maxOf(maxRows, retained.size)
                    maxCacheRows = maxOf(maxCacheRows, viewModel.performanceCacheRows((0 until 10).map { ids.getString(it) }))
                } while (loaded < 1_000)
            }
            val heapDelta = gcHeap() - baseline
            val report = JSONObject().put("api", Build.VERSION.SDK_INT).put("abi", Build.SUPPORTED_ABIS.first())
                .put("hardware", Build.HARDWARE).put("cores", Runtime.getRuntime().availableProcessors()).put("memoryKb", memoryKb)
                .put("sdkVersion", sdkVersion()).put("groups", 1_000).put("messages", actual.toString())
                .put("heavyMessages", 50_000).put("bodyAsciiBytes", 256).put("warmups", 5).put("measuredRuns", 30)
                .put("seedMs", workload.getLong("seedMs")).put("firstMs", JSONArray(firstSamples)).put("olderMs", JSONArray(olderSamples))
                .put("listMs", JSONArray(listSamples)).put("tieRuns", tieRuns).put("heapDeltaBytes", heapDelta)
                .put("maxTranscriptRows", maxRows).put("maxCacheRows", maxCacheRows).put("maxPageRows", maxPageRows)
            File(root, "result.json").writeText(report.toString(2))
            println("MESSENGER_PERFORMANCE_RESULT $report")
            assertEquals("Thirty runs outside tie retries are required", 30, firstSamples.size)
            assertEquals(30, olderSamples.size)
            assertTrue("First transcript p95 exceeds 300 ms", p95(firstSamples) <= 300)
            assertTrue("Older transcript p95 exceeds 250 ms", p95(olderSamples) <= 250)
            assertTrue("First list p95 exceeds 1000 ms", p95(listSamples) <= 1_000)
            assertTrue("Ten transcripts exceed 64 MiB heap delta", heapDelta <= 64L * 1024 * 1024)
            assertTrue("Transcript row cap was removed", maxRows <= 500)
            assertTrue("Published page row cap was removed", maxPageRows <= 500)
            assertTrue("Transcript cache trimming was removed", maxCacheRows <= 1_500)
        } finally {
            withContext(NonCancellable) { client?.end() }
            AndroidStreamLifecycle.enabled = previousLifecycle
        }
    }
}
