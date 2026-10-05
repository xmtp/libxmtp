package org.xmtp.android.example.connect

import android.app.Application
import android.net.Uri
import androidx.annotation.UiThread
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.walletconnect.wcmodal.client.Modal
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.xmtp.android.example.ClientManager
import uniffi.xmtp_sdk.*

class ConnectWalletViewModel(
    application: Application,
) : AndroidViewModel(application) {
    private val _showWalletState = MutableStateFlow(ShowWalletForSigningState(showWallet = false))
    val showWalletState: StateFlow<ShowWalletForSigningState>
        get() = _showWalletState.asStateFlow()

    private val _uiState = MutableStateFlow<ConnectUiState>(ConnectUiState.Unknown)
    val uiState: StateFlow<ConnectUiState> = _uiState

    @UiThread
    fun generateWallet() {
        viewModelScope.launch(Dispatchers.IO) {
            _uiState.value = ConnectUiState.Loading
            try {
                val wallet = generateLocalSigner()
                val address = wallet.identity().identifier
                val client =
                    SDKClient.create(
                        getApplication(),
                        wallet,
                        ClientManager.clientOptions(getApplication(), address),
                    )
                try {
                    client.inboxId()
                } finally {
                    withContext(NonCancellable) { client.end() }
                }
                _uiState.value = ConnectUiState.Success(address)
            } catch (error: Throwable) {
                if (error is CancellationException) throw error
                _uiState.value = ConnectUiState.Error(error.message.orEmpty())
            }
        }
    }

    fun clearShowWalletState() {
        _showWalletState.update {
            it.copy(showWallet = false)
        }
    }

    sealed class ConnectUiState {
        object Unknown : ConnectUiState()

        object Loading : ConnectUiState()

        data class Success(
            val address: String,
        ) : ConnectUiState()

        data class Error(
            val message: String,
        ) : ConnectUiState()
    }

    data class ShowWalletForSigningState(
        val showWallet: Boolean,
        val uri: Uri? = null,
    )
}
