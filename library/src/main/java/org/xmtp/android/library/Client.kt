package org.xmtp.android.library

import android.content.Context
import kotlinx.coroutines.runBlocking
import org.xmtp.android.library.codecs.ContentCodec
import org.xmtp.android.library.codecs.TextCodec
import org.xmtp.android.library.libxmtp.Message
import org.xmtp.android.library.libxmtp.XMTPLogger
import org.xmtp.android.library.messages.rawData
import uniffi.xmtpv3.FfiXmtpClient
import uniffi.xmtpv3.createClient
import uniffi.xmtpv3.generateInboxId
import uniffi.xmtpv3.getInboxIdForAddress
import uniffi.xmtpv3.getVersionInfo
import uniffi.xmtpv3.org.xmtp.android.library.libxmtp.InboxState
import java.io.File

typealias PreEventCallback = suspend () -> Unit

data class ClientOptions(
    val api: Api = Api(),
    val preAuthenticateToInboxCallback: PreEventCallback? = null,
    val appContext: Context,
    val dbEncryptionKey: ByteArray,
    val historySyncUrl: String = when (api.env) {
        XMTPEnvironment.PRODUCTION -> "https://message-history.production.ephemera.network/"
        XMTPEnvironment.LOCAL -> "http://0.0.0.0:5558"
        else -> "https://message-history.dev.ephemera.network/"
    },
    val dbDirectory: String? = null,
) {
    data class Api(
        val env: XMTPEnvironment = XMTPEnvironment.DEV,
        val isSecure: Boolean = true,
        val appVersion: String? = null,
    )
}

class Client() {
    lateinit var address: String
    lateinit var inboxId: String
    lateinit var installationId: String
    lateinit var preferences: PrivatePreferences
    lateinit var conversations: Conversations
    lateinit var environment: XMTPEnvironment
    lateinit var dbPath: String
    var logger: XMTPLogger = XMTPLogger()
    val libXMTPVersion: String = getVersionInfo()
    private lateinit var ffiClient: FfiXmtpClient

    companion object {
        private const val TAG = "Client"

        var codecRegistry = run {
            val registry = CodecRegistry()
            registry.register(codec = TextCodec())
            registry
        }

        suspend fun getOrCreateInboxId(options: ClientOptions, address: String): String {
            var inboxId = getInboxIdForAddress(
                logger = XMTPLogger(),
                host = options.api.env.getUrl(),
                isSecure = options.api.isSecure,
                accountAddress = address
            )
            if (inboxId.isNullOrBlank()) {
                inboxId = generateInboxId(address, 0.toULong())
            }
            return inboxId
        }

        fun register(codec: ContentCodec<*>) {
            codecRegistry.register(codec = codec)
        }
    }

    constructor(
        address: String,
        libXMTPClient: FfiXmtpClient,
        dbPath: String,
        installationId: String,
        inboxId: String,
        environment: XMTPEnvironment,
    ) : this() {
        this.address = address
        this.preferences = PrivatePreferences(client = this, ffiClient = libXMTPClient)
        this.ffiClient = libXMTPClient
        this.conversations =
            Conversations(client = this, ffiConversations = libXMTPClient.conversations())
        this.dbPath = dbPath
        this.installationId = installationId
        this.inboxId = inboxId
        this.environment = environment
    }

    private suspend fun initializeV3Client(
        address: String,
        clientOptions: ClientOptions,
        signingKey: SigningKey? = null,
    ): Client {
        val accountAddress = address.lowercase()
        val inboxId = getOrCreateInboxId(clientOptions, accountAddress)

        val (ffiClient, dbPath) = createFfiClient(
            accountAddress,
            inboxId,
            clientOptions,
            signingKey,
            clientOptions.appContext,
        )

        return Client(
            accountAddress,
            ffiClient,
            dbPath,
            ffiClient.installationId().toHex(),
            ffiClient.inboxId(),
            clientOptions.api.env
        )
    }

    // Function to create a V3 client with a signing key
    suspend fun create(
        account: SigningKey,
        options: ClientOptions,
    ): Client {
        return try {
            initializeV3Client(account.address, options, account)
        } catch (e: Exception) {
            throw XMTPException("Error creating V3 client: ${e.message}", e)
        }
    }

    // Function to build a V3 client from a address
    suspend fun build(
        address: String,
        options: ClientOptions,
    ): Client {
        return try {
            initializeV3Client(address, options)
        } catch (e: Exception) {
            throw XMTPException("Error creating V3 client: ${e.message}", e)
        }
    }

