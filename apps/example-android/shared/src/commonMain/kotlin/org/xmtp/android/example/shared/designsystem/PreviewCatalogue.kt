package org.xmtp.android.example.shared.designsystem

import androidx.compose.runtime.Composable
import org.xmtp.android.example.shared.*
import org.xmtp.android.example.shared.app.MessengerShell
import org.xmtp.android.example.shared.feature.chat.ChatFixtures
import org.xmtp.android.example.shared.feature.connect.ConnectFixtures
import org.xmtp.android.example.shared.feature.conversations.ConversationsFixtures
import org.xmtp.android.example.shared.feature.conversations.CreateConversationFixtures
import org.xmtp.android.example.shared.feature.group.GroupFixtures
import org.xmtp.android.example.shared.feature.handles.HandleFixtures
import org.xmtp.android.example.shared.feature.settings.SettingsFixtures

/** Use the same immutable samples for previews at each size, text scale, and layout direction. */
data class MessengerPreviewCase(
    val name: String,
    val state: MessengerUiState,
)

val messengerPreviewCases: List<MessengerPreviewCase> =
    buildList {
        listOf(
            "Welcome" to ConnectFixtures.empty,
            "Server ready" to ConnectFixtures.populated,
            "Checking server" to ConnectFixtures.loading,
            "Access token" to ConnectFixtures.tokenRequired,
            "Connection failure" to ConnectFixtures.failure,
            "Reset failure" to ConnectFixtures.resetFailure,
            "Migration" to ConnectFixtures.migration,
        ).forEach { (name, state) ->
            add(MessengerPreviewCase(name, MessengerUiState(connect = state)))
        }
        listOf(
            "Chats" to ConversationsFixtures.populated,
            "Empty chats" to ConversationsFixtures.empty,
            "Loading chats" to ConversationsFixtures.loading,
            "Offline" to ConversationsFixtures.offline,
            "Reconnecting" to ConversationsFixtures.reconnecting,
            "Requests" to ConversationsFixtures.requests,
            "Local search" to ConversationsFixtures.search,
            "Chats failure" to ConversationsFixtures.failure,
        ).forEach { (name, state) ->
            add(
                MessengerPreviewCase(
                    name,
                    MessengerUiState(shell = ShellState(AppRoute.Chats), conversations = state),
                ),
            )
        }
        listOf(
            "Chat" to ChatFixtures.populated,
            "Reply" to ChatFixtures.reply,
            "Selected message" to ChatFixtures.selected,
            "Reactors" to ChatFixtures.reactors,
            "History" to ChatFixtures.history,
            "Recovery" to ChatFixtures.recovery,
            "Sending" to ChatFixtures.sending,
            "Queued" to ChatFixtures.queued,
            "Sent" to ChatFixtures.sent,
            "Not sent" to ChatFixtures.notSent,
            "Unknown outcome" to ChatFixtures.unknown,
            "Send rejected" to ChatFixtures.rejected,
            "Accepted send finishing" to ChatFixtures.acceptedPendingCommit,
            "Long content" to ChatFixtures.longContent,
        ).forEach { (name, state) ->
            add(
                MessengerPreviewCase(
                    name,
                    MessengerUiState(
                        shell = ShellState(AppRoute.Chat("conversation-alex")),
                        conversations = ConversationsFixtures.populated,
                        chat = state,
                    ),
                ),
            )
        }
        add(
            MessengerPreviewCase(
                "Request details",
                MessengerUiState(
                    shell = ShellState(AppRoute.Chat("conversation-request")),
                    conversations = ConversationsFixtures.requests,
                    chat = ChatFixtures.request.copy(conversationId = "conversation-request", title = "New contact"),
                    request = ConversationsFixtures.request,
                ),
            ),
        )
        listOf(
            "Select recipients" to CreateConversationFixtures.populated,
            "Review group" to CreateConversationFixtures.review,
            "Create failure" to CreateConversationFixtures.failure,
        ).forEach { (name, state) ->
            add(
                MessengerPreviewCase(
                    name,
                    MessengerUiState(shell = ShellState(AppRoute.CreateConversation), create = state),
                ),
            )
        }
        listOf(
            "Details" to GroupFixtures.populated,
            "Pending member" to GroupFixtures.pendingMember,
            "Pending leave" to GroupFixtures.pendingLeave,
            "Unavailable action" to GroupFixtures.permissionFailure,
        ).forEach { (name, state) ->
            add(
                MessengerPreviewCase(
                    name,
                    MessengerUiState(
                        shell = ShellState(AppRoute.ConversationDetails("conversation-team")),
                        group = state,
                    ),
                ),
            )
        }
        listOf(
            "Settings" to SettingsFixtures.populated,
            "Notifications unavailable" to SettingsFixtures.unconfigured,
            "Notification permission denied" to SettingsFixtures.permissionDenied,
            "Settings reset failure" to SettingsFixtures.resetFailure,
            "Settings without a handle" to SettingsFixtures.populated.copy(handle = null, otherHandles = emptyList()),
        ).forEach { (name, state) ->
            add(
                MessengerPreviewCase(
                    name,
                    MessengerUiState(shell = ShellState(AppRoute.Settings), settings = state),
                ),
            )
        }
        listOf(
            "Choose handle" to HandleFixtures.empty,
            "Checking handle" to HandleFixtures.checking,
            "Handle available" to HandleFixtures.available,
            "Handle suggestions" to HandleFixtures.suggestions,
            "Claiming handle" to HandleFixtures.claiming,
            "Handle taken while claiming" to HandleFixtures.takenWhileClaiming,
            "Handle invalid characters" to HandleFixtures.invalidCharacters,
            "Handle starts with a digit" to HandleFixtures.startsWithDigit,
            "Handle too short" to HandleFixtures.tooShort,
            "Handle code not 4 digits" to HandleFixtures.badCode,
            "Handle too long" to HandleFixtures.tooLong,
            "Handle not allowed" to HandleFixtures.notAllowed,
            "No free handle codes" to HandleFixtures.noneFree,
            "Handle check failed" to HandleFixtures.checkFailed,
        ).forEach { (name, state) ->
            add(
                MessengerPreviewCase(
                    name,
                    MessengerUiState(shell = ShellState(AppRoute.ChooseHandle), handles = state),
                ),
            )
        }
        listOf(
            "Change handle" to HandleFixtures.changeAvailable,
            "Confirm handle change" to HandleFixtures.confirmChange,
            "Handle changed too soon" to HandleFixtures.tooSoon,
            "Handle switch back blocked" to HandleFixtures.switchBackBlocked,
            "All handle changes used" to HandleFixtures.limitReached,
            "Choose handle from Settings" to HandleFixtures.firstFromSettings,
            "Loading own handles in Settings" to HandleFixtures.changeLoading,
        ).forEach { (name, state) ->
            add(
                MessengerPreviewCase(
                    name,
                    MessengerUiState(shell = ShellState(AppRoute.ChangeHandle), handles = state),
                ),
            )
        }
    }

@Composable
fun MessengerPreview(
    case: MessengerPreviewCase,
    appearance: Appearance = Appearance.SYSTEM,
    reducedMotion: Boolean = true,
) {
    MessengerShell(
        case.state.copy(shell = case.state.shell.copy(appearance = appearance, reducedMotion = reducedMotion)),
        MessengerCallbacks(),
    )
}
