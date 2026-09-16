package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.xmtp.android.library.messages.PrivateKeyBuilder

/**
 * Spec 006 CFG-106 for Android: one test reads every field of
 * [Client.serverConfiguration], one calls [Client.fetchServerConfiguration]
 * against the shared backend.
 *
 * The shared stack runs `dev/backend/local.toml`, so the identifier, the two
 * query limits, and the single anvil chain are asserted exactly. Every other
 * published value is a compiled default that an operator may retune, so those
 * are asserted as read and in range.
 */
@RunWith(AndroidJUnit4::class)
class ServerConfigurationTest : BaseInstrumentedTest() {
    /** CFG-080: the build snapshot, field by field. */
    @Test
    fun testServerConfigurationExposesEveryField() {
        val client = runBlocking { createClient(PrivateKeyBuilder()) }
        val configuration = client.serverConfiguration()

        assertEquals("org.xmtp.local", configuration.identifier)
        assertTrue(
            "server version should be published: ${configuration.serverVersion}",
            configuration.serverVersion.isNotBlank(),
        )
        // local.toml publishes no minimum, which leaves the deployment open to
        // every client version (CFG-005).
        assertEquals("", configuration.minLibxmtpVersion)

        val auth = configuration.auth
        assertFalse(auth.enabled)
        assertTrue(auth.keys.isEmpty())
        assertTrue(auth.audiences.isEmpty())
        assertTrue(auth.issuers.isEmpty())
        assertTrue(auth.requiredScopes.isEmpty())

        val retention = configuration.retention
        assertTrue(retention.groupMessageSeconds > 0L)
        assertTrue(retention.welcomeSeconds > 0L)
        assertTrue(retention.keyPackageSeconds > 0L)

        val limits = configuration.limits
        // The two values local.toml overrides.
        assertEquals(50L, limits.maxQueryLimit)
        assertEquals(50L, limits.defaultQueryLimit)
        assertTrue(limits.maxEnvelopeBytes > 0L)
        assertTrue(limits.maxRequestBytes > 0L)
        assertTrue(limits.maxResponseBytes > 0L)
        assertTrue(limits.maxPublishTopics > 0L)
        assertTrue(limits.maxQueryTopics > 0L)
        assertTrue(limits.maxNewestMetadataTopics > 0L)
        assertTrue(limits.maxNewestFullTopics > 0L)
        assertTrue(limits.maxUpdateAdds > 0L)
        assertTrue(limits.maxUpdateRemoves > 0L)
        assertTrue(limits.maxStreamTopics > 0L)
        assertTrue(limits.maxStaticTopics > 0L)
        assertTrue(limits.maxLookupIdentifiers > 0L)
        assertTrue(limits.maxScwSignatures > 0L)
        assertTrue(limits.maxIdentityEntries > 0L)
        assertTrue(limits.maxUpdateFramesPerSecond > 0)
        assertTrue(limits.maxUpdateBurst > 0)
        assertTrue(limits.maxPingFramesPerSecond > 0)
        assertTrue(limits.maxPingBurst > 0)

        val mls = configuration.mls
        assertTrue(mls.maxGroupMembers > 0L)
        assertTrue(mls.maxInstallationsPerInbox > 0L)
        // `[mls]` is absent from local.toml, so the flag reads as the published
        // default rather than as absent.
        assertEquals(true, mls.commitLogEnabled)

        // The one chain the local stack verifies, anvil.
        assertEquals(listOf("eip155:31337"), configuration.smartContractWalletChains)
    }

    /** CFG-081: the same values, with no database and no client. */
    @Test
    fun testFetchServerConfigurationWithoutAClient() {
        val fetched = runBlocking { Client.fetchServerConfiguration(localApi().backendUrl) }

        assertEquals("org.xmtp.local", fetched.identifier)
        assertTrue(fetched.serverVersion.isNotBlank())
        assertFalse(fetched.auth.enabled)
        assertEquals(50L, fetched.limits.maxQueryLimit)
        assertEquals(listOf("eip155:31337"), fetched.smartContractWalletChains)

        val client = runBlocking { createClient(PrivateKeyBuilder()) }
        assertEquals(client.serverConfiguration(), fetched)
    }

    /** CFG-082: a refresh returns the fetched value and leaves the snapshot alone. */
    @Test
    fun testRefreshServerConfigurationLeavesTheSnapshot() {
        val client = runBlocking { createClient(PrivateKeyBuilder()) }
        val snapshot = client.serverConfiguration()

        val refreshed = runBlocking { client.refreshServerConfiguration() }

        assertEquals(snapshot, refreshed)
        assertEquals(snapshot, client.serverConfiguration())
    }
}
