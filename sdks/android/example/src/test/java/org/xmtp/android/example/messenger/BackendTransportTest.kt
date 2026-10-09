package org.xmtp.android.example.messenger

import org.junit.Assert.*
import org.junit.Test

class BackendTransportTest {
    @Test fun remoteTransportRequiresHttpsAndLocalRoutesUseLiteralHosts() {
        for (url in listOf(
            "https://example.com",
            "HTTPS://EXAMPLE.COM:443/api",
            "http://localhost:5050",
            "http://127.0.0.1:5050",
            "http://[::1]:5050",
        )) {
            assertEquals(url, validatedBackendUrl("  $url/  "))
        }
        for (url in listOf(
            "http://example.com",
            "http://192.168.1.2",
            "http://localhost.example.com",
            "http://sub.localhost",
            "http://127.1",
            "http://2130706433",
            "http://0x7f000001",
            "http://[::ffff:127.0.0.1]",
            "http://localhost.",
            "http://127.0.0.1@example.com",
            "https://user:secret@example.com",
            "https://example.com:0",
            "https://example.com:65536",
            "https://example.com:",
            "https://example.com:abc",
            "https:///example.com",
            "https://example.com/#fragment",
            "ftp://localhost",
            "invalid",
        )) {
            val error = runCatching { validatedBackendUrl(url) }.exceptionOrNull()
            assertTrue("Rejected URL: $url", error is IllegalArgumentException)
        }
    }
}
