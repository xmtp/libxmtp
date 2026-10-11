package org.xmtp.android.example.shared.app

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith
import androidx.compose.animation.core.tween
import androidx.compose.foundation.clickable
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import org.xmtp.android.example.shared.*
import org.xmtp.android.example.shared.designsystem.MessengerTheme
import org.xmtp.android.example.shared.designsystem.MessengerMotion
import org.xmtp.android.example.shared.feature.chat.ChatScreen
import org.xmtp.android.example.shared.feature.connect.ConnectScreen
import org.xmtp.android.example.shared.feature.conversations.ConversationsScreen
import org.xmtp.android.example.shared.feature.conversations.CreateConversationScreen
import org.xmtp.android.example.shared.feature.conversations.RequestsScreen
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
    MessengerTheme(state.shell.appearance, state.shell.reducedMotion) {
        Surface(Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.background) {
            BoxWithConstraints(Modifier.fillMaxSize().safeDrawingPadding().imePadding()) {
                val split = maxWidth >= 840.dp && state.shell.route != AppRoute.Connect && state.shell.route != AppRoute.Requests && !state.conversations.unknownTab
                if (split) {
                    Row(Modifier.fillMaxSize()) {
                        Surface(Modifier.width(340.dp).fillMaxHeight(), color = MaterialTheme.colorScheme.surface) {
                            Column {
                                ShellHeader(AppRoute.Chats, state, actions)
                                val selected = when (val route = state.shell.route) {
                                    is AppRoute.Chat -> route.conversationId
                                    is AppRoute.ConversationDetails -> route.conversationId
                                    is AppRoute.GroupFields -> route.conversationId
                                    is AppRoute.OwnFields -> route.conversationId
                                    else -> null
                                }
                                ConversationsScreen(state.conversations.copy(selectedConversationId = selected), actions.conversations, actions.shell)
                            }
                        }
                        VerticalDivider()
                        Box(Modifier.weight(1f)) {
                            if (state.shell.route == AppRoute.Chats) {
                                Column(Modifier.align(Alignment.Center).padding(32.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(12.dp)) {
                                    MessengerAvatar("XMTP", size = 72.dp)
                                    Text("Your conversations, together", style = MaterialTheme.typography.headlineMedium)
                                    Text("Choose a chat or start a new conversation.", color = MaterialTheme.colorScheme.onSurfaceVariant)
                                    Button({ actions.shell(ShellAction.Navigate(AppRoute.CreateConversation)) }) { Text("New message") }
                                }
                            } else {
                                RouteContent(state.shell.route, state, actions, extraScreen, composerExtra, messageExtra)
                            }
                        }
                    }
                } else {
                    val duration = if (state.shell.reducedMotion) 0 else MessengerMotion.ROUTE
                    AnimatedContent(state.shell.route, transitionSpec = { fadeIn(tween(duration)) togetherWith fadeOut(tween(duration)) }, label = "Screen") { route ->
                        RouteContent(route, state, actions, extraScreen, composerExtra, messageExtra)
                    }
                }
            }
        }
    }
}

@Composable
private fun RouteContent(route: AppRoute, state: MessengerUiState, actions: MessengerCallbacks, extraScreen: @Composable (Screen) -> Unit, composerExtra: @Composable () -> Unit, messageExtra: @Composable (MessageRow) -> Unit) {
    Column(Modifier.fillMaxSize()) {
        if (route != AppRoute.Connect) ShellHeader(route, state, actions)
        Box(Modifier.weight(1f)) {
            when (route) {
                AppRoute.Connect -> ConnectScreen(state.connect, actions.connect, actions.shell)
                AppRoute.Chats -> ConversationsScreen(state.conversations.copy(selectedConversationId = null), actions.conversations, actions.shell)
                AppRoute.Requests -> ConversationsScreen(state.conversations.copy(unknownTab = true), actions.conversations, actions.shell)
                AppRoute.CreateConversation -> CreateConversationScreen(state.create, actions.create)
                is AppRoute.Chat -> Column {
                    if (state.chat.unknown) Box(Modifier.heightIn(max = 240.dp).verticalScroll(rememberScrollState())) { RequestsScreen(state.request, actions.request, actions.shell) }
                    ChatScreen(state.chat, actions.chat, actions.shell, composerExtra, messageExtra)
                }
                is AppRoute.ConversationDetails -> ConversationDetailsScreen(state.group, actions.group, actions.shell, messageExtra)
                AppRoute.Settings -> SettingsScreen(state.settings, actions.settings, actions.shell)
                else -> extraScreen(route.screen())
            }
        }
    }
}

@Composable
private fun ShellHeader(route: AppRoute, state: MessengerUiState, actions: MessengerCallbacks) {
    Surface(color = MaterialTheme.colorScheme.surface, tonalElevation = 0.dp) {
        Row(Modifier.fillMaxWidth().heightIn(min = 76.dp).padding(horizontal = 12.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
            if (route != AppRoute.Chats) CircleAction("Back", AppIcons.Back) { actions.shell(ShellAction.Back) }
            if (route is AppRoute.Chat) {
                Row(Modifier.weight(1f).clickable { actions.shell(ShellAction.Navigate(AppRoute.ConversationDetails(route.conversationId))) }.padding(horizontal = 12.dp, vertical = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                    MessengerAvatar(state.chat.title, size = 42.dp, group = state.chat.group)
                    Column(Modifier.weight(1f).padding(start = 12.dp)) {
                        Text(state.chat.title, style = MaterialTheme.typography.titleMedium)
                        Text(if (state.chat.unknown) "Message request" else state.chat.memberCount?.let { "$it members" } ?: "Conversation details", style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                }
                CircleAction("Conversation details", AppIcons.More) { actions.shell(ShellAction.Navigate(AppRoute.ConversationDetails(route.conversationId))) }
            } else {
                Text(when (route) {
                    AppRoute.Chats -> "Chats"
                    AppRoute.Requests -> "Message requests"
                    AppRoute.CreateConversation -> "New message"
                    is AppRoute.ConversationDetails -> "Details"
                    AppRoute.Settings -> "Settings"
                    is AppRoute.GroupFields, is AppRoute.OwnFields -> "Developer tools"
                    AppRoute.AttachmentRecovery -> "Attachment recovery"
                    else -> "XMTP"
                }, Modifier.weight(1f).padding(horizontal = 12.dp), style = if (route == AppRoute.Chats) MaterialTheme.typography.headlineMedium else MaterialTheme.typography.titleLarge, fontWeight = FontWeight.Bold)
                if (route == AppRoute.Chats) {
                    CircleAction("New message", AppIcons.Add) { actions.shell(ShellAction.Navigate(AppRoute.CreateConversation)) }
                    Spacer(Modifier.width(8.dp))
                    CircleAction("Settings", AppIcons.Settings) { actions.shell(ShellAction.Navigate(AppRoute.Settings)) }
                }
            }
        }
    }
    HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
}
