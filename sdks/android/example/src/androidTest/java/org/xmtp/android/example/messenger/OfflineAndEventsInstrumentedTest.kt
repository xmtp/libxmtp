package org.xmtp.android.example.messenger
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.*
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.xmtp.android.example.BuildConfig
import uniffi.xmtp_sdk.*

@RunWith(AndroidJUnit4::class)
class OfflineAndEventsInstrumentedTest {
    @Test fun sameEndpointOfflineReopenKeepsLocalHistoryAndInbox() =
        runBlocking {
            val context =
                InstrumentationRegistry
                    .getInstrumentation()
                    .targetContext.applicationContext
            AndroidStreamLifecycle.enabled = false
            resumeStreams()
            val proxy =
                AppOfflineBackendProxy(
                    BuildConfig.XMTP_BACKEND_URL,
                )
            val session = AppSession(context)
            try {
                session
                    .connect(
                        proxy.url,
                        "",
                        false,
                    )
                val owner =
                    checkNotNull(
                        session.active.value,
                    )
                val chat =
                    owner.client.conversations
                        .createGroup(emptyList())
                val id =
                    chat
                        .sendText("stored offline history")
                val inbox =
                    owner.client
                        .inboxId()
                session
                    .signOut()
                proxy
                    .close()
                withContext(Dispatchers.IO) { proxy.assertUnavailable() }
                session
                    .connect(
                        proxy.url,
                        "",
                        false,
                    )
                val reopened =
                    checkNotNull(
                        session.active.value,
                    )
                assertEquals(
                    inbox,
                    reopened.client
                        .inboxId(),
                )
                val stored =
                    checkNotNull(
                        reopened.client.conversations
                            .getById(
                                chat
                                    .id(),
                            ),
                    )
                assertEquals(
                    listOf(id),
                    stored
                        .messages(publishedSelection())
                        .map {
                            it.id
                        },
                )
            } finally {
                withContext(NonCancellable) {
                    if (session.active.value != null) {
                        session
                            .deleteAccount()
                    }
                    session
                        .signOut()
                    proxy
                        .close()
                }
                AndroidStreamLifecycle.enabled = true
            }
        }

    @Test fun laggedEventRereadsCurrentConsentAndEarlySendEventMergesOneId() =
        runBlocking {
            val context =
                InstrumentationRegistry
                    .getInstrumentation()
                    .targetContext.applicationContext
            AndroidStreamLifecycle.enabled = false
            resumeStreams()
            val session = AppSession(context)
            val entered = CompletableDeferred<Unit>()
            val release = CompletableDeferred<Unit>()
            val lagged = CompletableDeferred<Unit>()
            val received =
                java.util.concurrent.ConcurrentHashMap<
                    String,
                    Message,
                >()
            var currentConsent: ConsentState? = null
            try {
                session
                    .connect(
                        BuildConfig.XMTP_BACKEND_URL,
                        "",
                        false,
                    )
                val owner =
                    checkNotNull(
                        session.active.value,
                    )
                val group =
                    owner.client.conversations
                        .createGroup(emptyList())
                session.onEvent = {
                    _,
                    event,
                    ->
                    if (event is ClientEvent
                            .ConsentChanged &&
                        !entered.isCompleted
                    ) {
                        entered
                            .complete(Unit)
                        release
                            .await()
                    }
                    if (event is ClientEvent.Lagged) {
                        lagged
                            .complete(Unit)
                    }
                }
                session.onInvalidated = {
                    currentConsent =
                        group
                            .state()
                            .common.consentState
                }
                group
                    .updateConsentState(
                        ConsentState.UNKNOWN,
                    )
                withTimeout(30_000) {
                    entered
                        .await()
                }
                repeat(1100) {
                    group
                        .updateConsentState(
                            if (it % 2 == 0) {
                                ConsentState.ALLOWED
                            } else {
                                ConsentState.UNKNOWN
                            },
                        )
                }
                release
                    .complete(Unit)
                withTimeout(30_000) {
                    lagged
                        .await()
                    while (currentConsent !=
                        ConsentState.UNKNOWN
                    ) {
                        delay(50)
                    }
                }
                assertEquals(
                    group
                        .state()
                        .common.consentState,
                    currentConsent,
                )
                session.onMessage = {
                    _,
                    message,
                    ->
                    received[
                        message.id,
                    ] = message
                }
                val coordinator =
                    SendCoordinator(
                        session.preferences,
                        session::accepts,
                    )
                val chat =
                    Conversation
                        .Group(group)
                var sawEarly = false
                val id =
                    coordinator
                        .queue(
                            owner.key,
                            owner.client,
                            chat,
                            reconcile = {
                                received[
                                    it.id,
                                ] = it
                            },
                        ) {
                            val queued =
                                chat
                                    .sendText(
                                        "event before acceptance return",
                                        SendOptions(optimistic = true),
                                    )
                            chat
                                .publishMessage(queued)
                            withTimeout(30_000) {
                                while (!received
                                        .containsKey(queued)
                                ) {
                                    delay(50)
                                }
                            }
                            sawEarly = true
                            queued
                        }
                assertTrue(sawEarly)
                assertEquals(
                    1,
                    received.values.count {
                        it.id == id
                    },
                )
                assertEquals(
                    1,
                    chat
                        .messages(publishedSelection())
                        .count {
                            it.id == id
                        },
                )
            } finally {
                release
                    .complete(Unit)
                withContext(NonCancellable) {
                    if (session.active.value != null) {
                        session
                            .deleteAccount()
                    }
                    session
                        .signOut()
                }
                AndroidStreamLifecycle.enabled = true
            }
        }
}
