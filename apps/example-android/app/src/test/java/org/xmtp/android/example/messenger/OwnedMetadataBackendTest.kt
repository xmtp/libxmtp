package org.xmtp.android.example.messenger

import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Test

class OwnedMetadataBackendTest {
    @Test fun onlyAnAdmittedCanonicalFixtureIsAccepted() {
        assertEquals("http://127.0.0.1:45678", ownedMetadataBackend("http://127.0.0.1:45678", "true"))
        for (admission in listOf(null, "false", "")) {
            assertThrows(IllegalArgumentException::class.java) {
                ownedMetadataBackend("http://127.0.0.1:45678", admission)
            }
        }
        for (target in listOf(
            null,
            "https://production.invalid",
            "http://localhost:45678",
            "http://127.0.0.1",
            "http://127.0.0.1:45678/",
            "http://127.0.0.1:45678?x=1",
            "http://user@127.0.0.1:45678",
            "http://127.0.0.1:65536",
        )) {
            assertThrows(IllegalArgumentException::class.java) { ownedMetadataBackend(target, "true") }
        }
    }
}
