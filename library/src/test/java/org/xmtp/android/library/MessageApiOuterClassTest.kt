package org.xmtp.android.library

import org.junit.Test

import org.junit.Assert.*
import org.xmtp.proto.message.api.v1.MessageApiOuterClass

class TestMessageApiOuterClass {
    @Test fun testTypesAreAvailable() {
        assertEquals(1, MessageApiOuterClass.SortDirection.SORT_DIRECTION_ASCENDING_VALUE)
    }
}