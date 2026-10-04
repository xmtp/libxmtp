package com.example.xmtpv3_example

import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Test

class BackendUrlPolicyTest {
    @Test
    fun releaseRejectsCleartextBackendUrls() {
        for (url in listOf("http://relay.example.com", "http://10.0.2.2:5050")) {
            assertThrows(IllegalArgumentException::class.java) { checkedBackendUrl(url, debug = false) }
        }
    }

    @Test
    fun debugAllowsOnlyLocalCleartextBackendUrls() {
        for (host in listOf("10.0.2.2", "127.0.0.1", "localhost", "[::1]")) {
            val url = "http://$host:5050"
            assertEquals(url, checkedBackendUrl(url, debug = true))
        }
        for (url in listOf(
            "http://relay.example.com",
            "http://localhost.evil.example",
            "http://localhost@evil.example",
        )) {
            assertThrows(IllegalArgumentException::class.java) { checkedBackendUrl(url, debug = true) }
        }
    }

    @Test
    fun httpsBackendUrlsWorkInBothBuildTypes() {
        val url = "https://relay.example.com:8443/rpc"
        assertEquals(url, checkedBackendUrl(url, debug = false))
        assertEquals(url, checkedBackendUrl(url, debug = true))
    }

    @Test
    fun unsupportedProtocolsAreRejected() {
        assertThrows(IllegalArgumentException::class.java) { checkedBackendUrl("ftp://localhost", debug = true) }
        assertThrows(IllegalArgumentException::class.java) { checkedBackendUrl("/rpc", debug = true) }
    }
}
