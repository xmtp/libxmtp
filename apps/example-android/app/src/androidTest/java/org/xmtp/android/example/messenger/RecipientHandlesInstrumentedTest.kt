package org.xmtp.android.example.messenger

import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.lifecycle.ViewModelProvider
import kotlinx.coroutines.*
import org.junit.After
import org.junit.Assert.*
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.rules.RuleChain
import org.xmtp.android.example.BuildConfig
import org.xmtp.android.example.MainActivity
import org.xmtp.android.example.shared.*
import org.xmtp.android.example.shared.feature.conversations.CreateConversationAction
import org.xmtp.android.example.shared.feature.conversations.CreateStep
import uniffi.xmtp_sdk.*
import java.util.UUID

/** Real handles enter through the recipient fields and reach the intended native inboxes. */
class RecipientHandlesInstrumentedTest {
    private val graphRule = TestAppGraphRule()
    private val compose = createAndroidComposeRule<MainActivity>()

    @get:Rule val rule = RuleChain.outerRule(graphRule).around(compose)
    private val model get() = ViewModelProvider(compose.activity)[MessengerViewModel::class.java]
    private val peers = mutableListOf<SDKClient>()
    private var lifecycle = true
    private val recipientLabel = "Handle, Ethereum address, or inbox ID"

    private suspend fun until(
        stage: String,
        check: suspend () -> Boolean,
    ) {
        try {
            withTimeout(30_000) { while (!check()) delay(20) }
        } catch (error: TimeoutCancellationException) {
            throw AssertionError(stage + ": " + model.state.value.error, error)
        }
    }

    @Before fun connect() =
        runBlocking {
            lifecycle = AndroidStreamLifecycle.enabled
            AndroidStreamLifecycle.enabled = false
            resumeStreams()
            model.session.signOut()
            model.session.connect(BuildConfig.XMTP_BACKEND_URL, "", false)
            val owner = checkNotNull(model.session.active.value)
            until("Session did not open") { model.state.value.inbox == owner.client.inboxId() }
        }

    @After fun close() =
        runBlocking {
            try {
                withContext(NonCancellable) {
                    try {
                        peers.forEach { it.end() }
                    } finally {
                        if (model.session.active.value != null) model.session.deleteAccount()
                    }
                }
            } finally {
                AndroidStreamLifecycle.enabled = lifecycle
            }
        }

    private suspend fun peer(): SDKClient =
        SDKClient
            .create(
                compose.activity,
                generateLocalSigner(),
                ClientOptions(
                    backend = BackendSource.Options(BackendOptions(url = BuildConfig.XMTP_BACKEND_URL)),
                    storage = StorageOptions(location = StorageLocation.InMemory),
                    deviceSync = false,
                ),
            ).also(peers::add)

    private suspend fun claim(
        client: SDKClient,
        coded: Boolean = false,
    ): String {
        val name =
            "peer" +
                UUID
                    .randomUUID()
                    .toString()
                    .replace("-", "")
                    .take(20)
        val handle = name + if (coded) "#2341" else ""
        client.claimHandle(handle)
        return handle
    }

    private fun button(label: String): SemanticsNodeInteraction {
        val matcher = hasText(label) and hasClickAction()
        if (model.state.value.screen == Screen.CONVERSATION_SETTINGS) {
            compose.onNodeWithTag("conversation-settings").performScrollToNode(matcher)
        }
        return compose.onNode(matcher)
    }

    private suspend fun create(
        input: String,
        expected: Set<String>,
        group: Boolean = false,
    ): Conversation {
        model.createConversation.clear()
        model.dispatch(MessengerAction.Navigate(Screen.CREATE))
        until("Create screen did not open") { model.state.value.screen == Screen.CREATE }
        model.createConversation.dispatch(CreateConversationAction.ChangeMode(group))
        compose.onNodeWithText(recipientLabel).performTextReplacement(input)
        androidx.test.espresso.Espresso
            .closeSoftKeyboard()
        button("Check and add").performScrollTo().performClick()
        until("Input did not resolve to its claimed inbox") {
            !model.createConversation.state.value.busy &&
                model.createConversation.state.value.selectedRecipients
                    .map { it.inboxId }
                    .toSet() == expected &&
                model.createConversation.state.value.failure == null
        }
        button("Continue").performScrollTo().performClick()
        until("Review did not open") { model.createConversation.state.value.step == CreateStep.REVIEW }
        button(if (group) "Create group" else "Start conversation").performScrollTo().performClick()
        until("Created conversation did not open") {
            model.state.value.screen == Screen.TIMELINE && model.state.value.conversationId != null
        }
        val owner = checkNotNull(model.session.active.value)
        return checkNotNull(owner.client.conversations.getById(checkNotNull(model.state.value.conversationId)))
    }

    @Test fun directMessagesResolveBareAndCodedHandles() =
        runBlocking {
            for (coded in listOf(false, true)) {
                val target = peer()
                val handle = claim(target, coded)
                val conversation = create(" " + handle.uppercase() + " ", setOf(target.inboxId()))
                assertEquals(target.inboxId(), (conversation as Conversation.Dm).dm.peerInboxId())
                model.dispatch(MessengerAction.Navigate(Screen.CONVERSATIONS))
                until("Chats did not reopen") { model.state.value.screen == Screen.CONVERSATIONS }
            }
        }

    @Test fun groupsResolveMixedRecipientsAndAddAHandleWithoutLeavingItPending() =
        runBlocking {
            val byHandle = peer()
            val handle = claim(byHandle)
            val byAddress = peer()
            val byInbox = peer()
            val expected = setOf(byHandle.inboxId(), byAddress.inboxId(), byInbox.inboxId())
            val conversation =
                create(
                    listOf(
                        handle,
                        byAddress.identity().identifier,
                        byInbox.inboxId(),
                        byHandle.inboxId(),
                    ).joinToString(", "),
                    expected,
                    group = true,
                )
            val group = (conversation as Conversation.Group).group
            assertTrue(group.members().map { it.inboxId }.containsAll(expected))
            val added = peer()
            val addedHandle = claim(added, coded = true)
            compose.onNodeWithContentDescription("Conversation details").performClick()
            until("Group settings did not open") {
                model.state.value.screen == Screen.CONVERSATION_SETTINGS && model.state.value.settings.group
            }
            button("Add member").performClick()
            compose.onNodeWithText(recipientLabel).performTextReplacement(addedHandle)
            androidx.test.espresso.Espresso
                .closeSoftKeyboard()
            button("Add member").performClick()
            until("Handle did not add the claimed inbox or stayed pending") {
                !model.group.state.value.busy &&
                    model.group.state.value.pendingMemberIds
                        .isEmpty() &&
                    model.group.state.value.settings.members
                        .any { it.inboxId == added.inboxId() }
            }
            assertTrue(group.members().any { it.inboxId == added.inboxId() })
            assertFalse(group.members().any { it.inboxId == addedHandle })
        }
}
