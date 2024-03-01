package com.example.xmtpv3_example

import android.os.Bundle
import android.util.Log
import android.widget.TextView
import androidx.appcompat.app.AppCompatActivity
import com.example.xmtpv3_example.R.id.selftest_output
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.callbackFlow
import java.io.File
import java.nio.charset.StandardCharsets
import java.security.SecureRandom
import kotlinx.coroutines.runBlocking
import org.bouncycastle.util.encoders.Hex.toHexString
import org.web3j.crypto.Credentials
import org.web3j.crypto.ECKeyPair
import org.web3j.crypto.Sign
import org.xmtp.android.library.Client
import org.xmtp.android.library.ClientOptions
import org.xmtp.android.library.DecodedMessage
import org.xmtp.android.library.XMTPEnvironment
import org.xmtp.android.library.XMTPException
import org.xmtp.android.library.libxmtp.Message
import org.xmtp.android.library.messages.PrivateKeyBuilder
import org.xmtp.android.library.messages.toV2
import uniffi.xmtpv3.FfiConversationCallback
import uniffi.xmtpv3.FfiGroup
import uniffi.xmtpv3.FfiInboxOwner
import uniffi.xmtpv3.FfiLogger
import uniffi.xmtpv3.FfiMessage
import uniffi.xmtpv3.FfiMessageCallback
import uniffi.xmtpv3.LegacyIdentitySource
import kotlinx.coroutines.flow.callbackFlow
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.launch
import uniffi.xmtpv3.FfiXmtpClient


const val EMULATOR_LOCALHOST_ADDRESS = "http://10.0.2.2:5556"
const val DEV_NETWORK_ADDRESS = "https://dev.xmtp.network:5556"

class Web3jInboxOwner(private val credentials: Credentials) : FfiInboxOwner {
    override fun getAddress(): String {
        return credentials.address
    }

    override fun sign(text: String): ByteArray {
        val messageBytes: ByteArray = text.toByteArray(StandardCharsets.UTF_8)
        val signature = Sign.signPrefixedMessage(messageBytes, credentials.ecKeyPair)
        return signature.r + signature.s + signature.v
    }
}

class AndroidFfiLogger : FfiLogger {
    override fun log(level: UInt, levelLabel: String, message: String) {
        Log.i("Rust", levelLabel + " - " + message)
    }
}

class ConversationCallback : FfiConversationCallback {
    override fun onConversation(conversation: FfiGroup) {
        Log.i(
                "App",
                "INFO - Conversation callback with ID: " +
                        toHexString(conversation.id()) +
                        ", members: " +
                        conversation.listMembers()
        )
        runBlocking {
            Log.i(
                "App",
                "INFO - Sending message from conversation callback"
            )
            conversation.send("Hi new group".toByteArray())
            Log.i(
                "App",
                "INFO - Send completed"
            )
        }
    }
}

// An example Android app testing the end-to-end flow through Rust
// Run setup_android_example.sh to set it up
class MainActivity : AppCompatActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_main)

        val textView: TextView = findViewById<TextView>(selftest_output)
        val dbDir: File = File(this.filesDir.absolutePath, "xmtp_db")
        try {
            dbDir.deleteRecursively()
        } catch (e: Exception) {}
        dbDir.mkdir()
        val dbPath: String = dbDir.absolutePath + "/android_example.db3"
        val dbEncryptionKey = SecureRandom().generateSeed(32)
        Log.i(
                "App",
                "INFO -\nDB path: " +
                        dbPath +
                        "\nDB encryption key: " +
                        dbEncryptionKey
        )

        runBlocking {
            try {
                val key = PrivateKeyBuilder()
                val client =
                        uniffi.xmtpv3.createClient(
                                AndroidFfiLogger(),
                                EMULATOR_LOCALHOST_ADDRESS,
                                false,
                                dbPath,
                                dbEncryptionKey,
                                key.address,
                                LegacyIdentitySource.KEY_GENERATOR,
                                getV2SerializedSignedPrivateKey(key),
                        )
                var walletSignature: ByteArray? = null
                val textToSign = client.textToSign()
                if (textToSign != null) {
                    walletSignature = key.sign(textToSign).toByteArray()
                }
                client.registerIdentity(walletSignature);
                textView.text = "Libxmtp version\n" + uniffi.xmtpv3.getVersionInfo() + "\n\nClient constructed, wallet address: " + client.accountAddress()
                Log.i("App", "Setting up conversation streaming")
                client.conversations().stream(ConversationCallback())
                streamAllMessages(client).collect()
            } catch (e: Exception) {
                textView.text = "Failed to construct client: " + e.message
            }
        }
    }

    fun streamAllMessages(client: FfiXmtpClient): Flow<DecodedMessage> = callbackFlow {
        val messageCallback = object : FfiMessageCallback {
            override fun onMessage(message: FfiMessage) {
                Log.i(
                    "App",
                    "INFO - Message callback with ID: " +
                            toHexString(message.id) +
                            ", members: " +
                            message.addrFrom
                )
            }
        }
        val stream = client.conversations().streamAllMessages(messageCallback)
        awaitClose { stream.end() }
    }

    fun getV2SerializedSignedPrivateKey(key: PrivateKeyBuilder): ByteArray {
        val options =
            ClientOptions(
                api =
                ClientOptions.Api(
                    env = XMTPEnvironment.LOCAL,
                    isSecure = false,
                ),
                appContext = this@MainActivity
            )
        val client = Client().create(account = key, options = options)
        return client.privateKeyBundleV1.toV2().identityKey.toByteArray();
    }
}
