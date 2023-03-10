package org.xmtp.android.example.conversation

import androidx.annotation.UiThread
import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingCommand
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.catch
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.mapLatest
import kotlinx.coroutines.launch
import org.xmtp.android.example.ClientManager
import org.xmtp.android.library.Conversation
import org.xmtp.android.library.DecodedMessage

class ConversationDetailViewModel(private val savedStateHandle: SavedStateHandle) : ViewModel() {

    private val conversationTopicFlow = savedStateHandle.getStateFlow<String?>(
        ConversationDetailActivity.EXTRA_CONVERSATION_TOPIC,
        null
    )

    private val conversationTopic = conversationTopicFlow.value

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
                    conversation = ClientManager.client.fetchConversation(conversationTopic)
                }
                conversation?.let {
                    listItems.addAll(
                        it.messages().map { message ->
                            MessageListItem.Message(message.id, message)
                        }
                    )
                }
                _uiState.value = UiState.Success(listItems)
            } catch (e: Exception) {
                _uiState.value = UiState.Error(e.localizedMessage.orEmpty())
            }
        }
    }

    @OptIn(ExperimentalCoroutinesApi::class)
    val streamMessages: StateFlow<List<MessageListItem>> =
        stateFlow(viewModelScope, emptyList()) { subscriptionCount ->
            if (conversation == null) {
                conversation = ClientManager.client.fetchConversation(conversationTopic)
            }
            conversation?.let {
                it.streamMessages()
                    .flowWhileShared(
                        subscriptionCount,
                        SharingStarted.WhileSubscribed(1000L)
                    )
                    .distinctUntilChanged()
                    .mapLatest { message ->
                        listOf(MessageListItem.Message(message.id, message))
                    }
                    .catch { emit(emptyList()) }
            }
            emptyFlow()
        }

@UiThread
fun sendMessage(body: String): StateFlow<SendMessageState> {
    val flow = MutableStateFlow<SendMessageState>(SendMessageState.Loading)
    viewModelScope.launch(Dispatchers.IO) {
        try {
            conversation?.send(body)
            flow.value = SendMessageState.Success
        } catch (e: Exception) {
            flow.value = SendMessageState.Error(e.localizedMessage.orEmpty())
        }
    }
    return flow
}

sealed class UiState {
    data class Loading(val listItems: List<MessageListItem>?) : UiState()
    data class Success(val listItems: List<MessageListItem>) : UiState()
    data class Error(val message: String) : UiState()
}

sealed class SendMessageState {
    object Loading : SendMessageState()
    object Success : SendMessageState()
    data class Error(val message: String) : SendMessageState()
}

sealed class MessageListItem(open val id: String, val itemType: Int) {
    companion object {
        const val ITEM_TYPE_MESSAGE = 1
    }

    data class Message(override val id: String, val message: DecodedMessage) :
        MessageListItem(id, ITEM_TYPE_MESSAGE)
}

@OptIn(ExperimentalCoroutinesApi::class)
private fun <T> Flow<T>.flowWhileShared(
    subscriptionCount: StateFlow<Int>,
    started: SharingStarted,
): Flow<T> {
    return started.command(subscriptionCount)
        .distinctUntilChanged()
        .flatMapLatest {
            when (it) {
                SharingCommand.START -> this
                SharingCommand.STOP, SharingCommand.STOP_AND_RESET_REPLAY_CACHE -> emptyFlow()
            }
        }
}

private fun <T> stateFlow(
    scope: CoroutineScope,
    initialValue: T,
    producer: (subscriptionCount: StateFlow<Int>) -> Flow<T>,
): StateFlow<T> {
    val state = MutableStateFlow(initialValue)
    scope.launch {
        producer(state.subscriptionCount).collect(state)
    }
    return state.asStateFlow()
}
}
