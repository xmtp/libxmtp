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
import org.xmtp.android.library.Group
import org.xmtp.android.library.XMTPEnvironment
import org.xmtp.android.library.messages.PrivateKeyBuilder
import org.xmtp.android.library.messages.toV2
import uniffi.xmtpv3.FfiConversationCallback
import uniffi.xmtpv3.FfiGroup
import uniffi.xmtpv3.FfiInboxOwner
import uniffi.xmtpv3.FfiListConversationsOptions
import uniffi.xmtpv3.FfiLogger
import uniffi.xmtpv3.FfiMessage
import uniffi.xmtpv3.FfiMessageCallback
import uniffi.xmtpv3.GroupPermissions
import uniffi.xmtpv3.LegacyIdentitySource
import kotlin.time.Duration.Companion.nanoseconds
import kotlin.time.DurationUnit

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

class MessageCallback : FfiMessageCallback {
    override fun onMessage(message: FfiMessage) {
        Log.i("App",
            "INFO - Message callback with ID: " +
                    toHexString(message.id) +
                    ", from: " +
                   message.addrFrom
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
        val alixDbPath: String = dbDir.absolutePath + "/android_example_alix.db3"
        val boDbPath: String = dbDir.absolutePath + "/android_example_bo.db3"
        val alixDbEncryptionKey = SecureRandom().generateSeed(32)
        val boDbEncryptionKey = SecureRandom().generateSeed(32)
        Log.i(
                "App",
                "INFO -\nDB path: " +
                        alixDbPath +
                        "\nDB encryption key: " +
                        alixDbEncryptionKey
        )

        runBlocking {
            try {

                // Create Alix Client
                val alixKey = PrivateKeyBuilder()
                val alix =
                        uniffi.xmtpv3.createClient(
                                AndroidFfiLogger(),
                                EMULATOR_LOCALHOST_ADDRESS,
                                false,
                                alixDbPath,
                                alixDbEncryptionKey,
                                alixKey.address,
                                LegacyIdentitySource.KEY_GENERATOR,
                                getV2SerializedSignedPrivateKey(alixKey),
                        )
                var alixWalletSignature: ByteArray? = null
                val alixTextToSign = alix.textToSign()
                if (alixTextToSign != null) {
                    alixWalletSignature = alixKey.sign(alixTextToSign).toByteArray()
                }
                alix.registerIdentity(alixWalletSignature);
                textView.text = "Libxmtp version\n" + uniffi.xmtpv3.getVersionInfo() + "\n\nClient constructed, wallet address: " + alix.accountAddress()

                // Create Bo Client
                val boKey = PrivateKeyBuilder()
                val bo =
                    uniffi.xmtpv3.createClient(
                        AndroidFfiLogger(),
                        EMULATOR_LOCALHOST_ADDRESS,
                        false,
                        boDbPath,
                        boDbEncryptionKey,
                        boKey.address,
                        LegacyIdentitySource.KEY_GENERATOR,
                        getV2SerializedSignedPrivateKey(boKey),
                    )
                var boWalletSignature: ByteArray? = null
                val boTextToSign = bo.textToSign()
                if (boTextToSign != null) {
                    boWalletSignature = boKey.sign(boTextToSign).toByteArray()
                }
                bo.registerIdentity(boWalletSignature);
                textView.text = "Libxmtp version\n" + uniffi.xmtpv3.getVersionInfo() + "\n\nClient constructed, wallet address: " + bo.accountAddress()

                var alixGroup = alix.conversations().createGroup(listOf(bo.accountAddress()), GroupPermissions.EVERYONE_IS_ADMIN)
                bo.conversations().sync()

                var boGroup = bo.conversations().list(
                    opts = FfiListConversationsOptions(
                        null,
                        null,
                        null
                    )
                ).first()
                alixGroup.stream(MessageCallback())
                val message = "hello"
                val preparedMessage =
                boGroup.send(message.encodeToByteArray())
                boGroup.send(message.encodeToByteArray())
                boGroup.send(message.encodeToByteArray())

                alixGroup.send(message.encodeToByteArray())
                alixGroup.send(message.encodeToByteArray())
                alixGroup.send(message.encodeToByteArray())

                boGroup.send(message.encodeToByteArray())
                boGroup.send(message.encodeToByteArray())
                boGroup.send(message.encodeToByteArray())

                Log.i("App", "Setting up conversation streaming")
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
                    env = XMTPEnvironment.LOCAL,
                    isSecure = false,
                ),
                appContext = this@MainActivity
            )
        val client = Client().create(account = key, options = options)
        return client.privateKeyBundleV1.toV2().identityKey.toByteArray();
    }
}
