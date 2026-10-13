package org.xmtp.android.example.messenger

import android.content.Intent
import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import org.xmtp.android.example.messenger.app.AppGraph
import org.xmtp.android.example.messenger.app.RouteOwner
import org.xmtp.android.example.shared.*
import org.xmtp.android.example.shared.attachments.AttachmentAction
import org.xmtp.android.example.shared.feature.chat.ChatAction
import org.xmtp.android.example.shared.feature.connect.ConnectAction
import org.xmtp.android.example.shared.feature.conversations.*
import org.xmtp.android.example.shared.feature.group.GroupAction
import org.xmtp.android.example.shared.feature.settings.SettingsAction

/** The shell binds routes and translates the current screen API to feature actions. */
internal class MessengerViewModel(
    val graph: AppGraph,
    savedStateHandle: SavedStateHandle,
) : ViewModel() {
    val session = graph.session
    val notifications = graph.notifications
    val sends = graph.sends
    val connect = graph.connect
    val conversations = graph.conversations
    val chat = graph.chat
    val group = graph.group
    val settings = graph.settings
    val createConversation = graph.createConversation
    val attachments = graph.attachments
    val projection = graph.projection
    val handles = graph.handles
    private val savedStateBinding = graph.bindSavedState(savedStateHandle, viewModelScope)
    private val display =
        FeatureStateProjection(
            session,
            connect,
            conversations,
            chat,
            createConversation,
            group,
            settings,
            attachments,
            notifications,
            graph.destinations,
            graph.navigationFailure,
            handles,
            viewModelScope,
        )
    val state = display.state
    val metadataState = group.metadataState

    init {
        graph.start()
    }

    override fun onCleared() {
        savedStateBinding.close()
    }

    suspend fun openPush(intent: Intent) = graph.openPush(intent)

    fun foreground(value: Boolean) = graph.foreground(value)

    fun dispatch(action: MessengerAction) {
        when (action) {
            is MessengerAction.ConnectFeature -> {
                connect.dispatch(action.action)
            }

            is MessengerAction.ConversationsFeature -> {
                dispatchConversations(action.action)
            }

            is MessengerAction.ChatFeature -> {
                dispatchChat(action.action)
            }

            is MessengerAction.CreateFeature -> {
                createConversation.dispatch(action.action)
            }

            is MessengerAction.GroupFeature -> {
                group.dispatch(action.action)
            }

            is MessengerAction.SettingsFeature -> {
                settings.dispatch(action.action)
            }

            is MessengerAction.RequestFeature -> {
                conversations.dispatch(action.action)
            }

            is MessengerAction.ShellFeature -> {
                graph.dispatch(action.action)
            }

            is MessengerAction.HandleFeature -> {
                handles.dispatch(action.action)
            }

            is MessengerAction.Connect -> {
                connect.dispatch(ConnectAction.Connect(action.backend, action.credential))
            }

            is MessengerAction.InspectBackend -> {
                connect.dispatch(ConnectAction.Inspect(action.backend))
            }

            is MessengerAction.Navigate -> {
                graph.dispatch(ShellAction.Navigate(route(action.screen)))
            }

            is MessengerAction.OpenConversation -> {
                conversations.dispatch(ConversationsAction.OpenConversation(action.id))
            }

            is MessengerAction.SelectTab -> {
                conversations.dispatch(ConversationsAction.SelectTab(action.unknown))
            }

            is MessengerAction.ListViewport -> {
                conversations.dispatch(ConversationsAction.Viewport(action.firstIndex, action.visibleCount))
            }

            MessengerAction.LoadMoreConversations -> {
                conversations.dispatch(ConversationsAction.LoadMore)
            }

            MessengerAction.Refresh -> {
                graph.dispatch(ShellAction.Refresh)
            }

            MessengerAction.RetryReader -> {
                graph.dispatch(ShellAction.RetryReader)
            }

            MessengerAction.LoadOlderRecovery -> {
                chat.dispatch(ChatAction.LoadOlderRecovery)
            }

            MessengerAction.LatestRecovery -> {
                chat.dispatch(ChatAction.LatestRecovery)
            }

            MessengerAction.LoadOlder -> {
                chat.dispatch(ChatAction.LoadOlder)
            }

            MessengerAction.JumpToLatest -> {
                chat.dispatch(ChatAction.JumpToLatest)
            }

            is MessengerAction.Viewport -> {
                chat.dispatch(ChatAction.Viewport(action.anchor, action.atNewest))
            }

            is MessengerAction.SendText -> {
                chat.dispatch(ChatAction.ChangeDraft(action.text, action.text.length, action.text.length))
                chat.dispatch(ChatAction.SendText(action.text, action.requestId, chat.state.value.composer.revision))
            }

            is MessengerAction.Reply -> {
                chat.dispatch(ChatAction.Reply(action.messageId))
            }

            is MessengerAction.React -> {
                chat.dispatch(ChatAction.React(action.messageId, action.emoji, action.remove))
            }

            is MessengerAction.RetrySend -> {
                chat.dispatch(ChatAction.RetrySend(action.messageId))
            }

            is MessengerAction.DeleteMessage -> {
                chat.dispatch(ChatAction.DeleteMessage(action.messageId))
            }

            is MessengerAction.Consent -> {
                conversations.consent(action.allowed)
            }

            is MessengerAction.Create -> {
                createConversation.dispatch(CreateConversationAction.ChangeMode(action.group))
                createConversation.dispatch(CreateConversationAction.ChangeRecipients(action.recipients))
                createConversation.dispatch(CreateConversationAction.ChangeName(action.name))
                createConversation.dispatch(CreateConversationAction.ChangeDescription(action.description))
                createConversation.dispatch(CreateConversationAction.ChangePreset(action.adminOnly))
                createConversation.dispatch(CreateConversationAction.Create)
            }

            is MessengerAction.UpdateGroup -> {
                group.dispatch(GroupAction.UpdateGroup(action.name, action.description))
            }

            is MessengerAction.SetPreset -> {
                group.dispatch(GroupAction.SetPreset(action.adminOnly))
            }

            is MessengerAction.AddMember -> {
                group.dispatch(GroupAction.AddMember(action.inboxId))
            }

            is MessengerAction.RemoveMember -> {
                group.dispatch(GroupAction.RemoveMember(action.inboxId))
            }

            is MessengerAction.SetAdmin -> {
                group.dispatch(GroupAction.SetAdmin(action.inboxId, action.admin))
            }

            is MessengerAction.SetDisappearing -> {
                group.dispatch(GroupAction.SetDisappearing(action.seconds))
            }

            MessengerAction.RequestRemoval -> {
                group.dispatch(GroupAction.RequestRemoval)
            }

            MessengerAction.SignOut -> {
                settings.dispatch(SettingsAction.SignOut)
            }

            MessengerAction.DeleteAccount -> {
                settings.dispatch(SettingsAction.DeleteLocalData)
            }

            MessengerAction.ResumeReset -> {
                connect.dispatch(ConnectAction.ResumeReset)
            }

            is MessengerAction.ResetLegacyAccount -> {
                connect.dispatch(ConnectAction.ResetLegacyAccount(action.inboxId))
            }

            is MessengerAction.DiscardUnknownSend -> {
                settings.dispatch(SettingsAction.DiscardUnknownSend(action.draftId))
            }

            is MessengerAction.PickReaction -> {
                chat.dispatch(ChatAction.ShowEmoji(action.messageId))
            }

            MessengerAction.SelectAttachment -> {
                graph.dispatchAttachment(AttachmentAction.Select)
            }

            is MessengerAction.OpenAttachment -> {
                graph.dispatchAttachment(AttachmentAction.Download(action.messageId))
            }

            is MessengerAction.SetAppNotifications -> {
                settings.dispatch(SettingsAction.SetNotifications(action.enabled))
            }

            is MessengerAction.SetConversationNotifications -> {
                group.dispatch(GroupAction.SetNotifications(action.enabled))
            }
        }
    }

    private fun dispatchConversations(action: ConversationsAction) {
        if (action is ConversationsAction.Create) {
            createConversation.dispatch(
                CreateConversationAction.ChangeMode(action.group),
            )
        }
        conversations.dispatch(action)
    }

    private fun dispatchChat(action: ChatAction) {
        when (action) {
            ChatAction.SelectAttachment -> graph.dispatchAttachment(AttachmentAction.Select)
            is ChatAction.OpenAttachment -> graph.dispatchAttachment(AttachmentAction.Download(action.messageId))
            else -> chat.dispatch(action)
        }
    }

    private fun route(screen: Screen): AppRoute {
        val conversationId = RouteOwner.conversationId(graph.destinations.state.value.route)
        return when (screen) {
            Screen.START -> {
                AppRoute.Connect
            }

            Screen.CONVERSATIONS -> {
                AppRoute.Chats
            }

            Screen.CREATE -> {
                AppRoute.CreateConversation
            }

            Screen.TIMELINE -> {
                conversationId
                    ?.let { AppRoute.Chat(it) } ?: AppRoute.Chats
            }

            Screen.CONVERSATION_SETTINGS -> {
                conversationId
                    ?.let { AppRoute.ConversationDetails(it) }
                    ?: AppRoute.Chats
            }

            Screen.APP_SETTINGS -> {
                AppRoute.Settings
            }

            Screen.GROUP_FIELDS -> {
                conversationId
                    ?.let { AppRoute.GroupFields(it) } ?: AppRoute.Chats
            }

            Screen.MY_FIELDS -> {
                conversationId
                    ?.let { AppRoute.OwnFields(it) } ?: AppRoute.Chats
            }

            Screen.DRAFTS -> {
                AppRoute.AttachmentRecovery
            }

            Screen.CHOOSE_HANDLE -> {
                AppRoute.ChooseHandle
            }

            Screen.CHANGE_HANDLE -> {
                AppRoute.ChangeHandle
            }
        }
    }
}
