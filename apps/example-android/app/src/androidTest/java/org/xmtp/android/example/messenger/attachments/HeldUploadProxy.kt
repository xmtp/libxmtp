package org.xmtp.android.example.messenger.attachments

import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONObject
import java.net.HttpURLConnection
import java.net.URL

/** Tests use the existing worktree proxy. The recipe supplies its device routes. */
internal class HeldUploadProxy {
    private val arguments = InstrumentationRegistry.getArguments()
    val backend =
        checkNotNull(arguments.getString("toxicBackendUrl")) { "Supply the worktree toxicBackendUrl test argument" }
    private val api =
        checkNotNull(arguments.getString("toxiproxyApi")) { "Supply the worktree toxiproxyApi test argument" }
    private val hold = "/proxies/backend/toxics/messenger-attachment-hold"

    private suspend fun request(
        path: String,
        method: String,
        body: String? = null,
    ) = withContext(Dispatchers.IO) {
        val connection = URL(api.trimEnd('/') + path).openConnection() as HttpURLConnection
        try {
            connection.requestMethod = method
            connection.connectTimeout = 10_000
            connection.readTimeout = 10_000
            if (body != null) {
                connection.doOutput = true
                connection.setRequestProperty("Content-Type", "application/json")
                connection.outputStream.use { it.write(body.toByteArray()) }
            }
            val status = connection.responseCode
            require(
                status in 200..299 || (method == "DELETE" && status == 404),
            ) { "Proxy control failed: HTTP $status" }
            if (status < 400) {
                connection.inputStream.bufferedReader().use { it.readText() }
            } else {
                connection.errorStream?.close()
                ""
            }
        } finally {
            connection.disconnect()
        }
    }

    suspend fun <T> preservingEnabled(
        cleanup: suspend () -> Unit,
        block: suspend () -> T,
    ): T =
        ProxyStateScope(
            readEnabled = { JSONObject(request("/proxies/backend", "GET")).getBoolean("enabled") },
            writeEnabled = ::enabled,
            releaseHold = ::release,
        ).run(cleanup, block)

    suspend fun hold() =
        request(
            "/proxies/backend/toxics",
            "POST",
            """{"name":"messenger-attachment-hold","type":"latency","stream":"upstream","attributes":{"latency":600000}}""",
        )

    suspend fun release() = request(hold, "DELETE")

    suspend fun enabled(value: Boolean) = request("/proxies/backend", "POST", """{"enabled":$value}""")
}
