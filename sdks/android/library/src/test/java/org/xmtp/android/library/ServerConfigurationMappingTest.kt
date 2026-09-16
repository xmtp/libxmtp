package org.xmtp.android.library

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Test
import uniffi.xmtpv3.FfiAuthConfiguration
import uniffi.xmtpv3.FfiLimitsConfiguration
import uniffi.xmtpv3.FfiMlsConfiguration
import uniffi.xmtpv3.FfiRetentionConfiguration
import uniffi.xmtpv3.FfiServerConfiguration
import uniffi.xmtpv3.FfiSigningKeyDescription

/**
 * Spec 006 §7 maps every published `uint64` to [Long] and every `uint32` to
 * [UInt]. This pins the mapping field by field, with a distinct value per field
 * so a crossed wire fails, and pins the failure mode of a value that does not
 * fit: an explicit [XMTPException], never a silent wrap to a negative number.
 */
class ServerConfigurationMappingTest {
    private fun ffiLimits(
        maxEnvelopeBytes: ULong = 1uL,
        maxUpdateFramesPerSecond: UInt = 17u,
    ): FfiLimitsConfiguration =
        FfiLimitsConfiguration(
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
        limits: FfiLimitsConfiguration = ffiLimits(),
        mls: FfiMlsConfiguration =
            FfiMlsConfiguration(
                maxGroupMembers = 250uL,
                maxInstallationsPerInbox = 10uL,
                commitLogEnabled = false,
            ),
    ): FfiServerConfiguration =
        FfiServerConfiguration(
            identifier = "org.xmtp.test",
            serverVersion = "1.2.3",
            minLibxmtpVersion = "1.0.0",
            auth =
                FfiAuthConfiguration(
                    enabled = true,
                    keys = listOf(FfiSigningKeyDescription(kid = "key-1", alg = "ES256")),
                    audiences = listOf("audience"),
                    issuers = listOf("issuer"),
                    requiredScopes = listOf("scope"),
                ),
            retention =
                FfiRetentionConfiguration(
                    groupMessageSeconds = 100uL,
                    welcomeSeconds = 200uL,
                    keyPackageSeconds = 300uL,
                ),
            limits = limits,
            mls = mls,
            smartContractWalletChains = listOf("eip155:1", "eip155:31337"),
        )

    @Test
    fun fromFfi_mapsEveryField() {
        val configuration = ServerConfiguration.fromFfi(ffiConfiguration())

        assertEquals("org.xmtp.test", configuration.identifier)
        assertEquals("1.2.3", configuration.serverVersion)
        assertEquals("1.0.0", configuration.minLibxmtpVersion)

        assertEquals(true, configuration.auth.enabled)
        assertEquals(listOf(SigningKeyDescription("key-1", "ES256")), configuration.auth.keys)
        assertEquals(listOf("audience"), configuration.auth.audiences)
        assertEquals(listOf("issuer"), configuration.auth.issuers)
        assertEquals(listOf("scope"), configuration.auth.requiredScopes)

        assertEquals(100L, configuration.retention.groupMessageSeconds)
        assertEquals(200L, configuration.retention.welcomeSeconds)
        assertEquals(300L, configuration.retention.keyPackageSeconds)

        assertEquals(
            LimitsConfiguration(
                maxEnvelopeBytes = 1L,
                maxRequestBytes = 2L,
                maxResponseBytes = 3L,
                maxPublishTopics = 4L,
                maxQueryTopics = 5L,
                maxQueryLimit = 6L,
                defaultQueryLimit = 7L,
                maxNewestMetadataTopics = 8L,
                maxNewestFullTopics = 9L,
                maxUpdateAdds = 10L,
                maxUpdateRemoves = 11L,
                maxStreamTopics = 12L,
                maxStaticTopics = 13L,
                maxLookupIdentifiers = 14L,
                maxScwSignatures = 15L,
                maxIdentityEntries = 16L,
                maxUpdateFramesPerSecond = 17u,
                maxUpdateBurst = 18u,
                maxPingFramesPerSecond = 19u,
                maxPingBurst = 20u,
            ),
            configuration.limits,
        )

        assertEquals(250L, configuration.mls.maxGroupMembers)
        assertEquals(10L, configuration.mls.maxInstallationsPerInbox)
        assertEquals(false, configuration.mls.commitLogEnabled)

        assertEquals(listOf("eip155:1", "eip155:31337"), configuration.smartContractWalletChains)
    }

    @Test
    fun fromFfi_keepsAbsentCommitLogFlagDistinctFromFalse() {
        val configuration =
            ServerConfiguration.fromFfi(
                ffiConfiguration(
                    mls =
                        FfiMlsConfiguration(
                            maxGroupMembers = 250uL,
                            maxInstallationsPerInbox = 10uL,
                            commitLogEnabled = null,
                        ),
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
            ServerConfiguration.fromFfi(ffiConfiguration(limits = ffiLimits(maxUpdateFramesPerSecond = fast)))

        assertEquals(fast, configuration.limits.maxUpdateFramesPerSecond)
    }

    @Test
    fun fromFfi_rejectsAValueThatDoesNotFitInALong() {
        val tooLarge = Long.MAX_VALUE.toULong() + 1uL

        val error =
            assertThrows(XMTPException::class.java) {
                ServerConfiguration.fromFfi(ffiConfiguration(limits = ffiLimits(maxEnvelopeBytes = tooLarge)))
            }

        assertEquals(
            "Server configuration limits.maxEnvelopeBytes is $tooLarge, which does not fit in a Long",
            error.message,
        )
    }
}
