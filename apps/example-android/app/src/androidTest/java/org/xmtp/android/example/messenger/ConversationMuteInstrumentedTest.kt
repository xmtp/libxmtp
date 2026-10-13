package org.xmtp.android.example.messenger

import androidx.lifecycle.ViewModelStore
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.*
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TestRule
import org.xmtp.android.example.BuildConfig
import org.xmtp.android.example.messenger.notifications.PushTransport
import uniffi.xmtp_sdk.*

/** A disabled push registration is separate from the user's explicit per-chat mute choice. */
class ConversationMuteInstrumentedTest {
    private val graphRule =
        TestAppGraphRule(
            notificationTransport =
                object : PushTransport {
                    override val configured = true

                    override fun requestToken(callback: (String?, Throwable?) -> Unit) = Unit
                },
        )

    @get:Rule val rule: TestRule = graphRule

    @Test fun pushBeingOffDoesNotMuteAChatAndExplicitMuteCanBeReversed() =
        runBlocking<Unit> {
            val store = ViewModelStore()
            val lifecycle = AndroidStreamLifecycle.enabled
            var peer: SDKClient? = null
            try {
                AndroidStreamLifecycle.enabled = false
                resumeStreams()
                val model = withContext(Dispatchers.Main) { graphRule.newShell(store) }
                model.session.connect(BuildConfig.XMTP_BACKEND_URL, "", false)
                val owner = checkNotNull(model.session.active.value)
                val context = InstrumentationRegistry.getInstrumentation().targetContext
                val target =
                    SDKClient
                        .create(
                            context,
                            generateLocalSigner(),
                            ClientOptions(
                                backend = BackendSource.Options(BackendOptions(url = BuildConfig.XMTP_BACKEND_URL)),
                                storage = StorageOptions(location = StorageLocation.InMemory),
                                deviceSync = false,
                            ),
                        ).also { peer = it }
                val chat = Conversation.Dm(owner.client.conversations.createDm(target.inboxId()))
                assertFalse(conversationState(chat).notificationsEnabled)
                assertEquals(
                    false,
                    model.conversations
                        .refresh(owner)
                        .single { it.id == chat.id() }
                        .muted,
                )
                val message = chat.sendText("Hello without a push registration")
                withTimeout(30_000) {
                    while (owner.client.conversations
                            .getMessageById(message)
                            ?.deliveryStatus !=
                        DeliveryStatus.PUBLISHED
                    ) {
                        delay(20)
                    }
                }
                assertEquals(
                    false,
                    model.conversations
                        .refresh(owner)
                        .single { it.id == chat.id() }
                        .muted,
                )
                graphRule.graph.notifications.setConversationEnabled(owner, chat, false)
                assertEquals(
                    true,
                    model.conversations
                        .refresh(owner)
                        .single { it.id == chat.id() }
                        .muted,
                )
                graphRule.graph.notifications.setConversationEnabled(owner, chat, true)
                assertEquals(
                    false,
                    model.conversations
                        .refresh(owner)
                        .single { it.id == chat.id() }
                        .muted,
                )
            } finally {
                withContext(NonCancellable) {
                    try {
                        peer?.end()
                    } finally {
                        try {
                            if (graphRule.graph.session.active.value != null) graphRule.graph.session.deleteAccount()
                        } finally {
                            withContext(Dispatchers.Main) { store.clear() }
                            AndroidStreamLifecycle.enabled = lifecycle
                        }
                    }
                }
            }
        }
}
