package org.xmtp.android.example.messenger
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.xmtp.android.example.shared.*

class ComposeActionInstrumentedTest {
    @get:Rule val compose = createComposeRule()

    @Test fun screenControlsDispatchTheCurrentAppActions() {
        val state = mutableStateOf(MessengerState(backend = "http://example.test"))
        val actions = mutableListOf<MessengerAction>()
        compose.setContent {
            MessengerScreens(
                state.value,
                {
                    actions
                        .add(it)
                },
            )
        }
        compose
            .onNodeWithText("Connect")
            .performClick()
        val connect =
            actions.single {
                it is MessengerAction.Connect
            } as MessengerAction.Connect
        assertEquals(
            "http://example.test",
            connect.backend,
        )
        compose.runOnIdle {
            state.value =
                MessengerState(
                    screen =
                        Screen.CONVERSATIONS,
                    conversations =
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
                )
        }
        compose
            .onNodeWithText("First conversation")
            .performClick()
        val open =
            MessengerAction
                .OpenConversation("one")
        assertTrue(
            actions
                .contains(open),
        )
        compose
            .onNodeWithContentDescription("New conversation")
            .performClick()
        assertTrue(
            actions
                .contains(
                    MessengerAction
                        .Navigate(
                            Screen.CREATE,
                        ),
                ),
        )
        compose.runOnIdle {
            state.value =
                MessengerState(
                    screen =
                        Screen.TIMELINE,
                    conversationId = "one",
                    conversationTitle = "First conversation",
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
                                "Delivered",
                            ),
                        ),
                )
        }
        compose
            .onNodeWithText(
                "Message",
                substring = false,
            ).performTextInput("New text")
        compose
            .onNodeWithText("Send")
            .performClick()
        assertTrue(
            actions
                .contains(
                    MessengerAction
                        .SendText("New text"),
                ),
        )
        compose
            .onNodeWithText("Hello")
            .performClick()
        compose
            .onNodeWithText("Reply")
            .performClick()
        assertTrue(
            actions
                .contains(
                    MessengerAction
                        .Reply("message"),
                ),
        )
        compose
            .onNodeWithContentDescription("Conversation settings")
            .performClick()
        assertTrue(
            actions
                .contains(
                    MessengerAction
                        .Navigate(
                            Screen.CONVERSATION_SETTINGS,
                        ),
                ),
        )
    }
}
