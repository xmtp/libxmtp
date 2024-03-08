package com.example.xmtpv3_example

import android.os.Bundle
import android.util.Log
import android.widget.TextView
import androidx.appcompat.app.AppCompatActivity
import com.example.xmtpv3_example.R.id.selftest_output
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
import org.xmtp.android.library.XMTPEnvironment
import org.xmtp.android.library.messages.PrivateKeyBuilder
import org.xmtp.android.library.messages.toV2
import uniffi.xmtpv3.FfiConversationCallback
import uniffi.xmtpv3.FfiGroup
import uniffi.xmtpv3.FfiInboxOwner
import uniffi.xmtpv3.FfiListConversationsOptions
import uniffi.xmtpv3.FfiLogger
import uniffi.xmtpv3.LegacyIdentitySource

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
                val privateKeyData = listOf(0x08, 0x36, 0x20, 0x0f, 0xfa, 0xfa, 0x17, 0xa3, 0xcb, 0x8b, 0x54, 0xf2, 0x2d, 0x6a, 0xfa, 0x60, 0xb1, 0x3d, 0xa4, 0x87, 0x26, 0x54, 0x32, 0x41, 0xad, 0xc5, 0xc2, 0x50, 0xdb, 0xb0, 0xe0, 0xcd)
                    .map { it.toByte() }
                    .toByteArray()
                // Use hardcoded privateKey
                val privateKey = PrivateKeyBuilder.buildFromPrivateKeyData(privateKeyData)
                val key = PrivateKeyBuilder(privateKey)
                val client =
                        uniffi.xmtpv3.createClient(
                                AndroidFfiLogger(),
                                DEV_NETWORK_ADDRESS,
                                true,
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
                val conversations = client.conversations().list(FfiListConversationsOptions(null, null, null))
                Log.i(
                    "App",
                    "INFO - conversation list is ${conversations.size} long but should be 2k+"
                )
                client.conversations().stream(ConversationCallback())
            } catch (e: Exception) {
                textView.text = "Failed to construct client: " + e.message
            }
        }
    }

    fun getV2SerializedSignedPrivateKey(key: PrivateKeyBuilder): ByteArray {
        val options =
            ClientOptions(
                api =
                ClientOptions.Api(
                    env = XMTPEnvironment.DEV,
                    isSecure = true,
                ),
                appContext = this@MainActivity
            )
        val client = Client().create(account = key, options = options)
        return client.privateKeyBundleV1.toV2().identityKey.toByteArray();
    }
}
