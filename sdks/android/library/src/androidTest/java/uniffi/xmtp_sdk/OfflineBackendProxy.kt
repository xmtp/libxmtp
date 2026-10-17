package uniffi.xmtp_sdk

import java.io.IOException
import java.net.InetAddress
import java.net.InetSocketAddress
import java.net.ServerSocket
import java.net.Socket
import java.net.URI
import java.util.Collections
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger

/** Use one backend URL, then stop its private test transport for offline reopen. */
internal class OfflineBackendProxy(
    backendUrl: String,
) : AutoCloseable {
    private val backend = URI(backendUrl).also { require(it.scheme == "http") }
    private val server = ServerSocket(0, 50, InetAddress.getByName("127.0.0.1"))
    private val executor = Executors.newCachedThreadPool()
    private val sockets = Collections.synchronizedSet(mutableSetOf<Socket>())
    private val closed = AtomicBoolean(false)
    private val accepted = AtomicInteger()
    val url = "http://127.0.0.1:${server.localPort}"

    init {
        executor.execute {
            while (!closed.get()) {
                try {
                    val client = server.accept()
                    sockets.add(client)
                    val remote = Socket()
                    sockets.add(remote)
                    remote.connect(InetSocketAddress(backend.host, if (backend.port < 0) 80 else backend.port), 5000)
                    accepted.incrementAndGet()
                    executor.execute { transfer(client, remote) }
                    executor.execute { transfer(remote, client) }
                } catch (error: IOException) {
                    if (!closed.get()) throw error
                }
            }
        }
    }

    private fun transfer(
        source: Socket,
        destination: Socket,
    ) {
        try {
            source.getInputStream().copyTo(destination.getOutputStream())
        } catch (_: IOException) {
            // Closing either transport ends both directions.
        } finally {
            source.close()
            destination.close()
            sockets.remove(source)
            sockets.remove(destination)
        }
    }

    fun assertUnavailable() {
        check(closed.get() && accepted.get() > 0)
        val endpoint = URI(url)
        val failure =
            runCatching {
                Socket().use { it.connect(InetSocketAddress(endpoint.host, endpoint.port), 250) }
            }.exceptionOrNull()
        check(failure is IOException) { "The same offline backend endpoint must reject new connections" }
        println("Offline backend endpoint closed: $url; accepted=${accepted.get()}")
    }

    override fun close() {
        if (!closed.compareAndSet(false, true)) return
        server.close()
        synchronized(sockets) { sockets.toList() }.forEach { it.close() }
        executor.shutdownNow()
    }
}
