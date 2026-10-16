package org.xmtp.android.example.messenger

import androidx.lifecycle.ViewModelStore
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.first
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.xmtp.android.example.BuildConfig
import uniffi.xmtp_sdk.*
import java.security.SecureRandom
import java.util.concurrent.atomic.AtomicInteger

class SessionRaceInstrumentedTest {
    private val graphRule = TestAppGraphRule()

    @get:Rule val graphScope: org.junit.rules.TestRule = graphRule
    private val context get() = InstrumentationRegistry.getInstrumentation().targetContext.applicationContext

    @Test fun staleSignOutAndResetCannotCloseANewerNativeConnect() =
        runBlocking {
            AndroidStreamLifecycle.enabled = false
            resumeStreams()
            val sessionFixture = TestSessionFixture(context)
            val session = sessionFixture.session
            val releases = mutableListOf<CompletableDeferred<Unit>>()
            try {
                session.signOut()
                for (reset in listOf(false, true)) {
                    session.connect(BuildConfig.XMTP_BACKEND_URL, "", false)
                    val first = checkNotNull(session.active.value)
                    val entered = CompletableDeferred<Unit>()
                    val release = CompletableDeferred<Unit>().also(releases::add)
                    session.beforeSessionStopLock = {
                        entered.complete(Unit)
                        release.await()
                    }
                    val stop = async(Dispatchers.IO) { if (reset) session.deleteAccount() else session.signOut() }
                    withTimeout(30_000) { entered.await() }
                    assertFalse(session.accepts(first.key))
                    session.connect(BuildConfig.XMTP_BACKEND_URL, "", false)
                    val latest = checkNotNull(session.active.value)
                    val group =
                        latest.client.conversations.createGroup(
                            emptyList(),
                            CreateGroupOptions(name = "Latest owner"),
                        )
                    release.complete(Unit)
                    withTimeout(30_000) { stop.await() }
                    assertEquals(latest.key, session.active.value?.key)
                    assertTrue(session.accepts(latest.key))
                    assertEquals("Latest owner", group.state().name)
                    assertTrue(session.preferences.signedIn())
                    assertTrue(latest.paths.database.exists())
                    assertNull(session.preferences.reset())
                    println("SESSION_ORDER_PROOF reset=$reset stage=old-stop-skipped-new-native-owner-live")
                    session.beforeSessionStopLock = {}
                }
            } finally {
                releases.forEach { it.complete(Unit) }
                session.beforeSessionStopLock = {}
                withContext(NonCancellable) {
                    if (session.active.value != null) session.deleteAccount()
                    session.signOut()
                }
                AndroidStreamLifecycle.enabled = true
            }
        }

    @Test fun defaultReaderRunsOnlyInTheForeground() =
        runBlocking {
            AndroidStreamLifecycle.enabled = false
            resumeStreams()
            val session = TestSessionFixture(context).session
            var holder: Job? = null
            try {
                session.signOut()
                session.visible(false)
                session.connect(BuildConfig.XMTP_BACKEND_URL, "", false)
                val owner = checkNotNull(session.active.value)
                delay(2_000)
                assertFalse("A background connect starts no default reader", defaultReaderOwned(owner.client))
                // Hold the lease as a killed process does until it expires.
                holder =
                    launch(Dispatchers.IO) {
                        while (true) {
                            try {
                                owner.client.conversations
                                    .streamAllMessages()
                                    .collect {}
                            } catch (error: XmtpException.ConsumerOwned) {
                                delay(50)
                            }
                        }
                    }
                withTimeout(30_000) { while (!defaultReaderOwned(owner.client)) delay(100) }
                session.visible(true)
                withTimeout(30_000) { session.connection.first { it == "RECONNECTING" } }
                assertNull("A held lease does not stop the reader", session.readerError.value)
                holder.cancelAndJoin()
                withTimeout(30_000) { session.connection.first { it == "CONNECTED" } }
                assertTrue("The reader takes the released lease", defaultReaderOwned(owner.client))
                session.visible(false)
                // A process that Android freezes or kills now leaves no lease for the next reader.
                withTimeout(30_000) { session.connection.first { it.isEmpty() } }
                assertFalse("A background session keeps no default reader", defaultReaderOwned(owner.client))
                session.visible(true)
                withTimeout(30_000) { session.connection.first { it == "CONNECTED" } }
                assertTrue("The foreground session owns delivery again", defaultReaderOwned(owner.client))
                assertNull(session.readerError.value)
            } finally {
                withContext(NonCancellable) {
                    holder?.cancelAndJoin()
                    session.visible(true)
                    if (session.active.value != null) session.deleteAccount()
                    session.signOut()
                }
                AndroidStreamLifecycle.enabled = true
            }
        }

