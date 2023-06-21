import kotlinx.coroutines.*
import kotlin.system.*
import uniffi.xmtpv3.*

runBlocking {
    val apiUrl: String = System.getenv("XMTP_API_URL") ?: "http://localhost:5556"
    try {
        val client = uniffi.xmtpv3.createClient(apiUrl, false)
        assert(client.walletAddress() != null) {
            "Should be able to get wallet address"
        }
     } catch (e: Exception) {
        assert(false) {
            "Should be able to construct client"
        }
     }
}

// This test does not pass yet - pending https://github.com/mozilla/uniffi-rs/issues/1611
// runBlocking {
//     try {
//         val client = uniffi.xmtpv3.createClient("http://malformed:5556", false);
//         assert(false) {
//             "Should throw error with malformed network address"
//         }
//      } catch (e: Exception) {
//      }
// }
