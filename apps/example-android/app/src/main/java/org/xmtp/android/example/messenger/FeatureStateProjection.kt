package org.xmtp.android.example.messenger

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.*
import org.xmtp.android.example.messenger.app.DestinationScope
import org.xmtp.android.example.messenger.feature.attachments.AttachmentController
import org.xmtp.android.example.messenger.feature.chat.ChatController
import org.xmtp.android.example.messenger.feature.connect.ConnectController
import org.xmtp.android.example.messenger.feature.conversations.ConversationsController
import org.xmtp.android.example.messenger.feature.conversations.CreateConversationController
import org.xmtp.android.example.messenger.feature.group.GroupController
import org.xmtp.android.example.messenger.feature.settings.SettingsController
import org.xmtp.android.example.messenger.notifications.NotificationController
import org.xmtp.android.example.shared.*
import org.xmtp.android.example.shared.feature.chat.ChatState
import org.xmtp.android.example.shared.feature.chat.ComposerState
import org.xmtp.android.example.shared.feature.connect.ConnectState
import org.xmtp.android.example.shared.feature.conversations.*
import org.xmtp.android.example.shared.feature.group.GroupState
import org.xmtp.android.example.shared.feature.settings.SettingsState

/** This facade derives display state. Each feature retains its own source state. */
internal class FeatureStateProjection(
    session: AppSession,
    connect: ConnectController,
    conversations: ConversationsController,
    chat: ChatController,
    create: CreateConversationController,
    group: GroupController,
    settings: SettingsController,
    attachments: AttachmentController,
    notifications: NotificationController,
    destinations: DestinationScope,
    navigationFailure: StateFlow<AppFailure?>,
    scope: CoroutineScope,
) {
    val state =
        combine(
            listOf<Flow<Any?>>(
                connect.state,
                conversations.state,
                chat.state,
                create.state,
                group.state,
                settings.state,
                conversations.requestState,
                attachments.availability,
                destinations.state,
                session.active,
                session.readerError,
                session.connection,
                session.error,
                navigationFailure,
            ),
        ) { values ->
            val rawConnect = values[0] as ConnectState
            val rawList = values[1] as ConversationsState
            val retainedChat = values[2] as ChatState
            val rawCreate = values[3] as CreateConversationState
            val retainedGroup = values[4] as GroupState
            val rawSettings = values[5] as SettingsState
            val requestState = values[6] as RequestState
            val attachment = values[7] as CapabilityUi
            val route = (values[8] as org.xmtp.android.example.messenger.app.DestinationState).route
            val routeId =
                when (route) {
                    is AppRoute.Chat -> route.conversationId
                    is AppRoute.ConversationDetails -> route.conversationId
                    is AppRoute.GroupFields -> route.conversationId
                    is AppRoute.OwnFields -> route.conversationId
                    else -> null
                }
            val rawChat =
                if (routeId != null && retainedChat.conversationId != routeId) {
                    ChatState(
                        conversationId = routeId,
                        title = "Opening chat",
                        busy = true,
                        composer = ComposerState(enabled = false),
                    )
                } else {
                    retainedChat
                }
            val rawGroup =
                if (routeId != null && retainedGroup.conversationId != routeId) {
                    GroupState(
                        conversationId = routeId,
                        busy = true,
                    )
                } else {
                    retainedGroup
                }
            val owner = values[9] as ActiveSession?
            val reader =
                (values[10] as String?)?.let {
                    AppFailure(
                        "The message reader stopped. Try again.",
                        "reader",
                        FailureRecovery.RETRY_READER,
                    )
                }
            val connection = values[11] as String
            val sessionFailure =
                (values[12] as String?)?.let {
                    AppFailure(
                        "The account operation failed. Try again.",
                        "session",
                    )
                }
            val globalFailure = (values[13] as AppFailure?) ?: sessionFailure
            val connectState =
                rawConnect.copy(failure = rawConnect.failure ?: globalFailure.takeIf { route == AppRoute.Connect })
            val listState =
                rawList.copy(
                    connection = connection,
                    selectedConversationId = routeId ?: rawList.selectedConversationId,
                    failure =
                        rawList.failure
                            ?: globalFailure.takeIf { route == AppRoute.Chats || route == AppRoute.Requests },
                )
            val createState =
                rawCreate.copy(
                    failure =
                        rawCreate.failure ?: globalFailure.takeIf { route == AppRoute.CreateConversation },
                )
            val groupState =
                rawGroup.copy(
                    failure =
                        rawGroup.failure ?: globalFailure.takeIf {
                            route is AppRoute.ConversationDetails || route is AppRoute.GroupFields ||
                                route is AppRoute.OwnFields
                        },
                )
            val features = FeatureAvailability(attachment.available, owner != null, notifications.configured)
            val chatState =
                rawChat.copy(
                    features = features,
                    readerFailure = reader,
                    failure = rawChat.failure ?: globalFailure.takeIf { route is AppRoute.Chat },
                )
            val settingsState =
                rawSettings.copy(
                    attachments = attachment,
                    failure =
                        rawSettings.failure ?: globalFailure.takeIf {
                            route == AppRoute.Settings || route == AppRoute.AttachmentRecovery
                        },
                )
            val shell = ShellState(route, rawSettings.inbox, connection, reader, settingsState.appearance)
            val screen =
                when (route) {
                    AppRoute.Connect -> Screen.START

                    AppRoute.Chats,
                    AppRoute.Requests,
                    -> Screen.CONVERSATIONS

                    AppRoute.CreateConversation -> Screen.CREATE

                    is AppRoute.Chat -> Screen.TIMELINE

                    is AppRoute.ConversationDetails -> Screen.CONVERSATION_SETTINGS

                    is AppRoute.GroupFields -> Screen.GROUP_FIELDS

                    is AppRoute.OwnFields -> Screen.MY_FIELDS

                    AppRoute.Settings -> Screen.APP_SETTINGS

                    AppRoute.AttachmentRecovery -> Screen.DRAFTS
                }
            val failure =
                when (route) {
                    AppRoute.Connect -> connectState.failure

                    AppRoute.Chats,
                    AppRoute.Requests,
                    -> requestState.failure ?: listState.failure

                    AppRoute.CreateConversation -> createState.failure

                    is AppRoute.Chat -> chatState.failure

                    is AppRoute.ConversationDetails,
                    is AppRoute.GroupFields,
                    is AppRoute.OwnFields,
                    -> groupState.failure

                    AppRoute.Settings,
                    AppRoute.AttachmentRecovery,
                    -> settingsState.failure
                } ?: (values[13] as AppFailure?) ?: sessionFailure
            MessengerState(
                screen = screen,
                backend = owner?.profile?.backend ?: connectState.backend,
                credentialsRequiredFor = connectState.credentialsRequiredFor,
                inbox = settingsState.inbox,
                busy =
                    when (route) {
                        AppRoute.Connect -> connectState.busy

                        AppRoute.Chats, AppRoute.Requests -> listState.busy

                        AppRoute.CreateConversation -> createState.busy

                        is AppRoute.Chat -> chatState.busy

                        is AppRoute.ConversationDetails,
                        is AppRoute.GroupFields,
                        is AppRoute.OwnFields,
                        -> groupState.busy

                        AppRoute.Settings, AppRoute.AttachmentRecovery -> settingsState.busy
                    },
                error = failure?.message,
                pendingReset = connectState.pendingReset || settingsState.pendingReset,
                readerError = reader?.message,
                connection = connection,
                migrationRequired = connectState.migrationRequired,
                migrationAccounts = connectState.migrationAccounts,
                conversations = listState.rows,
                unknownTab = route == AppRoute.Requests || listState.unknownTab,
                messages = chatState.messages,
                conversationId = chatState.conversationId,
                conversationTitle = chatState.title,
                conversationUnknown = chatState.unknown,
                textSendResult = chatState.composer.result,
                replyTo = chatState.composer.reply?.messageId,
                replyPreview = chatState.composer.reply?.firstLine,
                historyNotice = chatState.historyNotice,
                hasOlder = chatState.hasOlder,
                hasOlderRecovery = chatState.hasOlderRecovery,
                recoveryAtNewest = chatState.recoveryAtNewest,
                recoveryNotice = chatState.recoveryNotice,
                anchor = chatState.anchor,
                settings = groupState.settings,
                unknownSends = settingsState.unknownSends,
                features = features,
                notificationStatus = settingsState.notificationStatus,
                notificationsEnabled = settingsState.notificationsEnabled,
                shellState = shell,
                connectState = connectState,
                conversationsState = listState.copy(connection = connection),
                chatState = chatState,
                createState = createState,
                groupState = groupState,
                settingsState = settingsState,
                requestState = requestState,
            )
        }.stateIn(scope, SharingStarted.Eagerly, MessengerState())
}
