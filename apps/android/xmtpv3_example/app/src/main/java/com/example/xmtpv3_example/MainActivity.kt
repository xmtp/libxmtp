package com.example.xmtpv3_example

import android.os.Bundle
import android.widget.TextView
import androidx.appcompat.app.AppCompatActivity
import androidx.lifecycle.lifecycleScope
import kotlinx.coroutines.*
import uniffi.xmtp_sdk.*

class MainActivity : AppCompatActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val status = TextView(this).also { it.text = "Open local XMTP client" }
        setContentView(status)
        lifecycleScope.launch {
            try {
                val result =
                    withContext(Dispatchers.IO) {
                        val signer = generateLocalSigner()
                        val client =
                            SDKClient.create(
                                applicationContext,
                                signer,
                                ClientOptions(
                                    backend =
                                        BackendSource.Options(
                                            BackendOptions(url = BuildConfig.XMTP_BACKEND_URL),
                                        ),
                                    storage = StorageOptions(location = StorageLocation.InMemory),
                                ),
                            )
                        try {
                            val group = client.conversations().createGroup(emptyList<InboxId>())
                            group.sendText("Android SDK 8.0.0")
                            "Inbox: ${client.inboxId()}\nGroup: ${group.id()}\nMessages: ${group.messages().size}"
                        } finally {
                            withContext(NonCancellable) { client.end() }
                        }
                    }
                status.text = result
            } catch (error: Throwable) {
                if (error is CancellationException) throw error
                status.text = error.message ?: "Client failed"
            }
        }
    }
}
