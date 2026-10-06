package uniffi.xmtp_sdk

import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull
import java.net.HttpURLConnection
import java.net.URI

// Helpers for the JVM tests that use this worktree's local backend.
// `just android test` loads dev/docker/.env, so each worktree reaches its own stack.

internal fun liveEnv(name: String): String =
    checkNotNull(System.getenv(name)) {
        "$name is not set. Run the JVM tests through `just android test`, which loads dev/docker/.env."
    }

internal fun liveOptions(url: String = liveEnv("XMTP_BACKEND_URL")) =
    ClientOptions(
        backend = BackendSource.Options(BackendOptions(url = url)),
        storage = StorageOptions(location = StorageLocation.InMemory),
        deviceSync = false,
    )

/** The clients that one [withClients] block made. */
internal class ClientScope {
    private val clients = mutableListOf<SDKClient>()

    suspend fun create(
        signer: Signer? = null,
        options: ClientOptions = liveOptions(),
        defaultDirectory: String? = null,
        codecs: List<ContentCodec<*>> = emptyList(),
    ): SDKClient = own(SDKClient.create(signer ?: generateLocalSigner(), options, defaultDirectory, codecs))

    suspend fun build(
        identity: PublicIdentity,
        options: ClientOptions,
        inboxId: InboxId? = null,
        codecs: List<ContentCodec<*>> = emptyList(),
    ): SDKClient = own(SDKClient.build(identity, options, inboxId, codecs = codecs))

    fun own(client: SDKClient): SDKClient = client.also { synchronized(clients) { clients.add(it) } }

    /** Ends each client in reverse order and returns the first failure. */
    suspend fun endAll(): Throwable? {
        var first: Throwable? = null
        for (client in synchronized(clients) { clients.toList() }.asReversed()) {
            try {
                client.end()
            } catch (error: Throwable) {
                if (first == null) first = error
            }
        }
        return first
    }
}

/**
 * Runs [body] and ends every client that it made, when [body] returns, throws,
 * or stops at a failed create. The first error wins.
 */
internal suspend fun <T> withClients(body: suspend ClientScope.() -> T): T {
    val scope = ClientScope()
    val result =
        try {
            scope.body()
        } catch (error: Throwable) {
            withContext(NonCancellable) { scope.endAll() }?.let(error::addSuppressed)
            throw error
        }
    withContext(NonCancellable) { scope.endAll() }?.let { throw it }
    return result
}

/** True when [condition] holds before [timeoutMs]. */
internal suspend fun eventually(
    timeoutMs: Long,
    condition: suspend () -> Boolean,
): Boolean =
    withTimeoutOrNull(timeoutMs) {
        while (!condition()) delay(50)
        true
    } ?: false

/**
 * Sends one request to this worktree's Toxiproxy API (`XMTP_TOXIPROXY_API`).
 * A status in [accepted] is success; any other status fails.
 */
internal fun toxiproxy(
    path: String,
    body: String? = null,
    method: String = "POST",
    accepted: Iterable<Int> = 200..299,
) {
    val connection = URI(liveEnv("XMTP_TOXIPROXY_API") + path).toURL().openConnection() as HttpURLConnection
    try {
        connection.requestMethod = method
        if (body != null) {
            connection.doOutput = true
            connection.setRequestProperty("Content-Type", "application/json")
            connection.outputStream.use { it.write(body.toByteArray()) }
        }
        val status = connection.responseCode
        check(status in accepted) { "Toxiproxy $method $path returned $status" }
    } finally {
        connection.disconnect()
    }
}
