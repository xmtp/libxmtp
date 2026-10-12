package org.xmtp.android.example.messenger
import android.graphics.Bitmap
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.*
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.rules.RuleChain
import org.junit.rules.TestRule
import org.xmtp.android.example.BuildConfig
import org.xmtp.android.example.ExampleApp
import org.xmtp.android.example.MainActivity
import org.xmtp.android.example.shared.feature.chat.ChatAction
import uniffi.xmtp_sdk.*
import java.security.SecureRandom

class MessengerScreensInstrumentedTest {
    private val graphRule = TestAppGraphRule()
    val compose = createAndroidComposeRule<MainActivity>()

    @get:Rule val rules: TestRule = RuleChain.outerRule(graphRule).around(compose)

    private fun screen(name: String) {
        androidx.test.espresso.Espresso
            .closeSoftKeyboard()
        compose
            .waitForIdle()
        val bitmap =
            checkNotNull(
                InstrumentationRegistry
                    .getInstrumentation()
                    .uiAutomation
                    .takeScreenshot(),
            )
        try {
            writeProofScreenshot(compose.activity, name, bitmap)
        } finally {
            bitmap.recycle()
        }
    }

    private fun visible(text: String) {
        compose
            .waitUntil(30_000) {
                compose
                    .onAllNodesWithText(text)
                    .fetchSemanticsNodes()
                    .isNotEmpty()
            }
    }

    @Test fun realSessionRendersSetupListMessagesAndSettings() =
        runBlocking {
            val app =
                compose.activity
                    .application as ExampleApp
            val session =
                app.session
            assertEquals("org.xmtp.android.example", app.packageName)
            AndroidStreamLifecycle.enabled = false
            resumeStreams()
            var peer: SDKClient? = null
            var peerReader: Job? = null
            val releaseSend = CompletableDeferred<Unit>()
            try {
                compose
                    .onNodeWithContentDescription("XMTP logo")
                    .assertExists()
                screen("setup")
                compose
                    .onNodeWithText("Connect")
                    .performClick()
                compose
                    .waitUntil(30_000) {
                        session.active.value != null
                    }
                val owner =
                    checkNotNull(
                        session.active.value,
                    )
                peer =
                    SDKClient
                        .create(
                            app,
                            localSignerFromPrivateKey(
                                SecureRandom()
                                    .generateSeed(32),
                            ),
                            ClientOptions(
                                backend =
                                    BackendSource
                                        .Options(
                                            BackendOptions(
                                                url =
                                                    BuildConfig.XMTP_BACKEND_URL,
                                            ),
                                        ),
                                storage =
                                    StorageOptions(
                                        location =
                                            StorageLocation.InMemory,
                                    ),
                                deviceSync = false,
                            ),
                        )
                val second = checkNotNull(peer)
                peerReader =
                    launch {
                        second.conversations
                            .streamAllMessages()
                            .collect {
                            }
                    }
                val group =
                    owner.client.conversations
                        .createGroup(
                            listOf(
                                second
                                    .inboxId(),
                            ),
                            CreateGroupOptions(
                                name = "Navy proof group",
                                description = "Current SDK group",
                                permissions =
                                    GroupPermissionMode.AllMembers,
                            ),
                        )
                visible("Navy proof group")
                screen("conversations")
                compose.onNodeWithContentDescription("New conversation").performClick()
                screen("create")
                compose.onAllNodesWithText("Group").onFirst().performClick()
                screen("create-group")
                compose.onNodeWithContentDescription("Back").performClick()
                compose
                    .onNodeWithText("Navy proof group")
                    .performClick()
                withTimeout(30_000) {
                    while (second.conversations
                            .getById(
                                group
                                    .id(),
                            ) == null
                    ) {
                        delay(50)
                    }
                }
                checkNotNull(
                    second.conversations
                        .getById(
                            group
                                .id(),
                        ),
                ).sendText("Hello from the other member")
                visible("Hello from the other member")
                compose
                    .onNodeWithText("Message")
                    .performTextInput("Hello from this device")
                val sendEntered = CompletableDeferred<Unit>()
                graphRule.graph.chat.beforeQueuedAction = { action ->
                    if (action is ChatAction.SendText && action.text == "Hello from this device") {
                        sendEntered.complete(Unit)
                        releaseSend.await()
                    }
                }
                val jobsBeforeSend = checkNotNull(owner.work.coroutineContext[Job]).children.toSet()
                compose.runOnIdle {
                    val chat = graphRule.graph.chat
                    val repeated =
                        ChatAction.SendText(
                            "Hello from this device",
                            917L,
                            chat.state.value.composer.revision,
                        )
                    chat.dispatch(repeated)
                    chat.dispatch(repeated)
                }
                withTimeout(30_000) { sendEntered.await() }
                val submittedJobs =
                    checkNotNull(owner.work.coroutineContext[Job]).children.filterNot { it in jobsBeforeSend }.toList()
                releaseSend.complete(Unit)
                withTimeout(30_000) { submittedJobs.forEach { it.join() } }
                graphRule.graph.chat.beforeQueuedAction = {}
                visible("Hello from this device")
                val nativeSent =
                    Conversation
                        .Group(group)
                        .messages(publishedSelection().copy(deliveryStatus = null))
                        .filter { it.standardContent() == MessageContent.Text("Hello from this device") }
                assertEquals("Repeated send keeps one native row", 1, nativeSent.size)
                val peerChat = checkNotNull(second.conversations.getById(group.id()))
                withTimeout(30_000) {
                    while (peerChat.messages(publishedSelection()).none { it.id == nativeSent.single().id }) delay(50)
                }
                val peerSent =
                    peerChat
                        .messages(publishedSelection())
                        .filter { it.standardContent() == MessageContent.Text("Hello from this device") }
                assertEquals("Repeated send keeps one peer row", listOf(nativeSent.single().id), peerSent.map { it.id })
                compose
                    .onNodeWithText("Hello from the other member")
                    .performClick()
                compose
                    .onNodeWithText("Reply")
                    .performClick()
                compose
                    .onNodeWithText("Message")
                    .performTextInput("A reply from this device")
                compose
                    .onNodeWithText("Send")
                    .performClick()
                visible("A reply from this device")
                screen("messages")
                compose
                    .onNodeWithContentDescription("Conversation settings")
                    .performClick()
                visible("Members")
                screen("settings")
                compose.onNodeWithContentDescription("Back").performClick()
                compose.onNodeWithContentDescription("Back").performClick()
                compose.onNodeWithContentDescription("Settings").performClick()
                screen("app-settings")
                assertEquals(
                    "Navy proof group",
                    group
                        .state()
                        .name,
                )
            } finally {
                releaseSend.complete(Unit)
                graphRule.graph.chat.beforeQueuedAction = {}
                peerReader?.cancelAndJoin()
                withContext(NonCancellable) {
                    peer?.end()
                    if (session.active.value !=
                        null
                    ) {
                        session
                            .deleteAccount()
                    }
                }
                AndroidStreamLifecycle.enabled = true
            }
        }
}
