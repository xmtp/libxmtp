package org.xmtp.android.example.messenger

import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.first
import org.junit.Assert.*
import org.junit.Test
import org.xmtp.android.example.BuildConfig
import uniffi.xmtp_sdk.*
import java.security.SecureRandom

class SessionRaceInstrumentedTest {
    private val context get() = InstrumentationRegistry.getInstrumentation().targetContext.applicationContext

    @Test fun invalidatedFinalPersistenceCannotCommitOrStartReaders() =
        runBlocking {
            AndroidStreamLifecycle.enabled = false
            resumeStreams()
            val session = AppSession(context)
            val entered = CompletableDeferred<Unit>()
            val release = CompletableDeferred<Unit>()
            var accepted = false
            val invalidated = CompletableDeferred<Unit>()
            session.preferences.beforeSessionCommit = {
                entered.complete(Unit)
                release.await()
            }
            session.preferences.sessionCommitAccepted = { accepted = true }
            session.onSessionInvalidated = { invalidated.complete(Unit) }
            val opening = async(Dispatchers.IO) { session.connect(BuildConfig.XMTP_BACKEND_URL, "", false) }
            try {
                entered.await()
                val signingOut = async(Dispatchers.IO) { session.signOut() }
                // signOut reserves its generation before waiting for the open operation.
                invalidated.await()
                release.complete(Unit)
                opening.await()
                signingOut.await()
                assertFalse(accepted)
                assertFalse(session.preferences.signedIn())
                assertNull(session.active.value)
            } finally {
                release.complete(Unit)
                session.preferences.beforeSessionCommit = {}
                withContext(NonCancellable) {
                    opening.cancelAndJoin()
                    session.signOut()
                    if (session.preferences.active() !=
                        null
                    ) {
                        session.deleteAccount()
                    }
                }
                AndroidStreamLifecycle.enabled = true
            }
        }

    @Test fun invalidationDuringCallbackDoesNotAcknowledgeWhileUnregisterIsBlocked() =
        runBlocking {
            AndroidStreamLifecycle.enabled = false
            resumeStreams()
            val session = AppSession(context)
            val entered = CompletableDeferred<Unit>()
            val release = CompletableDeferred<Unit>()
            val unregistering = CompletableDeferred<Unit>()
            val allowUnregister = CompletableDeferred<Unit>()
            var peer: SDKClient? = null
            var peerReader: Job? = null
            var signingOut: Deferred<Unit>? = null
            try {
                session.connect(BuildConfig.XMTP_BACKEND_URL, "", false)
                val owner = checkNotNull(session.active.value)
                session.onMessage = { _, message ->
                    val body = (message.content as? SDKMessageContent.Standard)?.value as? MessageContent.Text
                    if (body?.v1 == "Unacknowledged callback") {
                        entered.complete(Unit)
                        release.await()
                    }
                }
                peer =
                    SDKClient.create(
                        context,
                        localSignerFromPrivateKey(SecureRandom().generateSeed(32)),
                        ClientOptions(
                            backend = BackendSource.Options(BackendOptions(url = BuildConfig.XMTP_BACKEND_URL)),
                            storage = StorageOptions(location = StorageLocation.InMemory),
                            deviceSync = false,
                        ),
                    )
                val second = checkNotNull(peer)
                peerReader = launch { second.conversations.streamAllMessages().collect { } }
                val remote = second.conversations.createDm(owner.client.inboxId())
                val id = remote.sendText("Unacknowledged callback")
                entered.await()
                session.unregisterNotifications = {
                    unregistering.complete(Unit)
                    allowUnregister.await()
                }
                signingOut = async(Dispatchers.IO) { session.signOut() }
                unregistering.await()
                assertFalse(session.accepts(owner.key))
                release.complete(Unit)
                val replay =
                    withTimeout(30_000) {
                        var message: Message? = null
                        while (message == null) {
                            try {
                                message =
                                    owner.client.conversations
                                        .streamAllMessages()
                                        .first()
                            } catch (
                                error: XmtpException.ConsumerOwned,
                            ) {
                                delay(20)
                            }
                        }
                        message
                    }
                assertEquals(id, replay.id)
                assertFalse(checkNotNull(signingOut).isCompleted)
                allowUnregister.complete(Unit)
                signingOut.await()
            } finally {
                release.complete(Unit)
                allowUnregister.complete(Unit)
                peerReader?.cancelAndJoin()
                withContext(NonCancellable) {
                    signingOut?.await()
                    peer?.end()
                    session.signOut()
                    if (session.preferences.active() !=
                        null
                    ) {
                        session.deleteAccount()
                    }
                }
                AndroidStreamLifecycle.enabled = true
            }
        }
}
