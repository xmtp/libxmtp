package org.xmtp.android.library.frames

import java.net.HttpURLConnection
import java.net.URL
import com.google.gson.Gson
import org.xmtp.android.library.XMTPException
import java.io.OutputStreamWriter

class ProxyClient(private val baseUrl: String) {

    suspend fun readMetadata(url: String): GetMetadataResponse {
        val connection = URL("$baseUrl?url=${java.net.URLEncoder.encode(url, "UTF-8")}").openConnection() as HttpURLConnection

        if (connection.responseCode != HttpURLConnection.HTTP_OK) {
            throw XMTPException("Failed to read metadata for $url, response code $connection.responseCode")
        }

        val response = connection.inputStream.bufferedReader().use { it.readText() }
        return Gson().fromJson(response, GetMetadataResponse::class.java)
    }

    fun post(url: String, payload: Any): GetMetadataResponse {
        val connection = URL("$baseUrl?url=${java.net.URLEncoder.encode(url, "UTF-8")}").openConnection() as HttpURLConnection
        connection.requestMethod = "POST"
        connection.setRequestProperty("Content-Type", "application/json; utf-8")
        connection.doOutput = true

        val gson = Gson()
        val jsonInputString = gson.toJson(payload)

        connection.outputStream.use { os ->
            val writer = OutputStreamWriter(os, "UTF-8")
            writer.write(jsonInputString)
            writer.flush()
            writer.close()
        }

        if (connection.responseCode != HttpURLConnection.HTTP_OK) {
            throw Exception("Failed to post to frame: ${connection.responseCode} ${connection.responseMessage}")
        }

        val response = connection.inputStream.bufferedReader().use { it.readText() }
        return gson.fromJson(response, GetMetadataResponse::class.java)
    }

    suspend fun postRedirect(url: String, payload: Any): PostRedirectResponse {
        val connection = URL("$baseUrl/redirect?url=${java.net.URLEncoder.encode(url, "UTF-8")}").openConnection() as HttpURLConnection
        connection.requestMethod = "POST"
        connection.setRequestProperty("Content-Type", "application/json; utf-8")
        connection.doOutput = true

        val gson = Gson()
        val jsonInputString = gson.toJson(payload)

        connection.outputStream.use { os ->
            val writer = OutputStreamWriter(os, "UTF-8")
            writer.write(jsonInputString)
            writer.flush()
            writer.close()
        }

        if (connection.responseCode != HttpURLConnection.HTTP_OK) {
            throw XMTPException("Failed to post to frame: ${connection.responseMessage}, resoinse code $connection.responseCode")
        }

        val response = connection.inputStream.bufferedReader().use { it.readText() }
        return gson.fromJson(response, PostRedirectResponse::class.java)
    }

    fun mediaUrl(url: String): String {
        return "${baseUrl}media?url=${java.net.URLEncoder.encode(url, "UTF-8")}"
    }
}