    private suspend fun createFfiClient(
        accountAddress: String,
        inboxId: String,
        options: ClientOptions,
        signingKey: SigningKey?,
        appContext: Context,
    ): Pair<FfiXmtpClient, String> {
        val alias = "xmtp-${options.api.env}-$inboxId"

        val mlsDbDirectory = options.dbDirectory
        val directoryFile = if (mlsDbDirectory != null) {
            File(mlsDbDirectory)
        } else {
            File(appContext.filesDir.absolutePath, "xmtp_db")
        }
        directoryFile.mkdir()
        dbPath = directoryFile.absolutePath + "/$alias.db3"

        val ffiClient = createClient(
            logger = logger,
            host = options.api.env.getUrl(),
            isSecure = options.api.isSecure,
            db = dbPath,
            encryptionKey = options.dbEncryptionKey,
            accountAddress = accountAddress,
            inboxId = inboxId,
            nonce = 0.toULong(),
            legacySignedPrivateKeyProto = null,
            historySyncUrl = options.historySyncUrl
        )

        options.preAuthenticateToInboxCallback?.let {
            runBlocking {
                it.invoke()
            }
        }
        ffiClient.signatureRequest()?.let { signatureRequest ->
            if (signingKey != null) {
                if (signingKey.type == WalletType.SCW) {
                    val chainId = signingKey.chainId
                        ?: throw XMTPException("ChainId is required for smart contract wallets")
                    signatureRequest.addScwSignature(
                        signingKey.signSCW(signatureRequest.signatureText()),
                        signingKey.address.lowercase(),
                        chainId.toULong(),
                        signingKey.blockNumber?.toULong()
                    )
                } else {
                    signingKey.sign(signatureRequest.signatureText())?.let {
                        signatureRequest.addEcdsaSignature(it.rawData)
                    }
                }

                ffiClient.registerIdentity(signatureRequest)
            } else {
                throw XMTPException("No signer passed but signer was required.")
            }
        }

        return Pair(ffiClient, dbPath)
    }

    fun findGroup(groupId: String): Group? {
        try {
            return Group(this, ffiClient.conversation(groupId.hexToByteArray()))
        } catch (e: Exception) {
            return null
        }
    }

    fun findConversation(conversationId: String): Conversation? {
        val conversation = ffiClient.conversation(conversationId.hexToByteArray())
        return if (conversation.groupMetadata().conversationType() == "dm") {
            Conversation.Dm(Dm(this, conversation))
        } else if (conversation.groupMetadata().conversationType() == "group") {
            Conversation.Group(Group(this, conversation))
        } else {
            null
        }
    }

    fun findConversationByTopic(topic: String): Conversation? {
        val regex = """/xmtp/mls/1/g-(.*?)/proto""".toRegex()
        val matchResult = regex.find(topic)
        val conversationId = matchResult?.groupValues?.get(1) ?: ""
        val conversation = ffiClient.conversation(conversationId.hexToByteArray())
        return if (conversation.groupMetadata().conversationType() == "dm") {
            Conversation.Dm(Dm(this, conversation))
        } else if (conversation.groupMetadata().conversationType() == "group") {
            Conversation.Group(Group(this, conversation))
        } else {
            null
        }
    }

    suspend fun findDm(address: String): Dm? {
        val inboxId =
            inboxIdFromAddress(address.lowercase()) ?: throw XMTPException("No inboxId present")
        try {
            return Dm(this, ffiClient.dmConversation(inboxId))
        } catch (e: Exception) {
            return null
        }
    }

    fun findMessage(messageId: String): Message? {
        return try {
            Message(this, ffiClient.message(messageId.hexToByteArray()))
        } catch (e: Exception) {
            null
        }
    }

    suspend fun canMessage(addresses: List<String>): Map<String, Boolean> {
        return ffiClient.canMessage(addresses)
    }

    suspend fun inboxIdFromAddress(address: String): String? {
        return ffiClient.findInboxId(address.lowercase())
    }

    fun deleteLocalDatabase() {
        dropLocalDatabaseConnection()
        File(dbPath).delete()
    }

    @Deprecated(
        message = "This function is delicate and should be used with caution. App will error if database not properly reconnected. See: reconnectLocalDatabase()",
    )
    fun dropLocalDatabaseConnection() {
        ffiClient.releaseDbConnection()
    }

    suspend fun reconnectLocalDatabase() {
        ffiClient.dbReconnect()
    }

    suspend fun requestMessageHistorySync() {
        ffiClient.requestHistorySync()
    }

    suspend fun revokeAllOtherInstallations(signingKey: SigningKey) {
        val signatureRequest = ffiClient.revokeAllOtherInstallations()
        signingKey.sign(signatureRequest.signatureText())?.let {
            signatureRequest.addEcdsaSignature(it.rawData)
            ffiClient.applySignatureRequest(signatureRequest)
        }
    }

    suspend fun inboxState(refreshFromNetwork: Boolean): InboxState {
        return InboxState(ffiClient.inboxState(refreshFromNetwork))
    }
}
