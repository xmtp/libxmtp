package org.xmtp.android.library

import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import uniffi.xmtp_sdk.*
import java.io.File
import java.net.URL
import java.nio.file.Files
import java.security.SecureRandom

// Cache-key tests do not open this fixed address.
fun localApi(appVersion: String? = null): BackendOptions =
    BackendOptions(url = "http://10.0.2.2:5050", appVersion = appVersion)

class TestFetcher(
    private val file: File,
) {
    fun fetch(url: URL): ByteArray {
        check(url.protocol == "https")
        return file.readBytes()
    }
}

class Fixtures : AutoCloseable {
    private val directory = Files.createTempDirectory("xmtp-jvm-fixtures").toFile()
    private val key = SecureRandom().generateSeed(32)
    private val backend =
        BackendSource.Options(
            BackendOptions(url = System.getenv("XMTP_BACKEND_URL") ?: "http://127.0.0.1:5050"),
        )
    val aliceAccount = runBlocking { generateLocalSigner() }
    val bobAccount = runBlocking { generateLocalSigner() }
    val alice = runBlocking { aliceAccount.identity() }
    val bob = runBlocking { bobAccount.identity() }
    val aliceClient = create(aliceAccount, "alice")
    val bobClient = create(bobAccount, "bob")

    private fun create(
        signer: Signer,
        label: String,
    ): SDKClient =
        runBlocking {
            val db = File(directory, label).also { check(it.mkdirs()) }
            SDKClient.create(
                signer,
                ClientOptions(
                    backend = backend,
                    storage = StorageOptions(StorageLocation.Directory(db.absolutePath), encryptionKey = key),
                ),
            )
        }

    override fun close() {
        try {
            runBlocking {
                withContext(NonCancellable) {
                    aliceClient.end()
                    bobClient.end()
                }
            }
        } finally {
            directory.deleteRecursively()
        }
    }
}

fun fixtures(): Fixtures = Fixtures()

// Use the real generated forwarders with a recording native boundary.
internal fun testSDKClient(
    raw: Client,
    codecs: List<ContentCodec<*>> = emptyList(),
): SDKClient {
    val constructor = SDKClient::class.java.getDeclaredConstructor(Client::class.java, List::class.java)
    constructor.isAccessible = true
    return constructor.newInstance(raw, codecs)
}
