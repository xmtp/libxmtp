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

    private val _uiState = MutableStateFlow<ClientUiState>(ClientUiState.Unknown)
    val uiState: StateFlow<ClientUiState> = _uiState
    private var client: Client? = null

    @UiThread
    fun createClient(encodedPrivateKeyData: String) {
        viewModelScope.launch(Dispatchers.IO) {
            try {
                val wallet = PrivateKeyBuilder(encodedPrivateKeyData = encodedPrivateKeyData)
                client = Client().create(wallet)
                _uiState.value = ClientUiState.Ready(client?.address.orEmpty())
            } catch (e: Exception) {
                _uiState.value = ClientUiState.Error(e.message.orEmpty())
            }
        }
    }

    @UiThread
    fun clearClient() {
        _uiState.value = ClientUiState.Unknown
        client = null
    }

    sealed class ClientUiState {
        object Unknown : ClientUiState()
        data class Ready(val address: String): ClientUiState()
        data class Error(val message: String): ClientUiState()
    }
}
