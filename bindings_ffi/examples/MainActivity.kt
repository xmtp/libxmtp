package com.example.xmtpv3_example

import android.os.Bundle
import android.widget.TextView
import androidx.appcompat.app.AppCompatActivity
import com.example.xmtpv3_example.R.id.selftest_output
import kotlinx.coroutines.runBlocking
import org.web3j.crypto.Credentials
import org.web3j.crypto.ECKeyPair
import org.web3j.crypto.Sign
import uniffi.xmtpv3.FfiInboxOwner
import java.nio.charset.StandardCharsets
import java.security.SecureRandom

const val EMULATOR_LOCALHOST_ADDRESS = "http://10.0.2.2:5556"

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

        runBlocking {
            try {
                val client = uniffi.xmtpv3.createClient(inboxOwner, EMULATOR_LOCALHOST_ADDRESS, false);
                textView.text = "Client constructed, wallet address: " + client.walletAddress();
            } catch (e: Exception) {
                textView.text = "Failed to construct client: " + e.message;
            }
        }
    }
}