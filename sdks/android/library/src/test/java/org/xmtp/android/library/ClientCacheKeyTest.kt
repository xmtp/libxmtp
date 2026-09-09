package org.xmtp.android.library

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertThrows
import org.junit.Test

class ClientCacheKeyTest {
    @Test
    fun testApiClientCacheKeysDifferentConfigurations() {
        val api = localApi()
        val second = api.copy(backendUrl = "https://backend.example.com")
        val third = api.copy(backendUrl = "https://other.example.com")
        assertNotEquals(api.toCacheKey(), second.toCacheKey())
        assertNotEquals(second.toCacheKey(), third.toCacheKey())
        assertNotEquals(api.toCacheKey(), third.toCacheKey())

        val versionOne = api.copy(appVersion = "1.0.0")
        val versionTwo = api.copy(appVersion = "2.0.0")
        assertNotEquals(versionOne.toCacheKey(), versionTwo.toCacheKey())
        assertNotEquals(api.toCacheKey(), versionOne.toCacheKey())
        assertNotEquals(api.toCacheKey(), api.copy(appVersion = "").toCacheKey())
        assertNotEquals(versionOne.toCacheKey(), second.copy(appVersion = "2.0.0").toCacheKey())

        assertEquals(versionOne.toCacheKey(), versionOne.copy().toCacheKey())
        assertEquals(api.toCacheKey(), localApi().toCacheKey())
        assertEquals(api.toCacheKey(), api.copy(env = "custom-db").toCacheKey())
        assertEquals(versionOne.toCacheKey(), versionOne.copy(env = "custom-db").toCacheKey())
        assertEquals("http://10.0.2.2:5050|null", api.toCacheKey())
        assertEquals("http://10.0.2.2:5050|1.0.0", versionOne.toCacheKey())
    }

    @Test
    fun testRejectsEmptyBackendUrl() {
        for (url in listOf("", " ", "\t\n")) {
            assertThrows(IllegalArgumentException::class.java) {
                ClientOptions.Api(backendUrl = url)
            }
        }
    }
}
