package org.xmtp.android.library

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class DbPoolOptionsTest {
    @Test
    fun defaultsToAllNull() {
        val options = DbPoolOptions()
        assertNull(options.maxPoolSize)
        assertNull(options.minPoolSize)
    }

    @Test
    fun carriesValuesThrough() {
        val options = DbPoolOptions(maxPoolSize = 10u, minPoolSize = 2u)
        assertEquals(10u, options.maxPoolSize)
        assertEquals(2u, options.minPoolSize)
    }

    @Test
    fun acceptsPartialFields() {
        val options = DbPoolOptions(maxPoolSize = 7u)
        assertEquals(7u, options.maxPoolSize)
        assertNull(options.minPoolSize)
    }
}
