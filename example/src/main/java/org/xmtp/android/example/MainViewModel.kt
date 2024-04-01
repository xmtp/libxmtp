package org.xmtp.android.example

import androidx.annotation.UiThread
import androidx.annotation.WorkerThread
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.catch
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.flow.mapLatest
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import org.xmtp.android.example.extension.flowWhileShared
import org.xmtp.android.example.extension.stateFlow
import org.xmtp.android.example.pushnotifications.PushNotificationTokenManager
import org.xmtp.android.library.Conversation
import org.xmtp.android.library.DecodedMessage
import org.xmtp.android.library.messages.Topic
import org.xmtp.android.library.push.Service

class MainViewModel : ViewModel() {

    private val _uiState = MutableStateFlow<UiState>(UiState.Loading(null))
    val uiState: StateFlow<UiState> = _uiState

    @UiThread
    fun setupPush() {
        viewModelScope.launch(Dispatchers.IO) {
            PushNotificationTokenManager.ensurePushTokenIsConfigured()
        }
    }

    @UiThread
    fun fetchConversations() {
        when (val uiState = uiState.value) {
            is UiState.Success -> _uiState.value = UiState.Loading(uiState.listItems)
            else -> _uiState.value = UiState.Loading(null)
        }
        viewModelScope.launch(Dispatchers.IO) {
            val listItems = mutableListOf<MainListItem>()
            try {
                val conversations = ClientManager.client.conversations.list(includeGroups = true)
                val hmacKeysResult = ClientManager.client.conversations.getHmacKeys()
                val subscriptions: MutableList<Service.Subscription> = conversations.map {
                    val hmacKeys = hmacKeysResult.hmacKeysMap
                    val result = hmacKeys[it.topic]?.valuesList?.map { hmacKey ->
                        Service.Subscription.HmacKey.newBuilder().also { sub_key ->
                            sub_key.key = hmacKey.hmacKey
                            sub_key.thirtyDayPeriodsSinceEpoch = hmacKey.thirtyDayPeriodsSinceEpoch
                        }.build()
                    }

                    Service.Subscription.newBuilder().also { sub ->
                        if (!result.isNullOrEmpty()) {
                            sub.addAllHmacKeys(result)
                        }
                        sub.topic = it.topic
                        sub.isSilent = it.version == Conversation.Version.V1
                    }.build()
                }.toMutableList()

                val welcomeTopic = Service.Subscription.newBuilder().also { sub ->
                    sub.topic = Topic.userWelcome(ClientManager.client.installationId).description
                    sub.isSilent = false
                }.build()
                subscriptions.add(welcomeTopic)

                PushNotificationTokenManager.xmtpPush.subscribeWithMetadata(subscriptions)
                listItems.addAll(
                    conversations.map { conversation ->
                        val lastMessage = fetchMostRecentMessage(conversation)
                        MainListItem.ConversationItem(
                            id = conversation.topic,
                            conversation,
                            lastMessage
                        )
                    }
                )
                listItems.add(
                    MainListItem.Footer(
                        id = "footer",
                        ClientManager.client.address,
                        ClientManager.client.apiClient.environment.name
                    )
                )
                _uiState.value = UiState.Success(listItems)
            } catch (e: Exception) {
                _uiState.value = UiState.Error(e.localizedMessage.orEmpty())
            }
        }
    }

    @WorkerThread
    private fun fetchMostRecentMessage(conversation: Conversation): DecodedMessage? {
        return runBlocking { conversation.messages(limit = 1).firstOrNull() }
    }

    @OptIn(ExperimentalCoroutinesApi::class)
    val stream: StateFlow<MainListItem?> =
        stateFlow(viewModelScope, null) { subscriptionCount ->
            if (ClientManager.clientState.value is ClientManager.ClientState.Ready) {
                ClientManager.client.conversations.streamAll()
                    .flowWhileShared(
                        subscriptionCount,
                        SharingStarted.WhileSubscribed(1000L)
                    )
                    .flowOn(Dispatchers.IO)
                    .distinctUntilChanged()
                    .mapLatest { conversation ->
                        val lastMessage = fetchMostRecentMessage(conversation)
                        MainListItem.ConversationItem(conversation.topic, conversation, lastMessage)
                    }
                    .catch { emptyFlow<MainListItem>() }
            } else {
                emptyFlow()
            }
        }

    sealed class UiState {
        data class Loading(val listItems: List<MainListItem>?) : UiState()
        data class Success(val listItems: List<MainListItem>) : UiState()
        data class Error(val message: String) : UiState()
    }

    sealed class MainListItem(open val id: String, val itemType: Int) {
        companion object {
            const val ITEM_TYPE_CONVERSATION = 1
            const val ITEM_TYPE_FOOTER = 2
        }

        data class ConversationItem(
            override val id: String,
            val conversation: Conversation,
            val mostRecentMessage: DecodedMessage?,
        ) : MainListItem(id, ITEM_TYPE_CONVERSATION)

        data class Footer(
            override val id: String,
            val address: String,
            val environment: String,
        ) : MainListItem(id, ITEM_TYPE_FOOTER)
    }
}
