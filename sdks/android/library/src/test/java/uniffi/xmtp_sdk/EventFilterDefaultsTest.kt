package uniffi.xmtp_sdk

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Test

class EventFilterDefaultsTest {
    @Test
    fun kindsOnlyLeavesOptionalFiltersUnset() {
        val kinds = listOf(EventKind.MESSAGE_RECEIVED)
        val filter = EventFilter(kinds = kinds)

        assertEquals(kinds, filter.kinds)
        assertNull(filter.groupIds)
        assertNull(filter.contentTypes)
        assertFalse(filter.referencesOwnMessages)
    }
}
