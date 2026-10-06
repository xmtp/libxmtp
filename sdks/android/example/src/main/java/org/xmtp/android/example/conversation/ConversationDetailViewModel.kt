package org.xmtp.android.example.conversation

import androidx.annotation.UiThread
import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.catch
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.flow.mapLatest
import kotlinx.coroutines.launch
import org.xmtp.android.example.ClientManager
import org.xmtp.android.example.extension.flowWhileShared
import org.xmtp.android.example.extension.messageStream
import org.xmtp.android.example.extension.stateFlow
import uniffi.xmtp_sdk.*

class ConversationDetailViewModel(
    private val savedStateHandle: SavedStateHandle,
) : ViewModel() {
    private val conversationTopicFlow =
        savedStateHandle.getStateFlow<String?>(
            ConversationDetailActivity.EXTRA_CONVERSATION_TOPIC,
            null,
        )

    private val conversationTopic get() = conversationTopicFlow.value

    fun setConversationTopic(conversationTopic: String?) {
        savedStateHandle[ConversationDetailActivity.EXTRA_CONVERSATION_TOPIC] = conversationTopic
    }

    private val _uiState = MutableStateFlow<UiState>(UiState.Loading(null))
    val uiState: StateFlow<UiState> = _uiState

    private var conversation: Conversation? = null

    @UiThread
    fun fetchMessages() {
        when (val uiState = uiState.value) {
            is UiState.Success -> _uiState.value = UiState.Loading(uiState.listItems)
            else -> _uiState.value = UiState.Loading(null)
        }
        viewModelScope.launch(Dispatchers.IO) {
            val listItems = mutableListOf<MessageListItem>()
            try {
                if (conversation == null) {
                    conversation = ClientManager.client.conversations.getById(conversationTopic!!)
                }
                conversation?.let {
                    if (conversation is Conversation.Group) {
                        (conversation as Conversation.Group).group.sync()
                    }
                    listItems.addAll(
                        it.messages().map { message ->
                            MessageListItem.Message(message.id, message)
                        },
                    )
                }
                _uiState.value = UiState.Success(listItems)
            } catch (error: Throwable) {
                if (error is CancellationException) throw error
                _uiState.value = UiState.Error(error.localizedMessage.orEmpty())
            }
        }
    }

    @OptIn(ExperimentalCoroutinesApi::class)
    val streamMessages: StateFlow<MessageListItem?> =
        stateFlow(viewModelScope, null) { subscriptionCount ->
            flow {
                val selected =
                    conversation ?: ClientManager.client
                        .conversations
                        .getById(checkNotNull(conversationTopic))
                        ?.also { conversation = it }
                if (selected != null) emitAll(ClientManager.client.messageStream(selected))
            }.flowWhileShared(
                subscriptionCount,
                SharingStarted.WhileSubscribed(1000L),
            ).flowOn(Dispatchers.IO)
                .distinctUntilChanged()
                .mapLatest { message -> MessageListItem.Message(message.id, message) }
                .catch { emptyFlow<MessageListItem>() }
        }

    @UiThread
    fun sendMessage(body: String): StateFlow<SendMessageState> {
        val flow = MutableStateFlow<SendMessageState>(SendMessageState.Loading)
        viewModelScope.launch(Dispatchers.IO) {
            try {
                checkNotNull(conversation) { "Conversation is not ready" }.sendText(body)
                flow.value = SendMessageState.Success
            } catch (error: Throwable) {
                if (error is CancellationException) throw error
                flow.value = SendMessageState.Error(error.localizedMessage.orEmpty())
            }
        }
        return flow
    }

    sealed class UiState {
        data class Loading(
            val listItems: List<MessageListItem>?,
        ) : UiState()

        data class Success(
            val listItems: List<MessageListItem>,
        ) : UiState()

        data class Error(
            val message: String,
        ) : UiState()
    }

    sealed class SendMessageState {
        object Loading : SendMessageState()

        object Success : SendMessageState()

        data class Error(
            val message: String,
        ) : SendMessageState()
    }

    sealed class MessageListItem(
        open val id: String,
        val itemType: Int,
    ) {
        companion object {
            const val ITEM_TYPE_MESSAGE = 1
        }

        data class Message(
            override val id: String,
            val message: uniffi.xmtp_sdk.Message,
        ) : MessageListItem(id, ITEM_TYPE_MESSAGE)
    }
}
