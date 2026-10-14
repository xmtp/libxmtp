package org.xmtp.android.example.shared

import org.xmtp.android.example.shared.feature.chat.*
import org.xmtp.android.example.shared.feature.connect.*
import org.xmtp.android.example.shared.feature.conversations.*
import org.xmtp.android.example.shared.feature.group.*
import org.xmtp.android.example.shared.feature.handles.*
import org.xmtp.android.example.shared.feature.settings.*

data class MessengerUiState(
    val shell: ShellState = ShellState(),
    val connect: ConnectState = ConnectState(),
    val conversations: ConversationsState = ConversationsState(),
    val chat: ChatState = ChatState(),
    val create: CreateConversationState = CreateConversationState(),
    val group: GroupState = GroupState(),
    val settings: SettingsState = SettingsState(),
    val request: RequestState = RequestState(),
    val handles: HandleState = HandleState(),
)

data class MessengerCallbacks(
    val shell: (ShellAction) -> Unit = {},
    val connect: (ConnectAction) -> Unit = {},
    val conversations: (ConversationsAction) -> Unit = {},
    val chat: (ChatAction) -> Unit = {},
    val create: (CreateConversationAction) -> Unit = {},
    val group: (GroupAction) -> Unit = {},
    val settings: (SettingsAction) -> Unit = {},
    val request: (RequestAction) -> Unit = {},
    val handles: (HandleAction) -> Unit = {},
)

fun Screen.route(conversationId: String?): AppRoute =
    when (this) {
        Screen.START -> AppRoute.Connect
        Screen.CONVERSATIONS -> AppRoute.Chats
        Screen.CREATE -> AppRoute.CreateConversation
        Screen.TIMELINE -> conversationId?.let { AppRoute.Chat(it) } ?: AppRoute.Chats
        Screen.CONVERSATION_SETTINGS -> conversationId?.let { AppRoute.ConversationDetails(it) } ?: AppRoute.Chats
        Screen.APP_SETTINGS -> AppRoute.Settings
        Screen.GROUP_FIELDS -> conversationId?.let { AppRoute.GroupFields(it) } ?: AppRoute.Chats
        Screen.MY_FIELDS -> conversationId?.let { AppRoute.OwnFields(it) } ?: AppRoute.Chats
        Screen.DRAFTS -> AppRoute.AttachmentRecovery
        Screen.CHOOSE_HANDLE -> AppRoute.ChooseHandle
        Screen.CHANGE_HANDLE -> AppRoute.ChangeHandle
    }

fun AppRoute.screen(): Screen =
    when (this) {
        AppRoute.Connect -> Screen.START
        AppRoute.Chats, AppRoute.Requests -> Screen.CONVERSATIONS
        AppRoute.CreateConversation -> Screen.CREATE
        is AppRoute.Chat -> Screen.TIMELINE
        is AppRoute.ConversationDetails -> Screen.CONVERSATION_SETTINGS
        AppRoute.Settings -> Screen.APP_SETTINGS
        is AppRoute.GroupFields -> Screen.GROUP_FIELDS
        is AppRoute.OwnFields -> Screen.MY_FIELDS
        AppRoute.AttachmentRecovery -> Screen.DRAFTS
        AppRoute.ChooseHandle -> Screen.CHOOSE_HANDLE
        AppRoute.ChangeHandle -> Screen.CHANGE_HANDLE
    }

// This adapter supports the current Activity while it moves to feature states.
fun MessengerState.toUiState(): MessengerUiState {
    val failure = error?.let { AppFailure(it, "legacy-operation", FailureRecovery.RETRY) }
    val readerFailure = readerError?.let { AppFailure(it, "reader", FailureRecovery.RETRY_READER) }
    val reply =
        messages
            .firstOrNull {
                it.id == replyTo
            }?.let {
                ReplyTarget(
                    it.id,
                    it.sender,
                    it.text
                        .lineSequence()
                        .firstOrNull()
                        .orEmpty(),
                )
            }
    return MessengerUiState(
        shell = shellState ?: ShellState(screen.route(conversationId), inbox, connection, readerFailure),
        connect =
            connectState
                ?: ConnectState(
                    backend = backend,
                    credentialsRequiredFor = credentialsRequiredFor,
                    busy = busy,
                    failure = failure,
                    pendingReset = pendingReset,
                    migrationRequired = migrationRequired,
                    migrationAccounts = migrationAccounts,
                ),
        conversations =
            conversationsState
                ?: ConversationsState(
                    rows = conversations,
                    unknownTab = unknownTab,
                    selectedConversationId = conversationId,
                    busy = busy,
                    connection = connection,
                    failure = failure,
                ),
        chat =
            chatState
                ?: ChatState(
                    conversationId = conversationId,
                    title = conversationTitle,
                    unknown = conversationUnknown,
                    messages = messages,
                    composer = ComposerState(reply = reply, result = textSendResult),
                    historyNotice = historyNotice,
                    hasOlder = hasOlder,
                    hasOlderRecovery = hasOlderRecovery,
                    recoveryAtNewest = recoveryAtNewest,
                    recoveryNotice = recoveryNotice,
                    anchor = anchor,
                    busy = busy,
                    failure = failure,
                    readerFailure = readerFailure,
                    features = features,
                ),
        create = createState ?: CreateConversationState(busy = busy, failure = failure),
        group =
            groupState
                ?: GroupState(
                    conversationId = conversationId,
                    settings = settings,
                    name = settings.title,
                    description = settings.description,
                    disappearingInput = settings.disappearingSeconds,
                    metadata = CapabilityUi(features.metadata),
                    notifications = CapabilityUi(features.notifications),
                    leave = CapabilityUi(settings.canRequestRemoval),
                    busy = busy,
                    failure = failure,
                ),
        settings =
            settingsState
                ?: SettingsState(
                    inbox = inbox,
                    backend = backend,
                    notifications = CapabilityUi(features.notifications),
                    attachments = CapabilityUi(features.attachments),
                    notificationStatus = notificationStatus,
                    notificationsEnabled = notificationsEnabled,
                    unknownSends = unknownSends,
                    pendingReset = pendingReset,
                    busy = busy,
                    failure = failure,
                ),
        request = requestState ?: RequestState(conversationId = conversationId.orEmpty(), title = conversationTitle),
        handles = handleState ?: HandleState(),
    )
}
