package org.xmtp.android.example.shared

import androidx.compose.runtime.Composable
import org.xmtp.android.example.shared.app.MessengerShell

@Composable
fun MessengerScreens(
    state: MessengerUiState,
    actions: MessengerCallbacks,
    extraScreen: @Composable (Screen) -> Unit = {},
    composerExtra: @Composable () -> Unit = {},
    messageExtra: @Composable (MessageRow) -> Unit = {},
) = MessengerShell(state, actions, extraScreen, composerExtra, messageExtra)

// The current Activity uses this adapter until it accepts feature states directly.
@Composable
fun MessengerScreens(
    state: MessengerState,
    action: (MessengerAction) -> Unit,
    extraScreen: @Composable (Screen) -> Unit = {},
    composerExtra: @Composable () -> Unit = {},
    messageExtra: @Composable (MessageRow) -> Unit = {},
) = MessengerScreens(
    state.toUiState(),
    MessengerCallbacks(
        shell = { action(MessengerAction.ShellFeature(it)) },
        connect = { action(MessengerAction.ConnectFeature(it)) },
        conversations = { action(MessengerAction.ConversationsFeature(it)) },
        chat = { action(MessengerAction.ChatFeature(it)) },
        create = { action(MessengerAction.CreateFeature(it)) },
        group = { action(MessengerAction.GroupFeature(it)) },
        settings = { action(MessengerAction.SettingsFeature(it)) },
        request = { action(MessengerAction.RequestFeature(it)) },
    ),
    extraScreen,
    composerExtra,
    messageExtra,
)
