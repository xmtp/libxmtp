package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.xmtp_sdk.*

/**
 * Read every field of [SDKClient.serverConfiguration] and call
 * [SDKClient.fetchServerConfiguration] against the shared backend.
 *
 * The shared stack runs `dev/backend/local.toml`, so the identifier, the two
 * query limits, and the single anvil chain are asserted exactly. Every other
 * published value is a compiled default that an operator may retune, so those
 * are asserted as read and in range.
 */
@RunWith(AndroidJUnit4::class)
class ServerConfigurationTest : BaseInstrumentedTest() {
    // verifies: CONF-061
    @Test
    fun testServerConfigurationExposesEveryField() {
        val client = runBlocking { createClient(createWallet()) }
        val configuration = client.serverConfiguration()

        assertEquals("org.xmtp.local", configuration.identifier)
        assertTrue(
            "server version should be published: ${configuration.serverVersion}",
            configuration.serverVersion.isNotBlank(),
        )
        // local.toml publishes no minimum, which leaves the deployment open to
        // every client version.
        assertEquals("", configuration.minLibxmtpVersion)

        val auth = configuration.auth
        assertFalse(auth.enabled)
        assertTrue(auth.keys.isEmpty())
        assertTrue(auth.audiences.isEmpty())
        assertTrue(auth.issuers.isEmpty())
        assertTrue(auth.requiredScopes.isEmpty())

        val retention = configuration.retention
        assertTrue(retention.groupMessageSeconds > 0uL)
        assertTrue(retention.welcomeSeconds > 0uL)
        assertTrue(retention.keyPackageSeconds > 0uL)

        val limits = configuration.limits
        // The two values local.toml overrides.
        assertEquals(50uL, limits.maxQueryLimit)
        assertEquals(50uL, limits.defaultQueryLimit)
        assertTrue(limits.maxEnvelopeBytes > 0uL)
        assertTrue(limits.maxRequestBytes > 0uL)
        assertTrue(limits.maxResponseBytes > 0uL)
        assertTrue(limits.maxPublishTopics > 0uL)
        assertTrue(limits.maxQueryTopics > 0uL)
        assertTrue(limits.maxNewestMetadataTopics > 0uL)
        assertTrue(limits.maxNewestFullTopics > 0uL)
        assertTrue(limits.maxUpdateAdds > 0uL)
        assertTrue(limits.maxUpdateRemoves > 0uL)
        assertTrue(limits.maxStreamTopics > 0uL)
        assertTrue(limits.maxStaticTopics > 0uL)
        assertTrue(limits.maxLookupIdentifiers > 0uL)
        assertTrue(limits.maxScwSignatures > 0uL)
        assertTrue(limits.maxIdentityEntries > 0uL)
        assertTrue(limits.maxUpdateFramesPerSecond > 0u)
        assertTrue(limits.maxUpdateBurst > 0u)
        assertTrue(limits.maxPingFramesPerSecond > 0u)
        assertTrue(limits.maxPingBurst > 0u)

        val mls = configuration.mls
        assertTrue(mls.maxGroupMembers > 0uL)
        assertTrue(mls.maxInstallationsPerInbox > 0uL)
        // `[mls]` is absent from local.toml, so the flag reads as the published
        // default rather than as absent.
        assertEquals(true, mls.commitLogEnabled)

        // The one chain the local stack verifies, anvil.
        assertEquals(listOf("eip155:31337"), configuration.smartContractWalletChains)
    }

    // verifies: CONF-062
    @Test
    fun testFetchServerConfigurationWithoutAClient() {
        val fetched = runBlocking { SDKClient.fetchServerConfiguration(BackendSource.Options(localApi())) }

        assertEquals("org.xmtp.local", fetched.identifier)
        assertTrue(fetched.serverVersion.isNotBlank())
        assertFalse(fetched.auth.enabled)
        assertEquals(50uL, fetched.limits.maxQueryLimit)
        assertEquals(listOf("eip155:31337"), fetched.smartContractWalletChains)

        val client = runBlocking { createClient(createWallet()) }
        assertEquals(client.serverConfiguration(), fetched)
    }

    // verifies: CONF-074
    @Test
    fun testRefreshServerConfigurationLeavesTheSnapshot() {
        val client = runBlocking { createClient(createWallet()) }
        val snapshot = client.serverConfiguration()

        val refreshed = runBlocking { client.refreshServerConfiguration() }

        assertEquals(snapshot, refreshed)
        assertEquals(snapshot, client.serverConfiguration())
    }
}
