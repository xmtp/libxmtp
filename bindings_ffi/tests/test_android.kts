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

class MockLogger : FfiLogger {
    override fun log(level: UInt, levelLabel: String, message: String) { println(message) }
}

val privateKey: ByteArray = SecureRandom().generateSeed(32)
val credentials: Credentials = Credentials.create(ECKeyPair.create(privateKey))
val inboxOwner = Web3jInboxOwner(credentials)
var logger = MockLogger()

runBlocking {
    println("Running first test")
    val apiUrl: String = System.getenv("XMTP_API_URL") ?: "http://localhost:5556"
    try {
        println("Creating client")
        val client = uniffi.xmtpv3.createClient(logger, inboxOwner, apiUrl, false)
        println("Calling walletAddress()")
        assert(client.walletAddress() != null) {
            "Should be able to get wallet address"
        }
        println("Returned")
     } catch (e: Exception) {
        assert(false) {
            "Should be able to construct client: " + e.message
        }
     }
}

// runBlocking {
//     try {
//         val client = uniffi.xmtpv3.createClient(logger, inboxOwner, "http://malformed:5556", false);
//         assert(false) {
//             "Should throw error with malformed network address"
//         }
//      } catch (e: Exception) {
//      }
// }

// TODO:
// 1. Fix logger issue - maybe make it a singleton?
// 2. Reproed other hanging test issue on Android Studio, try to repro it here? Probably an issue with the other callback interface?
runBlocking {
    println("Running third test")
    val newprivateKey: ByteArray = SecureRandom().generateSeed(32)
    val newcredentials: Credentials = Credentials.create(ECKeyPair.create(privateKey))
    val newinboxOwner = Web3jInboxOwner(credentials)
    var newlogger = MockLogger()

    val apiUrl: String = System.getenv("XMTP_API_URL") ?: "http://localhost:5556"
    try {
        val client = uniffi.xmtpv3.createClient(newlogger, newinboxOwner, apiUrl, false)
        println("Calling walletAddress()")
        assert(client.walletAddress() != null) {
            "Should be able to get wallet address"
        }
        println("Returned")
     } catch (e: Exception) {
        println("Execption")
        assert(false) {
            "Should be able to construct client: " + e.message
        }
     }
}
