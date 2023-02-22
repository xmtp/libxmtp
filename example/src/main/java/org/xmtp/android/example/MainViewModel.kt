package org.xmtp.android.example

import androidx.annotation.UiThread
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch
import org.xmtp.android.library.Client
import org.xmtp.android.library.messages.PrivateKeyBuilder

class MainViewModel : ViewModel() {

    private val _clientState = MutableStateFlow<ClientState>(ClientState.Unknown)
    val clientState: StateFlow<ClientState> = _clientState
    var client: Client? = null

    private val _uiState = MutableStateFlow<UiState>(UiState.Loading(null))
    val uiState: StateFlow<UiState> = _uiState

    @UiThread
    fun createClient(encodedPrivateKeyData: String) {
        if (clientState.value is ClientState.Ready) return
        viewModelScope.launch(Dispatchers.IO) {
            try {
                val wallet = PrivateKeyBuilder(encodedPrivateKeyData = encodedPrivateKeyData)
                client = Client().create(wallet)
                _clientState.value = ClientState.Ready
            } catch (e: Exception) {
                _clientState.value = ClientState.Error(e.localizedMessage.orEmpty())
            }
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
                client?.let {
                    listItems.addAll(
                        it.conversations.list().map { conversation ->
                            MainListItem.Conversation(
                                id = conversation.topic,
                                conversation.peerAddress
                            )
                        }
                    )
                    listItems.add(
                        MainListItem.Footer(
                            id = "footer",
                            it.address.orEmpty(),
                            it.apiClient.environment.name
                        )
                    )
                    _uiState.value = UiState.Success(listItems)
                }
            } catch (e: Exception) {
                _uiState.value = UiState.Error(e.localizedMessage.orEmpty())
            }
        }
    }

    @UiThread
    fun clearClient() {
        _clientState.value = ClientState.Unknown
        client = null
    }

    sealed class ClientState {
        object Unknown : ClientState()
        object Ready : ClientState()
        data class Error(val message: String) : ClientState()
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
        data class Conversation(override val id: String, val peerAddress: String) :
            MainListItem(id, ITEM_TYPE_CONVERSATION)

        data class Footer(override val id: String, val address: String, val environment: String) :
            MainListItem(id, ITEM_TYPE_FOOTER)
    }
}
