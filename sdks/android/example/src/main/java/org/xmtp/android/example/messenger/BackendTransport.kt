package org.xmtp.android.example.messenger

import org.xmtp.android.example.BuildConfig
import java.net.URI

internal val localDevelopmentHosts =
    setOf("localhost", "127.0.0.1", "::1", "[::1]") +
        if (BuildConfig.DEBUG) setOf("10.0.2.2") else emptySet()

/** Validate transport before profile, credential, or SDK work. */
internal fun validatedBackendUrl(backend: String): String {
    val url = backend.trim().trimEnd('/')
    val uri =
        try {
            URI(url)
        } catch (_: IllegalArgumentException) {
            throw IllegalArgumentException("Use a valid backend URL")
        } catch (_: java.net.URISyntaxException) {
            throw IllegalArgumentException("Use a valid backend URL")
        }
    require(uri.scheme?.lowercase() in setOf("http", "https") && uri.host != null) {
        "Use a valid HTTP or HTTPS backend URL"
    }
    require(
        uri.rawUserInfo == null && uri.rawFragment == null &&
            uri.rawAuthority?.endsWith(':') == false && (uri.port == -1 || uri.port in 1..65535),
    ) {
        "Use a backend URL without credentials, a fragment, or an invalid port"
    }
    require(uri.scheme.equals("https", ignoreCase = true) || uri.host.lowercase() in localDevelopmentHosts) {
        "Use HTTPS for a remote backend"
    }
    return url
}
