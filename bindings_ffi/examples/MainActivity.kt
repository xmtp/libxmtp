package com.example.xmtpv3_example

import android.os.Bundle
import android.util.Log
import android.widget.TextView
import androidx.appcompat.app.AppCompatActivity
import com.example.xmtpv3_example.R.id.selftest_output
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import org.bouncycastle.util.encoders.Hex.toHexString
import org.web3j.crypto.Credentials
import org.web3j.crypto.ECKeyPair
import org.web3j.crypto.Sign
import uniffi.xmtpv3.FfiConversationCallback
import uniffi.xmtpv3.FfiGroup
import uniffi.xmtpv3.FfiInboxOwner
import uniffi.xmtpv3.FfiLogger
import uniffi.xmtpv3.FfiMessage
import uniffi.xmtpv3.FfiMessageCallback
import uniffi.xmtpv3.FfiXmtpClient
import uniffi.xmtpv3.LegacyIdentitySource
import java.io.File
import java.nio.charset.StandardCharsets
import java.security.SecureRandom

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

class ConversationCallback: FfiConversationCallback {
    override fun onConversation(conversation: FfiGroup) {
        Log.i("App", "INFO - Conversation callback with ID: " + toHexString(conversation.id()) + ", members: " + conversation.listMembers())
    }
}

class GroupCallback: FfiConversationCallback {
    private val _groups = MutableSharedFlow<FfiGroup>()
    val groups = _groups.asSharedFlow()
    override fun onConversation(conversation: FfiGroup) {
        Log.i("App", "INFO - Conversation callback with ID: " + toHexString(conversation.id()) + ", members: " + conversation.listMembers())
        runBlocking { _groups.emit(conversation) }
    }
}

// An example Android app testing the end-to-end flow through Rust
// Run setup_android_example.sh to set it up
class MainActivity : AppCompatActivity() {
    private var client: FfiXmtpClient? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_main)

        val textView: TextView = findViewById<TextView>(selftest_output)
        val privateKey: ByteArray = SecureRandom().generateSeed(32)
        val credentials: Credentials = Credentials.create(ECKeyPair.create(privateKey))
        val inboxOwner = Web3jInboxOwner(credentials)
        val dbDir: File = File(this.filesDir.absolutePath, "xmtp_db")
        try {
            dbDir.deleteRecursively()
        } catch (e: Exception) {
        }
        dbDir.mkdir()
        val dbPath: String = dbDir.absolutePath + "/android_example.db3"
        val dbEncryptionKey = SecureRandom().generateSeed(32)
        Log.i(
            "App",
            "INFO -\naccountAddress: " + inboxOwner.getAddress() + "\nprivateKey: " + privateKey.asList() + "\nDB path: " + dbPath + "\nDB encryption key: " + dbEncryptionKey
        )

        runBlocking {
            try {
                val client = uniffi.xmtpv3.createClient(
                    AndroidFfiLogger(),
                    EMULATOR_LOCALHOST_ADDRESS,
                    false,
                    dbPath,
                    dbEncryptionKey,
                    inboxOwner.getAddress(),
                    LegacyIdentitySource.NONE,
                    null,
                )
                var walletSignature: ByteArray? = null;
                val textToSign = client.textToSign();
                if (textToSign != null) {
                    walletSignature = inboxOwner.sign(textToSign)
                }
                client.registerIdentity(walletSignature);
                textView.text = "Client constructed, wallet address: " + client.accountAddress()
                Log.i("App", "Setting up conversation streaming")
                streamGroups().collect {
                    Log.i("App", "Group1 - Conversation callback with ID: " + toHexString(it.id()) + ", members: " + it.listMembers())
                }
                Log.i("App", "stream groups done")
                streamGroups2().collect {
                    Log.i("App", "Group2 - Conversation callback with ID: " + toHexString(it.id()) + ", members: " + it.listMembers())
                }
                Log.i("App", "stream groups 2 done")
            } catch (e: Exception) {
                textView.text = "Failed to construct client: " + e.message
            }
        }

    }

    private fun streamGroups(): Flow<FfiGroup> = flow {
        client?.conversations()?.stream(ConversationCallback())
    }

    private fun streamGroups2(): Flow<FfiGroup> = flow {
        val callback = GroupCallback()
        val stream = client?.conversations()?.stream(callback)

        coroutineScope {
            launch {
                try {
                    callback.groups.collect { group ->
                        emit(group)
                    }
                } catch (error: Exception) {
                    Log.e("App", "stream error ${error.message}")
                } finally {
                    stream?.end()
                }
            }
        }
    }
}
