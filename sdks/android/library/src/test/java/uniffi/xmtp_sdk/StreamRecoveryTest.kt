package uniffi.xmtp_sdk

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.net.HttpURLConnection
import java.net.URI
import java.util.Collections

// A live message stream through the local Toxiproxy fault proxy. The proxy
// drops every connection and refuses new ones; the open Kotlin Flow must stay
// open, report the outage, and deliver the message sent meanwhile after the
// proxy comes back. Rust tests the reconnect itself:
// xmtp_mls/src/client/tests/lifecycle.rs::should_reconnect.
class StreamRecoveryTest {
    // dev/docker/toxiproxy/config.json defines this proxy in front of the backend.
    private val proxy = "backend"

    private fun toxiproxy(
        path: String,
        body: String,
    ) {
        val connection = URI(liveEnv("XMTP_TOXIPROXY_API") + path).toURL().openConnection() as HttpURLConnection
        try {
            connection.requestMethod = "POST"
            connection.doOutput = true
            connection.setRequestProperty("Content-Type", "application/json")
            connection.outputStream.use { it.write(body.toByteArray()) }
            val status = connection.responseCode
            check(status in 200..299) { "Toxiproxy $path returned $status" }
        } finally {
            connection.disconnect()
        }
    }

    private fun setProxyEnabled(enabled: Boolean) = toxiproxy("/proxies/$proxy", """{"enabled": $enabled}""")

    // Enables every proxy and removes every toxic.
    private fun resetProxies() = toxiproxy("/reset", "")

    @Test
    fun messageStreamRecoversAfterTheProxyDropsItsConnections() =
        runBlocking {
            resetProxies()
            try {
                withTimeout(240_000) {
                    withClients {
                        val sender = create()
                        val receiver = create(options = liveOptions(liveEnv("XMTP_BACKEND_TOXIC_URL")))
                        val group = sender.conversations().createGroup(listOf(receiver.inboxId()))
                        receiver.conversations().sync()
                        val joined = (receiver.conversations().getById(group.id()) as Conversation.Group).group
                        val received = Collections.synchronizedList(mutableListOf<MessageId>())
                        val states = Collections.synchronizedList(mutableListOf<ConnectionState>())
                        var closed: SDKStreamCloseReason? = null
                        val stream =
                            launch(Dispatchers.Default) {
                                receiver
                                    .messages(
                                        joined,
                                        onClose = { closed = it },
                                        onConnectionStateChange = { _, current -> states.add(current) },
                                    ).collect { received.add(it.id) }
                            }
                        try {
                            val before = group.sendText("before the outage")
                            assertTrue(
                                "The stream did not deliver before the outage",
                                eventually(30_000) {
                                    before in
                                        received
                                },
                            )
                            val outageStart = states.size
                            setProxyEnabled(false)
                            val during =
                                try {
                                    val id = group.sendText("during the outage")
                                    assertTrue(
                                        "The stream did not report the dropped connection",
                                        eventually(60_000) {
                                            synchronized(states) {
                                                states.drop(outageStart).any { it != ConnectionState.CONNECTED }
                                            }
                                        },
                                    )
                                    id
                                } finally {
                                    setProxyEnabled(true)
                                }
                            assertTrue(
                                "The stream did not deliver the message sent during the outage",
                                eventually(120_000) { during in received },
                            )
                            assertTrue(
                                "The stream did not report the restored connection: $states",
                                eventually(
                                    30_000,
                                ) { synchronized(states) { states.last() } == ConnectionState.CONNECTED },
                            )
                            val after = group.sendText("after the outage")
                            assertTrue(
                                "The recovered stream stopped delivering",
                                eventually(30_000) { after in received },
                            )
                            assertNull("The stream closed during recovery: $closed", closed)
                        } finally {
                            stream.cancelAndJoin()
                        }
                    }
                }
            } finally {
                resetProxies()
            }
        }
}
