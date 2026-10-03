package org.xmtp.android.library

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import uniffi.xmtp_sdk.AuthConfiguration
import uniffi.xmtp_sdk.LimitsConfiguration
import uniffi.xmtp_sdk.MlsConfiguration
import uniffi.xmtp_sdk.RetentionConfiguration
import uniffi.xmtp_sdk.ServerConfiguration
import uniffi.xmtp_sdk.SigningKeyDescription

/** The generated SDK keeps unsigned wire values without a signed conversion. */
class ServerConfigurationMappingTest {
    private fun ffiLimits(
        maxEnvelopeBytes: ULong = 1uL,
        maxUpdateFramesPerSecond: UInt = 17u,
    ): LimitsConfiguration =
        LimitsConfiguration(
            maxEnvelopeBytes = maxEnvelopeBytes,
            maxRequestBytes = 2uL,
            maxResponseBytes = 3uL,
            maxPublishTopics = 4uL,
            maxQueryTopics = 5uL,
            maxQueryLimit = 6uL,
            defaultQueryLimit = 7uL,
            maxNewestMetadataTopics = 8uL,
            maxNewestFullTopics = 9uL,
            maxUpdateAdds = 10uL,
            maxUpdateRemoves = 11uL,
            maxStreamTopics = 12uL,
            maxStaticTopics = 13uL,
            maxLookupIdentifiers = 14uL,
            maxScwSignatures = 15uL,
            maxIdentityEntries = 16uL,
            maxUpdateFramesPerSecond = maxUpdateFramesPerSecond,
            maxUpdateBurst = 18u,
            maxPingFramesPerSecond = 19u,
            maxPingBurst = 20u,
        )

    private fun ffiConfiguration(
        limits: LimitsConfiguration = ffiLimits(),
        mls: MlsConfiguration =
            MlsConfiguration(
                maxGroupMembers = 250uL,
                maxInstallationsPerInbox = 10uL,
                commitLogEnabled = false,
            ),
    ): ServerConfiguration =
        ServerConfiguration(
            identifier = "org.xmtp.test",
            serverVersion = "1.2.3",
            minLibxmtpVersion = "1.0.0",
            auth =
                AuthConfiguration(
                    enabled = true,
                    keys = listOf(SigningKeyDescription(kid = "key-1", alg = "ES256")),
                    audiences = listOf("audience"),
                    issuers = listOf("issuer"),
                    requiredScopes = listOf("scope"),
                ),
            retention =
                RetentionConfiguration(
                    groupMessageSeconds = 100uL,
                    welcomeSeconds = 200uL,
                    keyPackageSeconds = 300uL,
                ),
            limits = limits,
            mls = mls,
            smartContractWalletChains = listOf("eip155:1", "eip155:31337"),
            attachments = null,
            applicationComponents = emptyList(),
        )

    @Test
    fun fromFfi_mapsEveryField() {
        val configuration = ffiConfiguration()

        assertEquals("org.xmtp.test", configuration.identifier)
        assertEquals("1.2.3", configuration.serverVersion)
        assertEquals("1.0.0", configuration.minLibxmtpVersion)

        assertEquals(true, configuration.auth.enabled)
        assertEquals(listOf(SigningKeyDescription("key-1", "ES256")), configuration.auth.keys)
        assertEquals(listOf("audience"), configuration.auth.audiences)
        assertEquals(listOf("issuer"), configuration.auth.issuers)
        assertEquals(listOf("scope"), configuration.auth.requiredScopes)

        assertEquals(100uL, configuration.retention.groupMessageSeconds)
        assertEquals(200uL, configuration.retention.welcomeSeconds)
        assertEquals(300uL, configuration.retention.keyPackageSeconds)

        assertEquals(
            LimitsConfiguration(
                maxEnvelopeBytes = 1uL,
                maxRequestBytes = 2uL,
                maxResponseBytes = 3uL,
                maxPublishTopics = 4uL,
                maxQueryTopics = 5uL,
                maxQueryLimit = 6uL,
                defaultQueryLimit = 7uL,
                maxNewestMetadataTopics = 8uL,
                maxNewestFullTopics = 9uL,
                maxUpdateAdds = 10uL,
                maxUpdateRemoves = 11uL,
                maxStreamTopics = 12uL,
                maxStaticTopics = 13uL,
                maxLookupIdentifiers = 14uL,
                maxScwSignatures = 15uL,
                maxIdentityEntries = 16uL,
                maxUpdateFramesPerSecond = 17u,
                maxUpdateBurst = 18u,
                maxPingFramesPerSecond = 19u,
                maxPingBurst = 20u,
            ),
            configuration.limits,
        )

        assertEquals(250uL, configuration.mls.maxGroupMembers)
        assertEquals(10uL, configuration.mls.maxInstallationsPerInbox)
        assertEquals(false, configuration.mls.commitLogEnabled)

        assertEquals(listOf("eip155:1", "eip155:31337"), configuration.smartContractWalletChains)
    }

    @Test
    fun fromFfi_keepsAbsentCommitLogFlagDistinctFromFalse() {
        val configuration =
            ffiConfiguration(
                mls =
                    MlsConfiguration(
                        maxGroupMembers = 250uL,
                        maxInstallationsPerInbox = 10uL,
                        commitLogEnabled = null,
                    ),
            )

        assertNull(configuration.mls.commitLogEnabled)
    }

    /**
     * A rate is a `uint32` on the wire, so any value up to 2^32 - 1 is one a
     * deployment may publish. Reading it must return the configuration, not
     * throw for a value a signed `Int` could not have held.
     */
    @Test
    fun fromFfi_keepsARateAboveTheSignedIntegerRange() {
        val fast = Int.MAX_VALUE.toUInt() + 1u

        val configuration =
            ffiConfiguration(limits = ffiLimits(maxUpdateFramesPerSecond = fast))

        assertEquals(fast, configuration.limits.maxUpdateFramesPerSecond)
    }

    @Test
    fun keepsAValueThatDoesNotFitInALong() {
        val tooLarge = Long.MAX_VALUE.toULong() + 1uL
        val configuration = ffiConfiguration(limits = ffiLimits(maxEnvelopeBytes = tooLarge))
        assertEquals(tooLarge, configuration.limits.maxEnvelopeBytes)
    }
}
