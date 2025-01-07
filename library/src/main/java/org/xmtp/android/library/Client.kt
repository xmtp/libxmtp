package org.xmtp.android.library

import android.content.Context
import kotlinx.coroutines.runBlocking
import org.xmtp.android.library.codecs.ContentCodec
import org.xmtp.android.library.codecs.TextCodec
import org.xmtp.android.library.libxmtp.InboxState
import org.xmtp.android.library.libxmtp.Message
import org.xmtp.android.library.messages.rawData
import uniffi.xmtpv3.FfiConversationType
import uniffi.xmtpv3.FfiDeviceSyncKind
import uniffi.xmtpv3.FfiSignatureRequest
import uniffi.xmtpv3.FfiXmtpClient
import uniffi.xmtpv3.XmtpApiClient
import uniffi.xmtpv3.connectToBackend
import uniffi.xmtpv3.createClient
import uniffi.xmtpv3.generateInboxId
import uniffi.xmtpv3.getInboxIdForAddress
import uniffi.xmtpv3.getVersionInfo
import java.io.File

typealias PreEventCallback = suspend () -> Unit

data class ClientOptions(
    val api: Api = Api(),
    val preAuthenticateToInboxCallback: PreEventCallback? = null,
    val appContext: Context,
    val dbEncryptionKey: ByteArray,
    val historySyncUrl: String = when (api.env) {
        XMTPEnvironment.PRODUCTION -> "https://message-history.production.ephemera.network/"
        XMTPEnvironment.LOCAL -> "http://10.0.2.2:5558"
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
    val libXMTPVersion: String = getVersionInfo()
    private lateinit var ffiClient: FfiXmtpClient

    companion object {
        private const val TAG = "Client"

        var codecRegistry = run {
            val registry = CodecRegistry()
            registry.register(codec = TextCodec())
            registry
        }

        private val apiClientCache = mutableMapOf<String, XmtpApiClient>()
        private val cacheLock = Any()

        suspend fun connectToApiBackend(api: ClientOptions.Api): XmtpApiClient {
            val cacheKey = api.env.getUrl()
            return synchronized(cacheLock) {
                apiClientCache.getOrPut(cacheKey) {
                    runBlocking {
                        connectToBackend(api.env.getUrl(), api.isSecure)
                    }
                }
            }
        }

        suspend fun getOrCreateInboxId(
            api: ClientOptions.Api,
            address: String,
        ): String {
            var inboxId = getInboxIdForAddress(
                api = connectToApiBackend(api),
                accountAddress = address.lowercase()
            )
            if (inboxId.isNullOrBlank()) {
                inboxId = generateInboxId(address.lowercase(), 0.toULong())
            }
            return inboxId
        }

        fun register(codec: ContentCodec<*>) {
            codecRegistry.register(codec = codec)
        }

        suspend fun canMessage(
            accountAddresses: List<String>,
            appContext: Context,
            api: ClientOptions.Api,
        ): Map<String, Boolean> {
            val accountAddress = "0x0000000000000000000000000000000000000000"
            val inboxId = getOrCreateInboxId(api, accountAddress)
            val alias = "xmtp-${api.env}-$inboxId"

            val directoryFile = File(appContext.filesDir.absolutePath, "xmtp_db")
            directoryFile.mkdir()
            val dbPath = directoryFile.absolutePath + "/$alias.db3"

            val ffiClient = createClient(
                api = connectToApiBackend(api),
                db = dbPath,
                encryptionKey = null,
                accountAddress = accountAddress.lowercase(),
                inboxId = inboxId,
                nonce = 0.toULong(),
                legacySignedPrivateKeyProto = null,
                historySyncUrl = null
            )

            val result = ffiClient.canMessage(accountAddresses)
            ffiClient.releaseDbConnection()
            File(dbPath).delete()

            return result
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
        this.address = address.lowercase()
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
        inboxId: String? = null,
    ): Client {
        val accountAddress = address.lowercase()
        val recoveredInboxId =
            inboxId ?: getOrCreateInboxId(clientOptions.api, accountAddress)

        val (ffiClient, dbPath) = createFfiClient(
            accountAddress,
            recoveredInboxId,
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
            clientOptions.api.env,
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
        inboxId: String? = null,
    ): Client {
        return try {
            initializeV3Client(address, options, inboxId = inboxId)
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
            api = connectToApiBackend(options.api),
            db = dbPath,
            encryptionKey = options.dbEncryptionKey,
            accountAddress = accountAddress.lowercase(),
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
            signingKey?.let { handleSignature(signatureRequest, it) }
                ?: throw XMTPException("No signer passed but signer was required.")
            ffiClient.registerIdentity(signatureRequest)
        }
        return Pair(ffiClient, dbPath)
    }

    suspend fun revokeAllOtherInstallations(signingKey: SigningKey) {
        val signatureRequest = ffiClient.revokeAllOtherInstallations()
        handleSignature(signatureRequest, signingKey)
        ffiClient.applySignatureRequest(signatureRequest)
    }

    suspend fun addAccount(newAccount: SigningKey) {
        val signatureRequest =
            ffiClient.addWallet(newAccount.address.lowercase())
        handleSignature(signatureRequest, newAccount)
        ffiClient.applySignatureRequest(signatureRequest)
    }

    suspend fun removeAccount(recoverAccount: SigningKey, addressToRemove: String) {
        val signatureRequest = ffiClient.revokeWallet(addressToRemove.lowercase())
        handleSignature(signatureRequest, recoverAccount)
        ffiClient.applySignatureRequest(signatureRequest)
    }

    private suspend fun handleSignature(
        signatureRequest: FfiSignatureRequest,
        signingKey: SigningKey,
    ) {
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
    }

    fun signWithInstallationKey(message: String): ByteArray {
        return ffiClient.signWithInstallationKey(message)
    }

    fun verifySignature(message: String, signature: ByteArray): Boolean {
        return try {
            ffiClient.verifySignedWithInstallationKey(message, signature)
            true
        } catch (e: Exception) {
            false
        }
    }

    fun verifySignatureWithInstallationId(
        message: String,
        signature: ByteArray,
        installationId: String,
    ): Boolean {
        return try {
            ffiClient.verifySignedWithPublicKey(message, signature, installationId.hexToByteArray())
            true
        } catch (e: Exception) {
            false
        }
    }

    fun findGroup(groupId: String): Group? {
        return try {
            Group(this, ffiClient.conversation(groupId.hexToByteArray()))
        } catch (e: Exception) {
            null
        }
    }

    suspend fun findConversation(conversationId: String): Conversation? {
        return try {
            val conversation = ffiClient.conversation(conversationId.hexToByteArray())
            when (conversation.conversationType()) {
                FfiConversationType.GROUP -> Conversation.Group(Group(this, conversation))
                FfiConversationType.DM -> Conversation.Dm(Dm(this, conversation))
                else -> null
            }
        } catch (e: Exception) {
            null
        }
    }

    suspend fun findConversationByTopic(topic: String): Conversation? {
        val regex = """/xmtp/mls/1/g-(.*?)/proto""".toRegex()
        val matchResult = regex.find(topic)
        val conversationId = matchResult?.groupValues?.get(1) ?: ""
        return try {
            val conversation = ffiClient.conversation(conversationId.hexToByteArray())
            when (conversation.conversationType()) {
                FfiConversationType.GROUP -> Conversation.Group(Group(this, conversation))
                FfiConversationType.DM -> Conversation.Dm(Dm(this, conversation))
                else -> null
            }
        } catch (e: Exception) {
            null
        }
    }

    fun findDmByInboxId(inboxId: String): Dm? {
        return try {
            Dm(this, ffiClient.dmConversation(inboxId))
        } catch (e: Exception) {
            null
        }
    }

    suspend fun findDmByAddress(address: String): Dm? {
        val inboxId =
            inboxIdFromAddress(address.lowercase()) ?: throw XMTPException("No inboxId present")
        return findDmByInboxId(inboxId)
    }

    fun findMessage(messageId: String): Message? {
        return try {
            Message.create(ffiClient.message(messageId.hexToByteArray()))
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
        ffiClient.sendSyncRequest(FfiDeviceSyncKind.MESSAGES)
    }

    suspend fun inboxStatesForInboxIds(
        refreshFromNetwork: Boolean,
        inboxIds: List<String>,
    ): List<InboxState> {
        return ffiClient.addressesFromInboxId(refreshFromNetwork, inboxIds).map { InboxState(it) }
    }

    suspend fun inboxState(refreshFromNetwork: Boolean): InboxState {
        return InboxState(ffiClient.inboxState(refreshFromNetwork))
    }
}
