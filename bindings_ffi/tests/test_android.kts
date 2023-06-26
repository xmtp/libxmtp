import java.nio.charset.StandardCharsets
import java.security.SecureRandom
import kotlinx.coroutines.*
import kotlin.system.*
import org.web3j.crypto.*
import uniffi.xmtpv3.*

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

val privateKey: ByteArray = SecureRandom().generateSeed(32)
val credentials: Credentials = Credentials.create(ECKeyPair.create(privateKey))
val inboxOwner = Web3jInboxOwner(credentials)

runBlocking {
    val apiUrl: String = System.getenv("XMTP_API_URL") ?: "http://localhost:5556"
    try {
        val client = uniffi.xmtpv3.createClient(inboxOwner, apiUrl, false)
        assert(client.walletAddress() != null) {
            "Should be able to get wallet address"
        }
     } catch (e: Exception) {
        assert(false) {
            "Should be able to construct client: " + e.message
        }
     }
}

// This test does not pass yet - pending https://github.com/mozilla/uniffi-rs/issues/1611
runBlocking {
    try {
        val client = uniffi.xmtpv3.createClient(inboxOwner, "http://malformed:5556", false);
        assert(false) {
            "Should throw error with malformed network address"
        }
     } catch (e: Exception) {
     }
}
