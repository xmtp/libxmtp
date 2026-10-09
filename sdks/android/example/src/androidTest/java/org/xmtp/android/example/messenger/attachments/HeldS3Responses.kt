package org.xmtp.android.example.messenger.attachments

import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONObject
import java.net.HttpURLConnection
import java.net.URL

internal class HeldS3Responses {
    private val api = checkNotNull(InstrumentationRegistry.getArguments().getString("s3GateApi"))

    private suspend fun request(action: String): JSONObject =
        withContext(Dispatchers.IO) {
            val connection = URL(api + action).openConnection() as HttpURLConnection
            connection.connectTimeout = 5_000
            connection.readTimeout = 5_000
            try {
                check(connection.responseCode == 200)
                JSONObject(connection.inputStream.bufferedReader().use { it.readText() })
            } finally {
                connection.disconnect()
            }
        }

    suspend fun hold() {
        request("hold")
    }

    suspend fun release() {
        request("release")
    }

    suspend fun heldGets() = request("state").getInt("held_gets")
}
