package org.xmtp.android.example.messenger

import java.net.URI

/** Only the supported loopback routes use local attachment transport. */
internal fun localAttachmentNetwork(backend: String): Boolean =
    runCatching {
        val uri = URI(backend.trim())
        uri.scheme?.lowercase() in setOf("http", "https") &&
            uri.host?.lowercase() in localDevelopmentHosts
    }.getOrDefault(false)
