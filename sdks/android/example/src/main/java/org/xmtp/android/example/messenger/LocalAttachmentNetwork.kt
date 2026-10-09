package org.xmtp.android.example.messenger

import java.net.URI

/** Only the supported loopback routes use local attachment transport. */
internal fun localAttachmentNetwork(backend: String): Boolean =
    runCatching {
        val uri = URI(backend.trim())
        uri.scheme in setOf("http", "https") &&
            uri.host?.lowercase() in setOf("localhost", "127.0.0.1", "::1", "[::1]", "10.0.2.2")
    }.getOrDefault(false)
