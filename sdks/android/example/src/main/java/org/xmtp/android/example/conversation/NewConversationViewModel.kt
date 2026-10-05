package org.xmtp.android.example.conversation

import androidx.annotation.UiThread
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch
import org.xmtp.android.example.ClientManager
import uniffi.xmtp_sdk.*
import uniffi.xmtp_sdk.PublicIdentity
import uniffi.xmtp_sdk.PublicIdentityKind

class NewConversationViewModel : ViewModel() {
    private val _uiState = MutableStateFlow<UiState>(UiState.Unknown)
    val uiState: StateFlow<UiState> = _uiState

    @UiThread
    fun createConversation(address: String) {
        _uiState.value = UiState.Loading
        viewModelScope.launch(Dispatchers.IO) {
            try {
                val conversation =
                    ClientManager.client.conversations().createDm(
                        PublicIdentity(
                            address,
                            PublicIdentityKind.ETHEREUM,
                        ),
                    )
                _uiState.value = UiState.Success(Conversation.Dm(conversation))
            } catch (e: Exception) {
                _uiState.value = UiState.Error(e.localizedMessage.orEmpty())
            }
        }
    }

    @UiThread
    fun createGroup(addresses: List<String>) {
        _uiState.value = UiState.Loading
        viewModelScope.launch(Dispatchers.IO) {
            try {
                val group =
                    ClientManager.client.conversations().createGroup(
                        addresses.map {
                            PublicIdentity(
                                it,
                                PublicIdentityKind.ETHEREUM,
                            )
                        },
                    )
                _uiState.value = UiState.Success(Conversation.Group(group))
            } catch (e: Exception) {
                _uiState.value = UiState.Error(e.localizedMessage.orEmpty())
            }
        }
    }

    sealed class UiState {
        object Unknown : UiState()

        object Loading : UiState()

        data class Success(
            val conversation: Conversation,
        ) : UiState()

        data class Error(
            val message: String,
        ) : UiState()
    }
}
