package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Exercises the device-sync archive round trip over **TLS**.
 *
 * The rest of the instrumented suite runs against [XMTPEnvironment.LOCAL], whose history sync
 * URL is `http://10.0.2.2:5558` — plain HTTP. No TLS handshake happens there, so nothing in CI
 * ever built a TLS connection on the archive path. That is why the
 * `Expect rustls-platform-verifier to be initialized` abort reached a device instead of a build:
 * it can only fire once reqwest actually negotiates TLS, which only happens on dev/production.
 *
 * This test pins the clients to [XMTPEnvironment.DEV], so the upload
 * (`xmtp_archive::exporter::post_to_url`) and the download (the device-sync worker) both go over
 * https to `message-history.dev.ephemera.network`.
 *
 * Opt-in: it needs network access and a reachable dev history server, so it stays out of the PR
 * gate. Run it with:
 *
 * ```
 * ./gradlew :library:connectedAndroidTest \
 *   -Pandroid.testInstrumentationRunnerArguments.class=org.xmtp.android.library.HistorySyncTlsTest \
 *   -Pandroid.testInstrumentationRunnerArguments.devTls=true
 * ```
 *
 * From Android Studio, add `devTls=true` under Run > Edit Configurations > Instrumentation
 * arguments. Without the flag the test reports as skipped rather than failing.
 */
@RunWith(AndroidJUnit4::class)
class HistorySyncTlsTest : BaseInstrumentedTest() {
    private val devApi = ClientOptions.Api(env = XMTPEnvironment.DEV)

    @Before
    override fun setUp() {
        super.setUp()
        val optedIn = InstrumentationRegistry.getArguments().getString("devTls") == "true"
        assumeTrue(
            "Set -Pandroid.testInstrumentationRunnerArguments.devTls=true to run " +
                "(requires network access and a reachable dev history server)",
            optedIn,
        )
    }

    /**
     * Before the Android TLS fix this does not fail an assertion — it aborts the whole
     * instrumentation process, and the run is reported as "Process crashed" / "Test run failed
     * to complete". Check logcat for the abort message to confirm you reproduced *this* bug and
     * not an unrelated network failure.
     */
    @Test
    fun testArchiveRoundTripOverTls() =
        runBlocking {
            val wallet = createWallet()
            val client1 = createClient(wallet, api = devApi)
            val boClient = createClient(createWallet(), api = devApi)

            val group = client1.conversations.newGroup(listOf(boClient.inboxId))
            val msgFromClient1 = group.send("hello over TLS")

            delay(1000)
            // Second installation of the same wallet — the archive recipient.
            val client2 = createClient(wallet, api = devApi)
            delay(1000)

            client1.syncAllDeviceSyncGroups()

            // Upload leg: POSTs the archive to the https history server.
            client1.sendSyncArchive(pin = ARCHIVE_PIN)
            delay(2000)

            client2.syncAllDeviceSyncGroups()
            client2.listAvailableArchives(daysCutoff = 7)

            // Download leg: the device-sync worker GETs the archive over the same TLS path.
            // This is the exact call that crash-looped on device.
            processSyncArchiveWhenAvailable(client2)
            client2.conversations.syncAllConversations()

            val group2 =
                client2.conversations.findGroup(group.id)
                    ?: throw AssertionError("Archive did not deliver group with ID: ${group.id}")
            assertTrue(group2.messages().any { it.id == msgFromClient1 })
        }

    /**
     * The history server is read-after-write eventual: a GET issued moments after the upload can
     * come back 404 even though the object lands shortly after. Only a 404 is retried — any other
     * failure is rethrown immediately so real breakage still fails fast.
     */
    private suspend fun processSyncArchiveWhenAvailable(
        client: Client,
        timeoutMs: Long = 60_000,
        intervalMs: Long = 2_000,
    ) {
        val deadline = System.currentTimeMillis() + timeoutMs
        while (true) {
            try {
                client.processSyncArchive(ARCHIVE_PIN)
                return
            } catch (e: Exception) {
                val notYetUploaded = e.message?.contains("404 Not Found") == true
                if (!notYetUploaded || System.currentTimeMillis() >= deadline) throw e
                delay(intervalMs)
            }
        }
    }

    private companion object {
        const val ARCHIVE_PIN = "123"
    }
}
