package org.xmtp.android.example.connect

import android.app.Application
import android.net.Uri
import androidx.annotation.UiThread
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.walletconnect.wcmodal.client.Modal
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import org.xmtp.android.example.ClientManager
import org.xmtp.android.example.account.WalletConnectV2Account
import org.xmtp.android.library.Client
import org.xmtp.android.library.XMTPException
import org.xmtp.android.library.codecs.GroupUpdatedCodec
import org.xmtp.android.library.messages.PrivateKeyBuilder
import org.xmtp.android.library.messages.PrivateKeyBundleV1Builder

class ConnectWalletViewModel(application: Application) : AndroidViewModel(application) {

    private val chains: List<ChainSelectionUi> =
        Chains.values().map { it.toChainUiState() }

    private val _showWalletState = MutableStateFlow(ShowWalletForSigningState(showWallet = false))
    val showWalletState: StateFlow<ShowWalletForSigningState>
        get() = _showWalletState.asStateFlow()

    private val _uiState = MutableStateFlow<ConnectUiState>(ConnectUiState.Unknown)
    val uiState: StateFlow<ConnectUiState> = _uiState

    init {
        DappDelegate.wcEventModels
            .filterNotNull()
            .onEach { walletEvent ->
                when (walletEvent) {
                    is Modal.Model.ApprovedSession -> {
                        connectWallet(walletEvent)
                    }

                    else -> Unit
                }

            }.launchIn(viewModelScope)
    }

    fun getSessionParams() = Modal.Params.SessionParams(
        requiredNamespaces = getNamespaces(),
        optionalNamespaces = getOptionalNamespaces()
    )

    private fun getNamespaces(): Map<String, Modal.Model.Namespace.Proposal> {
        val namespaces: Map<String, Modal.Model.Namespace.Proposal> =
            chains
                .groupBy { it.chainNamespace }
                .map { (key: String, selectedChains: List<ChainSelectionUi>) ->
                    key to Modal.Model.Namespace.Proposal(
                        chains = selectedChains.map { it.chainId },
                        methods = selectedChains.flatMap { it.methods }.distinct(),
                        events = selectedChains.flatMap { it.events }.distinct()
                    )
                }.toMap()


        return namespaces.toMutableMap()
    }

    private fun getOptionalNamespaces() = chains
        .groupBy { it.chainId }
        .map { (key: String, selectedChains: List<ChainSelectionUi>) ->
            key to Modal.Model.Namespace.Proposal(
                methods = selectedChains.flatMap { it.methods }.distinct(),
                events = selectedChains.flatMap { it.events }.distinct()
            )
        }.toMap()

    @UiThread
    fun generateWallet() {
        viewModelScope.launch(Dispatchers.IO) {
            _uiState.value = ConnectUiState.Loading
            try {
                val wallet = PrivateKeyBuilder()
                val client = Client().create(wallet, ClientManager.clientOptions(getApplication()))
                Client.register(codec = GroupUpdatedCodec())
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
    fun connectWallet(approvedSession: Modal.Model.ApprovedSession) {
        viewModelScope.launch(Dispatchers.IO) {
            _uiState.value = ConnectUiState.Loading
            try {
                val wallet = WalletConnectV2Account(
                    approvedSession,
                    Chains.ETHEREUM_MAIN.chainNamespace
                ) { uri ->
                    _showWalletState.update {
                        it.copy(showWallet = true, uri = uri)
                    }
                }
                val client = Client().create(wallet, ClientManager.clientOptions(getApplication()))
                Client.register(codec = GroupUpdatedCodec())
                _uiState.value = ConnectUiState.Success(
                    wallet.address,
                    PrivateKeyBundleV1Builder.encodeData(client.privateKeyBundleV1)
                )
            } catch (e: Exception) {
                _uiState.value = ConnectUiState.Error(e.message.orEmpty())
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
        data class Success(val address: String, val encodedKeyData: String) : ConnectUiState()
        data class Error(val message: String) : ConnectUiState()
    }

    data class ShowWalletForSigningState(val showWallet: Boolean, val uri: Uri? = null)
}
