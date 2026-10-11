package org.xmtp.android.example.messenger
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.emoji2.text.EmojiCompat
import androidx.test.espresso.Espresso
import androidx.test.espresso.matcher.ViewMatchers.withContentDescription
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.*
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.rules.RuleChain
import org.junit.rules.TestRule
import org.junit.runner.RunWith
import org.xmtp.android.example.BuildConfig
import org.xmtp.android.example.MainActivity
import org.xmtp.android.example.shared.*
import org.xmtp.android.example.shared.feature.chat.MessageSurface
import uniffi.xmtp_sdk.*

@RunWith(AndroidJUnit4::class)
class OfflineAndEventsInstrumentedTest {
    private val graphRule = TestAppGraphRule()
    val compose = createAndroidComposeRule<MainActivity>()

    @get:Rule val rules: TestRule = RuleChain.outerRule(graphRule).around(compose)

    @Test fun sameEndpointOfflineReopenKeepsLocalHistoryAndInbox() =
        runBlocking {
            AndroidStreamLifecycle.enabled = false
            resumeStreams()
            val proxy = AppOfflineBackendProxy(BuildConfig.XMTP_BACKEND_URL)
            val graph = graphRule.graph
            val session = graph.session

            suspend fun until(
                stage: String,
                check: suspend () -> Boolean,
            ) {
                assertTrue(
                    stage,
                    withTimeoutOrNull(30_000) {
                        while (!check()) delay(30)
                        true
                    } == true,
                )
            }
            try {
                compose.onNodeWithText("Server address").performTextReplacement(proxy.url)
                compose.onNode(hasText("Join a server") and hasClickAction()).performScrollTo().performClick()
                until("real relayed server connected") { session.active.value != null }
                val owner = checkNotNull(session.active.value)
                val chat =
                    owner.client.conversations.createGroup(
                        emptyList(),
                        CreateGroupOptions(name = "Offline history"),
                    )
                val id = chat.sendText("stored offline history")
                val inbox = owner.client.inboxId()
                graph.destinations.navigate(AppRoute.Chat(chat.id()))
                until("history visible before outage") {
                    graph.chat.state.value.messages
                        .any { it.id == id }
                }
                session.signOut()
                proxy.close()
                withContext(Dispatchers.IO) { proxy.assertUnavailable() }
                session.connect(proxy.url, "", false)
                val reopened = checkNotNull(session.active.value)
                assertEquals(inbox, reopened.client.inboxId())
                assertEquals(proxy.url, reopened.profile.backend)
                val stored = checkNotNull(reopened.client.conversations.getById(chat.id()))
                assertEquals(listOf(id), stored.messages(publishedSelection()).map { it.id })
                until("offline local inbox visible") {
                    graph.conversations.state.value.rows
                        .any { it.id == chat.id() }
                }
                compose.onNodeWithText("Offline history").performClick()
                until("offline local body visible") {
                    graph.chat.state.value.messages
                        .any { it.id == id }
                }
                compose.onNodeWithText("stored offline history").assertIsDisplayed().performClick()
                compose.onNodeWithContentDescription("More reactions").performClick()
                compose.onNodeWithTag("emoji-sheet").assertExists()
                until("offline bundled emoji font") { EmojiCompat.get().loadState == EmojiCompat.LOAD_STATE_SUCCEEDED }
                until("offline native emoji grid") {
                    runCatching {
                        Espresso
                            .onView(withContentDescription(org.hamcrest.CoreMatchers.containsString("😀")))
                            .check { view, error ->
                                if (error != null) throw error
                                assertTrue(view.isShown)
                            }
                    }.isSuccess
                }
                compose.onNodeWithTag("emoji-search").performTextInput("grinning face")
                compose.waitUntil(30_000) {
                    compose.onAllNodes(hasText("😀") and hasClickAction()).fetchSemanticsNodes().isNotEmpty()
                }
                compose.onNodeWithContentDescription("Close emoji picker").performClick()
                until("offline sheet close retains native selection") {
                    graph.chat.state.value.overlay
                        ?.surface == MessageSurface.POPOVER
                }
                assertEquals(
                    id,
                    graph.chat.state.value.overlay
                        ?.messageId,
                )
                assertEquals(listOf(id), stored.messages(publishedSelection()).map { it.id })
                println("OFFLINE_PROOF stage=same-endpoint-local-inbox-history-native-picker-and-search")
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
            val sessionFixture = TestSessionFixture(context)
            val session = sessionFixture.session
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
                sessionFixture.projection.onEvent = {
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
                sessionFixture.projection.onInvalidated = {
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
                sessionFixture.projection.onMessage = {
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
