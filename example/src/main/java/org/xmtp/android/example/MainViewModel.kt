package org.xmtp.android.example

import androidx.annotation.UiThread
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch
import org.xmtp.android.library.Conversation

class MainViewModel : ViewModel() {

    private val _uiState = MutableStateFlow<UiState>(UiState.Loading(null))
    val uiState: StateFlow<UiState> = _uiState

    @UiThread
    fun fetchConversations() {
        when (val uiState = uiState.value) {
            is UiState.Success -> _uiState.value = UiState.Loading(uiState.listItems)
            else -> _uiState.value = UiState.Loading(null)
        }
        viewModelScope.launch(Dispatchers.IO) {
            val listItems = mutableListOf<MainListItem>()
            try {
                listItems.addAll(
                    ClientManager.client.conversations.list().map { conversation ->
                        MainListItem.ConversationItem(
                            id = conversation.topic,
                            conversation
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
        data class ConversationItem(override val id: String, val conversation: Conversation) :
            MainListItem(id, ITEM_TYPE_CONVERSATION)

        data class Footer(override val id: String, val address: String, val environment: String) :
            MainListItem(id, ITEM_TYPE_FOOTER)
    }
}
