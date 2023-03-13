package org.xmtp.android.example.connect

import android.app.Application
import androidx.annotation.UiThread
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import dev.pinkroom.walletconnectkit.WalletConnectKit
import dev.pinkroom.walletconnectkit.WalletConnectKitConfig
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch
import org.xmtp.android.example.ClientManager
import org.xmtp.android.example.account.WalletConnectAccount
import org.xmtp.android.library.Client
import org.xmtp.android.library.XMTPException
import org.xmtp.android.library.messages.PrivateKeyBuilder
import org.xmtp.android.library.messages.PrivateKeyBundleV1Builder

class ConnectWalletViewModel(application: Application) : AndroidViewModel(application) {

    private val _uiState = MutableStateFlow<ConnectUiState>(ConnectUiState.Unknown)
    val uiState: StateFlow<ConnectUiState> = _uiState

    private val walletConnectKitConfig = WalletConnectKitConfig(
        context = application,
        bridgeUrl = "https://safe-walletconnect.safe.global/",
        appUrl = "https://xmtp.org",
        appName = "XMTP Example",
        appDescription = "Example app using the xmtp-android SDK"
    )
    val walletConnectKit = WalletConnectKit.Builder(walletConnectKitConfig).build()

    @UiThread
    fun generateWallet() {
        viewModelScope.launch(Dispatchers.IO) {
            _uiState.value = ConnectUiState.Loading
            try {
                val wallet = PrivateKeyBuilder()
                val client = Client().create(wallet, ClientManager.CLIENT_OPTIONS)
                _uiState.value = ConnectUiState.Success(
                    wallet.address,
                    PrivateKeyBundleV1Builder.encodeData(client.privateKeyBundleV1)
                )
            } catch (e: XMTPException) {
                _uiState.value = ConnectUiState.Error(e.message.orEmpty())
            }
        }
    }

    @UiThread
    fun connectWallet() {
        viewModelScope.launch(Dispatchers.IO) {
            _uiState.value = ConnectUiState.Loading
            try {
                val wallet = WalletConnectAccount(walletConnectKit)
                val client = Client().create(wallet, ClientManager.CLIENT_OPTIONS)
                _uiState.value = ConnectUiState.Success(
                    wallet.address,
                    PrivateKeyBundleV1Builder.encodeData(client.privateKeyBundleV1)
                )
            } catch (e: Exception) {
                _uiState.value = ConnectUiState.Error(e.message.orEmpty())
            }
        }
    }

    sealed class ConnectUiState {
        object Unknown : ConnectUiState()
        object Loading : ConnectUiState()
        data class Success(val address: String, val encodedKeyData: String) : ConnectUiState()
        data class Error(val message: String) : ConnectUiState()
    }
}
