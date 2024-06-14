package org.xmtp.android.example

import android.content.Context
import androidx.annotation.UiThread
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.GlobalScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch
import org.xmtp.android.library.Client
import org.xmtp.android.library.ClientOptions
import org.xmtp.android.library.XMTPEnvironment
import org.xmtp.android.library.codecs.GroupUpdatedCodec
import org.xmtp.android.library.messages.PrivateKeyBundleV1Builder

object ClientManager {

    fun clientOptions(appContext: Context?): ClientOptions {
        return ClientOptions(
            api = ClientOptions.Api(
                XMTPEnvironment.DEV,
                appVersion = "XMTPAndroidExample/v1.0.0",
                isSecure = true
            ),
            enableV3 = true,
            appContext = appContext
        )
    }

    private val _clientState = MutableStateFlow<ClientState>(ClientState.Unknown)
    val clientState: StateFlow<ClientState> = _clientState

    private var _client: Client? = null

    val client: Client
        get() = if (clientState.value == ClientState.Ready) {
            _client!!
        } else {
            throw IllegalStateException("Client called before Ready state")
        }

    @UiThread
    fun createClient(encodedPrivateKeyData: String, appContext: Context) {
        if (clientState.value is ClientState.Ready) return
        GlobalScope.launch(Dispatchers.IO) {
            try {
                val v1Bundle =
                    PrivateKeyBundleV1Builder.fromEncodedData(data = encodedPrivateKeyData)
                _client = Client().buildFrom(v1Bundle, clientOptions(appContext))
                Client.register(codec = GroupUpdatedCodec())
                _clientState.value = ClientState.Ready
            } catch (e: Exception) {
                _clientState.value = ClientState.Error(e.localizedMessage.orEmpty())
            }
        }
    }

    @UiThread
    fun clearClient() {
        _clientState.value = ClientState.Unknown
        _client = null
    }

    sealed class ClientState {
        object Unknown : ClientState()
        object Ready : ClientState()
        data class Error(val message: String) : ClientState()
    }
}
