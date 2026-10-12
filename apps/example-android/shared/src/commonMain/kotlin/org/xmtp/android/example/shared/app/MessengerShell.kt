package org.xmtp.android.example.shared.app

import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import org.xmtp.android.example.shared.*
import org.xmtp.android.example.shared.feature.chat.ChatScreen
import org.xmtp.android.example.shared.feature.connect.ConnectScreen
import org.xmtp.android.example.shared.feature.conversations.ConversationsScreen
import org.xmtp.android.example.shared.feature.conversations.CreateConversationScreen
import org.xmtp.android.example.shared.feature.group.ConversationDetailsScreen
import org.xmtp.android.example.shared.feature.settings.SettingsScreen

@Composable
fun MessengerShell(
    state: MessengerUiState,
    actions: MessengerCallbacks,
    extraScreen: @Composable (Screen) -> Unit = {},
    composerExtra: @Composable () -> Unit = {},
    messageExtra: @Composable (MessageRow) -> Unit = {},
) {
    val route = state.shell.route
    val screen = route.screen()
    val failure =
        when (screen) {
            Screen.START -> state.connect.failure
            Screen.CONVERSATIONS -> state.conversations.failure
            Screen.CREATE -> state.create.failure
            Screen.TIMELINE -> state.chat.failure
            Screen.CONVERSATION_SETTINGS -> state.group.failure
            Screen.APP_SETTINGS -> state.settings.failure
            else -> null
        }
    val busy =
        when (screen) {
            Screen.START -> state.connect.busy
            Screen.CONVERSATIONS -> state.conversations.busy
            Screen.CREATE -> state.create.busy
            Screen.TIMELINE -> state.chat.busy
            Screen.CONVERSATION_SETTINGS -> state.group.busy
            Screen.APP_SETTINGS -> state.settings.busy
            else -> false
        }
    MaterialTheme(
        colorScheme =
            lightColorScheme(
                primary = blue,
                onPrimary = Color.White,
                background = Color.White,
                surface = Color.White,
                onSurface = ink,
                onBackground = ink,
                onSurfaceVariant = Color(0xFF657186),
            ),
    ) {
        Surface(Modifier.fillMaxSize()) {
            Column(Modifier.fillMaxSize().safeDrawingPadding()) {
                if (screen != Screen.START) {
                    Row(
                        Modifier.fillMaxWidth().padding(horizontal = 12.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        if (screen != Screen.CONVERSATIONS) {
                            CircleAction("Back", AppIcons.Back) { actions.shell(ShellAction.Back) }
                        }
                        Text(
                            when (screen) {
                                Screen.CONVERSATIONS -> "Conversations"
                                Screen.TIMELINE -> state.chat.title
                                Screen.CREATE -> "New conversation"
                                Screen.CONVERSATION_SETTINGS -> "Conversation settings"
                                Screen.APP_SETTINGS -> "Settings"
                                Screen.GROUP_FIELDS -> "Group fields"
                                Screen.MY_FIELDS -> "My fields"
                                Screen.DRAFTS -> "Draft recovery"
                                else -> "XMTP"
                            },
                            Modifier.weight(1f).padding(12.dp),
                            style = MaterialTheme.typography.titleLarge,
                            fontWeight = FontWeight.Bold,
                        )
                        if (screen == Screen.CONVERSATIONS) {
                            CircleAction(
                                "New conversation",
                                AppIcons.Add,
                            ) { actions.shell(ShellAction.Navigate(AppRoute.CreateConversation)) }
                            CircleAction(
                                "Settings",
                                AppIcons.Settings,
                            ) { actions.shell(ShellAction.Navigate(AppRoute.Settings)) }
                        }
                        if (route is AppRoute.Chat) {
                            CircleAction("Conversation settings", AppIcons.More) {
                                actions.shell(ShellAction.Navigate(AppRoute.ConversationDetails(route.conversationId)))
                            }
                        }
                    }
                }
                failure?.takeIf { screen != Screen.START || state.connect.pendingReset }?.let {
                    Notice(it.message, "Retry") {
                        if (state.connect.pendingReset) {
                            actions.connect(org.xmtp.android.example.shared.feature.connect.ConnectAction.ResumeReset)
                        } else {
                            actions.shell(ShellAction.Refresh)
                        }
                    }
                }
                if (busy) LinearProgressIndicator(Modifier.fillMaxWidth())
                Box(Modifier.weight(1f)) {
                    when (screen) {
                        Screen.START -> {
                            if (state.connect.pendingReset) {
                                Text(
                                    "Finish local reset before you connect.",
                                )
                            } else {
                                ConnectScreen(state.connect, actions.connect, actions.shell)
                            }
                        }

                        Screen.CONVERSATIONS -> {
                            ConversationsScreen(
                                state.conversations,
                                actions.conversations,
                                actions.shell,
                            )
                        }

                        Screen.CREATE -> {
                            CreateConversationScreen(state.create, actions.create)
                        }

                        Screen.TIMELINE -> {
                            ChatScreen(
                                state.chat,
                                actions.chat,
                                actions.shell,
                                composerExtra,
                                messageExtra,
                            )
                        }

                        Screen.CONVERSATION_SETTINGS -> {
                            ConversationDetailsScreen(
                                state.group,
                                actions.group,
                                actions.shell,
                            )
                        }

                        Screen.APP_SETTINGS -> {
                            SettingsScreen(state.settings, actions.settings, actions.shell)
                        }

                        else -> {
                            extraScreen(screen)
                        }
                    }
                }
            }
        }
    }
}
