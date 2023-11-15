package org.xmtp.android.example

import android.app.Application
import com.walletconnect.android.Core
import com.walletconnect.android.CoreClient
import com.walletconnect.android.relay.ConnectionType
import com.walletconnect.wcmodal.client.Modal
import com.walletconnect.wcmodal.client.WalletConnectModal
import timber.log.Timber

const val BASE_LOG_TAG = "WC2"
class ExampleApp: Application() {

    override fun onCreate() {
        super.onCreate()
        val connectionType = ConnectionType.AUTOMATIC
        val relayUrl = "relay.walletconnect.com"
        val serverUrl = "wss://$relayUrl?projectId=${BuildConfig.PROJECT_ID}"
        val appMetaData = Core.Model.AppMetaData(
            name = "XMTP Example",
            description = "Example app using the xmtp-android SDK",
            url = "https://xmtp.org",
            icons = listOf("https://avatars.githubusercontent.com/u/82580170?s=48&v=4"),
            redirect = "xmtp-example-wc://request"
        )

        CoreClient.initialize(
            metaData = appMetaData,
            relayServerUrl = serverUrl,
            connectionType = connectionType,
            application = this,
            onError = {
                Timber.tag("$BASE_LOG_TAG CoreClient").d(it.toString())
            }
        )

        WalletConnectModal.initialize(
            init = Modal.Params.Init(core = CoreClient),
            onSuccess = {
                Timber.tag("$BASE_LOG_TAG initialize").d("initialize successfully")
            },
            onError = { error ->
                Timber.tag("$BASE_LOG_TAG initialize").d(error.toString())
            }
        )
    }
}