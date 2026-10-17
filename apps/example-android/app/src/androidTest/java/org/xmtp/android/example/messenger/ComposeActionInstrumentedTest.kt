package org.xmtp.android.example.messenger

import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.xmtp.android.example.shared.*
import org.xmtp.android.example.shared.feature.chat.*
import org.xmtp.android.example.shared.feature.connect.*
import org.xmtp.android.example.shared.feature.conversations.*

class ComposeActionInstrumentedTest {
    @get:Rule val compose = createComposeRule()

    @Test fun screenControlsDispatchTheCurrentAppActions() {
        val state =
            mutableStateOf(
                MessengerUiState(
                    connect = ConnectState(backend = "http://example.test"),
                ),
            )
        val actions = mutableListOf<Any>()
        compose.setContent {
            MessengerScreens(
                state.value,
                MessengerCallbacks(
                    shell = { actions.add(it) },
                    connect = { actions.add(it) },
                    conversations = { actions.add(it) },
                    chat = { action ->
                        actions.add(action)
                        // Return controlled input state. The fixture does not perform commands.
                        when (action) {
                            is ChatAction.ChangeDraft -> {
                                state.value =
                                    state.value.copy(
                                        chat =
                                            state.value.chat.copy(
                                                composer =
                                                    ComposerState(
                                                        text = action.text,
                                                        selectionStart = action.selectionStart,
                                                        selectionEnd = action.selectionEnd,
                                                        revision = 7,
                                                    ),
                                            ),
                                    )
                            }

                            is ChatAction.SelectMessage -> {
                                state.value =
                                    state.value.copy(
                                        chat =
                                            state.value.chat.copy(
                                                overlay = MessageOverlayState(action.messageId),
                                            ),
                                    )
                            }

                            is ChatAction.Reply -> {
                                state.value =
                                    state.value.copy(
                                        chat = state.value.chat.copy(overlay = null),
                                    )
                            }

                            else -> {
                                Unit
                            }
                        }
                    },
                ),
            )
        }
        compose.onNodeWithText("Join a server").performScrollTo().performClick()
        val connect = actions.filterIsInstance<ConnectAction.Connect>().single()
        assertEquals("http://example.test", connect.backend)

        compose.runOnIdle {
            state.value =
                MessengerUiState(
                    shell = ShellState(route = AppRoute.Chats),
                    conversations =
                        ConversationsState(
                            rows =
                                listOf(
                                    ConversationRow(
                                        "one",
                                        "First conversation",
                                        "Preview",
                                        "12:00",
                                        "1",
                                        false,
                                        1,
                                    ),
                                ),
                        ),
                )
        }
        compose.onNodeWithText("First conversation").performClick()
        assertTrue(actions.contains(ConversationsAction.OpenConversation("one")))
        compose.onNodeWithContentDescription("New message").performClick()
        assertTrue(actions.contains(ShellAction.Navigate(AppRoute.CreateConversation)))

        compose.runOnIdle {
            state.value =
                MessengerUiState(
                    shell = ShellState(route = AppRoute.Chat("one")),
                    chat =
                        ChatState(
                            conversationId = "one",
                            title = "First conversation",
                            messages =
                                listOf(
                                    MessageRow(
                                        "message",
                                        "You",
                                        "Hello",
                                        "12:00",
                                        "Oct 8",
                                        100,
                                        true,
                                        "Sent",
                                    ),
                                ),
                        ),
                )
        }
        compose.onNodeWithText("Message", substring = false).performTextInput("New text")
        compose.onNodeWithContentDescription("Send message").performClick()
        val sent = actions.filterIsInstance<ChatAction.SendText>().single()
        assertEquals("New text", sent.text)
        assertEquals(
            "Send carries the displayed draft revision",
            7L,
            sent.submittedRevision,
        )
        compose.onNodeWithText("Hello").performClick()
        compose.onNodeWithText("Reply").performClick()
        assertTrue(actions.contains(ChatAction.Reply("message")))
        compose.onNodeWithContentDescription("Conversation details").performClick()
        assertTrue(actions.contains(ShellAction.Navigate(AppRoute.ConversationDetails("one"))))
    }
}
