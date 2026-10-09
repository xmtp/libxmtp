package org.xmtp.android.example.messenger

import java.io.IOException
import java.net.InetAddress
import java.net.InetSocketAddress
import java.net.ServerSocket
import java.net.Socket
import java.net.URI
import java.util.Collections
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger

/** This fixture owns its listener, workers and every accepted socket. */
internal class AppOfflineBackendProxy(
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
                    if (closed.get()) {
                        client.close()
                        continue
                    }
                    sockets.add(client)
                    val remote = Socket()
                    sockets.add(remote)
                    remote.connect(InetSocketAddress(backend.host, if (backend.port < 0) 80 else backend.port), 5000)
                    if (closed.get()) {
                        client.close()
                        remote.close()
                        continue
                    }
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
            // Closing either socket ends the transfer.
        } finally {
            source.close()
            destination.close()
            sockets.remove(source)
            sockets.remove(destination)
        }
    }

    fun assertUnavailable() {
        check(closed.get() && accepted.get() > 0 && server.isClosed)
        val endpoint = URI(url)
        val failure =
            runCatching {
                Socket().use { socket ->
                    socket.bind(InetSocketAddress("127.0.0.1", 0))
                    check(socket.localPort != endpoint.port) { "Probe source port equals its destination" }
                    socket.connect(InetSocketAddress(endpoint.host, endpoint.port), 250)
                }
            }.exceptionOrNull()
        check(failure is IOException) {
            "The same offline endpoint must reject connections: " +
                (failure?.javaClass?.name ?: "connection accepted") + " at $url"
        }
    }

    override fun close() {
        if (!closed.compareAndSet(false, true)) return
        server.close()
        synchronized(sockets) { sockets.toList() }.forEach { it.close() }
        executor.shutdownNow()
        check(executor.awaitTermination(5, TimeUnit.SECONDS)) { "Proxy workers did not stop" }
        synchronized(sockets) { sockets.toList() }.forEach { it.close() }
    }
}