    @Test fun invalidatedFinalPersistenceCannotCommitOrStartReaders() =
        runBlocking {
            AndroidStreamLifecycle.enabled = false
            resumeStreams()
            val sessionFixture = TestSessionFixture(context)
            val session = sessionFixture.session
            val entered = CompletableDeferred<Unit>()
            val release = CompletableDeferred<Unit>()
            var accepted = false
            val invalidated = CompletableDeferred<Unit>()
            session.preferences.beforeSessionCommit = {
                entered.complete(Unit)
                release.await()
            }
            session.preferences.sessionCommitAccepted = { accepted = true }
            sessionFixture.lifecycle.onSessionInvalidated = { invalidated.complete(Unit) }
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

    @Test fun listenerFailureEndsUnpublishedNativeOwner() = openingFailure(false, false)

    @Test fun listenerCancellationEndsUnpublishedNativeOwner() = openingFailure(false, true)

    @Test fun persistenceFailureEndsUnpublishedNativeOwner() = openingFailure(true, false)

    @Test fun persistenceCancellationEndsUnpublishedNativeOwner() = openingFailure(true, true)

    private fun openingFailure(
        atCommit: Boolean,
        cancel: Boolean,
    ) = runBlocking {
        AndroidStreamLifecycle.enabled = false
        resumeStreams()
        val sessionFixture = TestSessionFixture(context)
        val session = sessionFixture.session
        session.signOut()
        val entered = CompletableDeferred<Unit>()
        val release = CompletableDeferred<Unit>()
        val result = CompletableDeferred<Throwable?>()
        var held: ActiveSession? = null
        var work: Job? = null
        var closeCount = 0
        sessionFixture.lifecycle.beforeEnd = { closeCount += 1 }

        suspend fun fault() {
            entered.complete(Unit)
            if (cancel) release.await() else error("Opening fault")
        }
        session.beforeOpeningListener = { owner ->
            held = owner
            work = owner.work.launch { awaitCancellation() }
            if (!atCommit) fault()
        }
        session.preferences.beforeSessionCommit = { if (atCommit) fault() }
        val attempt =
            launch(Dispatchers.IO) {
                try {
                    session.connect(BuildConfig.XMTP_BACKEND_URL, "", false)
                    result.complete(null)
                } catch (error: Throwable) {
                    result.complete(error)
                }
            }
        try {
            withTimeout(30_000) { entered.await() }
            if (cancel) attempt.cancel()
            withTimeout(30_000) { attempt.join() }
            assertNotNull(result.await())
            val owner = checkNotNull(held)
            assertNull(session.active.value)
            assertFalse(session.preferences.signedIn())
            val failure = runCatching { owner.client.conversations.listGroups(null) }.exceptionOrNull()
            assertTrue("Native owner must be closed: $failure", failure is XmtpException.ClientClosed)
            assertTrue(checkNotNull(work).isCompleted)
            assertEquals(1, closeCount)
            println(
                "OPENING_PROOF stage=${if (atCommit) "persistence" else "listener"}-${if (cancel) "cancel" else "failure"}-native-client-closed",
            )
            assertTrue(owner.paths.database.exists())
            session.deleteAccount()
            assertFalse(owner.paths.database.exists())
            assertNull(session.preferences.reset())
            assertEquals(1, closeCount)
            println("OPENING_PROOF stage=closed-owner-storage-reset")
        } finally {
            release.complete(Unit)
            session.beforeOpeningListener = {}
            session.preferences.beforeSessionCommit = {}
            withContext(NonCancellable) {
                attempt.cancelAndJoin()
                held
                    ?.work
                    ?.coroutineContext
                    ?.get(Job)
                    ?.cancelAndJoin()
                held?.client?.let { client ->
                    if (runCatching {
                            client.conversations.listGroups(
                                null,
                            )
                        }.exceptionOrNull() !is XmtpException.ClientClosed
                    ) {
                        client.end()
                    }
                }
                session.signOut()
                if (session.preferences.active() != null) session.deleteAccount()
            }
            AndroidStreamLifecycle.enabled = true
        }
    }

    @Test fun invalidationDuringCallbackDoesNotAcknowledgeWhileUnregisterIsBlocked() =
        runBlocking {
            // Keep each existing deadline and name the wait that fails.
            suspend fun <T> atStage(
                stage: String,
                action: suspend () -> T,
            ): T =
                try {
                    withTimeout(30_000) { action() }
                } catch (error: TimeoutCancellationException) {
                    throw AssertionError("Stage: $stage", error)
                }

            AndroidStreamLifecycle.enabled = false
            resumeStreams()
            val session = graphRule.graph.session
            val oldShellStore = ViewModelStore()
            val newShellStore = ViewModelStore()
            withContext(Dispatchers.Main) { graphRule.newShell(oldShellStore) }
            val handled = AtomicInteger()
            val entered = CompletableDeferred<Unit>()
            val release = CompletableDeferred<Unit>()
            val invalidated = CompletableDeferred<Unit>()
            graphRule.observers.onSessionInvalidated = { invalidated.complete(Unit) }
            val unregistering = CompletableDeferred<Unit>()
            val allowUnregister = CompletableDeferred<Unit>()
            var peer: SDKClient? = null
            var peerReader: Job? = null
            var signingOut: Deferred<Unit>? = null
            try {
                session.connect(BuildConfig.XMTP_BACKEND_URL, "", false)
                val owner = checkNotNull(session.active.value)
                graphRule.observers.afterHandle = { _, message ->
                    val body = (message.content as? SDKMessageContent.Standard)?.value as? MessageContent.Text
                    if (body?.v1 == "Unacknowledged callback") {
                        handled.incrementAndGet()
                        entered.complete(Unit)
                        withContext(NonCancellable) { release.await() }
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
                atStage("required handler entry") { entered.await() }
                withContext(Dispatchers.Main) {
                    oldShellStore.clear()
                    graphRule.newShell(newShellStore)
                }
                assertSame(owner, session.active.value)
                assertEquals("Shell replacement adds no required handler", 1, handled.get())
                val stillOwned =
                    atStage("default reader ownership after shell replacement") {
                        runCatching {
                            owner.client.conversations
                                .streamAllMessages()
                                .first()
                        }.exceptionOrNull()
                    }
                assertTrue("Shell replacement keeps one default reader", stillOwned is XmtpException.ConsumerOwned)
                assertNull(session.readerError.value)
                graphRule.observers.beforeUnregister = {
                    unregistering.complete(Unit)
                    allowUnregister.await()
                }
                signingOut = async(Dispatchers.IO) { session.signOut() }
                invalidated.await()
                assertFalse(session.accepts(owner.key))
                release.complete(Unit)
                unregistering.await()
                val replay =
                    atStage("original native ID replay while unregister is blocked") {
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
                    withContext(Dispatchers.Main) {
                        oldShellStore.clear()
                        newShellStore.clear()
                    }
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

// A second default reader fails at open while another reader owns delivery.
internal suspend fun defaultReaderOwned(client: SDKClient): Boolean =
    try {
        withTimeoutOrNull(1_000) { client.conversations.streamAllMessages().first() }
        false
    } catch (error: XmtpException.ConsumerOwned) {
        true
    }
