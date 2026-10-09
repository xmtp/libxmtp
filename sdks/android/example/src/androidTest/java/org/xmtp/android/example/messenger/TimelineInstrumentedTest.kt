package org.xmtp.android.example.messenger

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import java.security.SecureRandom
import kotlinx.coroutines.*
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.xmtp.android.example.BuildConfig
import uniffi.xmtp_sdk.*

@RunWith(AndroidJUnit4::class)
class TimelineInstrumentedTest {
    @Test fun directCollectorReconcilesIdsAndUnreadUsesInsertionTime() = runBlocking {
        val context = InstrumentationRegistry.getInstrumentation().targetContext.applicationContext
        AndroidStreamLifecycle.enabled = false
        val session = AppSession(context)
        var peer: SDKClient? = null
        var reader: Job? = null
        val received = java.util.concurrent.ConcurrentHashMap<String, Message>()
        session.onMessage = { _, message -> received[message.id] = message }
        try {
            session.connect(BuildConfig.XMTP_BACKEND_URL, "", false)
            val owner = checkNotNull(session.active.value)
            peer = SDKClient.create(context, localSignerFromPrivateKey(SecureRandom().generateSeed(32)), ClientOptions(backend = BackendSource.Options(BackendOptions(url = BuildConfig.XMTP_BACKEND_URL)), storage = StorageOptions(location = StorageLocation.InMemory), deviceSync = false))
            val second = checkNotNull(peer)
            reader = launch { second.conversations.streamAllMessages().collect { } }
            val peerChat = second.conversations.createDm(owner.client.inboxId())
            val id = peerChat.sendText("incoming")
            withTimeout(30_000) { while (!received.containsKey(id)) delay(50) }
            val chat = checkNotNull(owner.client.conversations.getById(checkNotNull(received[id]).conversationId))
            assertEquals(1uL, chat.countMessages(incomingSelection(owner.client.inboxId())))
            val inserted = checkNotNull(received[id]).insertedAt.ns
            assertEquals(0uL, chat.countMessages(incomingSelection(owner.client.inboxId(), inserted)))
            assertEquals(inserted, incomingSelection(owner.client.inboxId(), inserted).insertedAfter?.ns)
            assertEquals(1, received.values.count { it.id == id })
            session.retryReader()
            assertEquals(1, chat.messages(publishedSelection()).count { it.id == id })
        } finally { reader?.cancelAndJoin(); withContext(NonCancellable) { peer?.end(); session.deleteAccount() }; AndroidStreamLifecycle.enabled = true }
    }
}
