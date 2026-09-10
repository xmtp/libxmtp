package org.xmtp.android.library

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class StreamSettingsTest {
    @Test
    fun unsetFieldsUseNativeDefaults() {
        val settings = StreamSettings().toFfi()
        assertNull(settings.maxAdmissionRows)
        assertNull(settings.maxAdmissionBytes)
        assertNull(settings.maxFetchedRows)
        assertNull(settings.maxFetchedBytes)
        assertNull(settings.groupPendingRows)
        assertNull(settings.groupPendingBytes)
        assertNull(settings.welcomePendingRows)
        assertNull(settings.welcomePendingBytes)
        assertNull(settings.identityPendingRows)
        assertNull(settings.identityPendingBytes)
        assertNull(settings.maxPendingRowsPerTopic)
        assertNull(settings.maxPendingBytesPerTopic)
        assertNull(settings.maxDependencyRequests)
        assertNull(settings.maxLocalReadRows)
        assertNull(settings.maxLocalReadBytes)
        assertNull(settings.receiverFallbackIntervalMs)
        assertNull(settings.activeDatabasePollIntervalMs)
        assertNull(settings.defaultConsumerLeaseDurationMs)
        assertNull(settings.identityReferenceWaitMs)
        assertNull(settings.barrierTimeoutMs)
    }

    @Test
    fun suppliedFieldsKeepTheirValuesAndNativeIntegerWidths() {
        val settings =
            StreamSettings(
                maxAdmissionRows = UInt.MAX_VALUE,
                maxAdmissionBytes = ULong.MAX_VALUE,
                maxFetchedRows = 3u,
                maxFetchedBytes = 4uL,
                groupPendingRows = 5uL,
                groupPendingBytes = 6uL,
                welcomePendingRows = 7uL,
                welcomePendingBytes = 8uL,
                identityPendingRows = 9uL,
                identityPendingBytes = 10uL,
                maxPendingRowsPerTopic = 11uL,
                maxPendingBytesPerTopic = 12uL,
                maxDependencyRequests = 13u,
                maxLocalReadRows = 14u,
                maxLocalReadBytes = 15uL,
                receiverFallbackIntervalMs = 16uL,
                activeDatabasePollIntervalMs = 17uL,
                defaultConsumerLeaseDurationMs = 18uL,
                identityReferenceWaitMs = 19uL,
                barrierTimeoutMs = 20uL,
            ).toFfi()
        assertEquals(UInt.MAX_VALUE, settings.maxAdmissionRows)
        assertEquals(ULong.MAX_VALUE, settings.maxAdmissionBytes)
        assertEquals(3u, settings.maxFetchedRows)
        assertEquals(4uL, settings.maxFetchedBytes)
        assertEquals(5uL, settings.groupPendingRows)
        assertEquals(6uL, settings.groupPendingBytes)
        assertEquals(7uL, settings.welcomePendingRows)
        assertEquals(8uL, settings.welcomePendingBytes)
        assertEquals(9uL, settings.identityPendingRows)
        assertEquals(10uL, settings.identityPendingBytes)
        assertEquals(11uL, settings.maxPendingRowsPerTopic)
        assertEquals(12uL, settings.maxPendingBytesPerTopic)
        assertEquals(13u, settings.maxDependencyRequests)
        assertEquals(14u, settings.maxLocalReadRows)
        assertEquals(15uL, settings.maxLocalReadBytes)
        assertEquals(16uL, settings.receiverFallbackIntervalMs)
        assertEquals(17uL, settings.activeDatabasePollIntervalMs)
        assertEquals(18uL, settings.defaultConsumerLeaseDurationMs)
        assertEquals(19uL, settings.identityReferenceWaitMs)
        assertEquals(20uL, settings.barrierTimeoutMs)
    }
}
