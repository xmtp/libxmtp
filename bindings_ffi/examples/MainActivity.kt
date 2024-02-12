package com.example.xmtpv3_example

import android.os.Bundle
import android.util.Log
import android.widget.TextView
import androidx.appcompat.app.AppCompatActivity
import com.example.xmtpv3_example.R.id.selftest_output
import kotlinx.coroutines.runBlocking
import org.bouncycastle.util.encoders.Hex.toHexString
import org.web3j.crypto.Credentials
import org.web3j.crypto.ECKeyPair
import org.web3j.crypto.Sign
import uniffi.xmtpv3.FfiConversationCallback
import uniffi.xmtpv3.FfiGroup
import uniffi.xmtpv3.FfiInboxOwner
import uniffi.xmtpv3.FfiLogger
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

// An example Android app testing the end-to-end flow through Rust
// Run setup_android_example.sh to set it up
class MainActivity : AppCompatActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_main)

        val textView: TextView = findViewById<TextView>(selftest_output)
        val privateKey: ByteArray = SecureRandom().generateSeed(32)
        val credentials: Credentials = Credentials.create(ECKeyPair.create(privateKey))
        val inboxOwner = Web3jInboxOwner(credentials)
        val dbDir: File = File(this.filesDir.absolutePath, "xmtp_db")
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
                textView.text = "Libxmtp version\n" + uniffi.xmtpv3.getVersionInfo() + "\n\nClient constructed, wallet address: " + client.accountAddress()
                Log.i("App", "Setting up conversation streaming")
                client.conversations().stream(ConversationCallback());
            } catch (e: Exception) {
                textView.text = "Failed to construct client: " + e.message
            }
        }

        dbDir.deleteRecursively()
    }
}
