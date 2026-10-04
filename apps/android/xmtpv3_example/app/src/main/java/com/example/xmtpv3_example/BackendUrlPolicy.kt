package com.example.xmtpv3_example

import java.net.URI

internal fun checkedBackendUrl(
    url: String,
    debug: Boolean,
): String {
    val endpoint = URI(url)
    val host = endpoint.host?.lowercase()
    val local = host in setOf("10.0.2.2", "127.0.0.1", "localhost", "[::1]")
    require(
        host != null &&
            (
                endpoint.scheme.equals("https", ignoreCase = true) ||
                    (debug && local && endpoint.scheme.equals("http", ignoreCase = true))
            ),
    ) { "Use HTTPS for the backend URL. Local HTTP is available only in debug builds." }
    return url
}
