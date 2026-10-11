package org.xmtp.android.example.shared

import org.xmtp.android.example.shared.feature.chat.ChatFixtures
import org.xmtp.android.example.shared.feature.connect.ConnectFixtures
import org.xmtp.android.example.shared.feature.conversations.ConversationsFixtures
import org.xmtp.android.example.shared.feature.conversations.CreateConversationFixtures
import org.xmtp.android.example.shared.feature.group.GroupFixtures
import org.xmtp.android.example.shared.feature.settings.SettingsFixtures

data class MessengerFixture(
    val name: String,
    val state: MessengerUiState,
)

object MessengerFixtures {
    private val base =
        MessengerUiState(
            shell = ShellState(AppRoute.Chats, "sample-own-inbox"),
            connect = ConnectFixtures.populated,
            conversations = ConversationsFixtures.populated,
            chat = ChatFixtures.populated,
            create = CreateConversationFixtures.populated,
            group = GroupFixtures.populated,
            settings = SettingsFixtures.populated,
            request = ConversationsFixtures.request,
        )

    val scenarios =
        listOf(
            MessengerFixture(
                "Connect / empty",
                base.copy(shell = base.shell.copy(route = AppRoute.Connect), connect = ConnectFixtures.empty),
            ),
            MessengerFixture(
                "Connect / loading",
                base.copy(shell = base.shell.copy(route = AppRoute.Connect), connect = ConnectFixtures.loading),
            ),
            MessengerFixture("Connect / populated", base.copy(shell = base.shell.copy(route = AppRoute.Connect))),
            MessengerFixture(
                "Connect / failure",
                base.copy(shell = base.shell.copy(route = AppRoute.Connect), connect = ConnectFixtures.failure),
            ),
            MessengerFixture(
                "Connect / token",
                base.copy(shell = base.shell.copy(route = AppRoute.Connect), connect = ConnectFixtures.tokenRequired),
            ),
            MessengerFixture(
                "Connect / reset failure",
                base.copy(shell = base.shell.copy(route = AppRoute.Connect), connect = ConnectFixtures.resetFailure),
            ),
            MessengerFixture("Chats / empty", base.copy(conversations = ConversationsFixtures.empty)),
            MessengerFixture("Chats / loading", base.copy(conversations = ConversationsFixtures.loading)),
            MessengerFixture("Chats / populated", base),
            MessengerFixture("Chats / failure", base.copy(conversations = ConversationsFixtures.failure)),
            MessengerFixture("Chats / offline", base.copy(conversations = ConversationsFixtures.offline)),
            MessengerFixture(
                "Requests",
                base.copy(
                    shell = base.shell.copy(route = AppRoute.Requests),
                    conversations = ConversationsFixtures.requests,
                ),
            ),
            MessengerFixture(
                "Create / recipients",
                base.copy(shell = base.shell.copy(route = AppRoute.CreateConversation)),
            ),
            MessengerFixture(
                "Create / review",
                base.copy(
                    shell = base.shell.copy(route = AppRoute.CreateConversation),
                    create = CreateConversationFixtures.review,
                ),
            ),
            MessengerFixture(
                "Create / failure",
                base.copy(
                    shell = base.shell.copy(route = AppRoute.CreateConversation),
                    create = CreateConversationFixtures.failure,
                ),
            ),
            MessengerFixture(
                "Chat / empty",
                base.copy(
                    shell = base.shell.copy(route = AppRoute.Chat("conversation-alex")),
                    chat = ChatFixtures.empty,
                ),
            ),
            MessengerFixture(
                "Chat / loading",
                base.copy(
                    shell = base.shell.copy(route = AppRoute.Chat("conversation-alex")),
                    chat = ChatFixtures.loading,
                ),
            ),
            MessengerFixture(
                "Chat / populated",
                base.copy(shell = base.shell.copy(route = AppRoute.Chat("conversation-alex"))),
            ),
            MessengerFixture(
                "Chat / failure",
                base.copy(
                    shell = base.shell.copy(route = AppRoute.Chat("conversation-alex")),
                    chat = ChatFixtures.failure,
                ),
            ),
            MessengerFixture(
                "Chat / reply",
                base.copy(
                    shell = base.shell.copy(route = AppRoute.Chat("conversation-alex")),
                    chat = ChatFixtures.reply,
                ),
            ),
            MessengerFixture(
                "Chat / queued",
                base.copy(
                    shell = base.shell.copy(route = AppRoute.Chat("conversation-alex")),
                    chat = ChatFixtures.queued,
                ),
            ),
            MessengerFixture(
                "Chat / unknown",
                base.copy(
                    shell = base.shell.copy(route = AppRoute.Chat("conversation-alex")),
                    chat = ChatFixtures.unknown,
                ),
            ),
            MessengerFixture(
                "Chat / accepted commit failure",
                base.copy(
                    shell = base.shell.copy(route = AppRoute.Chat("conversation-alex")),
                    chat = ChatFixtures.acceptedPendingCommit,
                ),
            ),
            MessengerFixture(
                "Group / populated",
                base.copy(shell = base.shell.copy(route = AppRoute.ConversationDetails("conversation-team"))),
            ),
            MessengerFixture(
                "Group / failure",
                base.copy(
                    shell = base.shell.copy(route = AppRoute.ConversationDetails("conversation-team")),
                    group = GroupFixtures.failure,
                ),
            ),
            MessengerFixture(
                "Group / pending leave",
                base.copy(
                    shell = base.shell.copy(route = AppRoute.ConversationDetails("conversation-team")),
                    group = GroupFixtures.pendingLeave,
                ),
            ),
            MessengerFixture("Settings / populated", base.copy(shell = base.shell.copy(route = AppRoute.Settings))),
            MessengerFixture(
                "Settings / failure",
                base.copy(shell = base.shell.copy(route = AppRoute.Settings), settings = SettingsFixtures.failure),
            ),
            MessengerFixture(
                "Settings / permission denied",
                base.copy(
                    shell = base.shell.copy(route = AppRoute.Settings),
                    settings = SettingsFixtures.permissionDenied,
                ),
            ),
            MessengerFixture(
                "Settings / unconfigured",
                base.copy(shell = base.shell.copy(route = AppRoute.Settings), settings = SettingsFixtures.unconfigured),
            ),
        )
}
