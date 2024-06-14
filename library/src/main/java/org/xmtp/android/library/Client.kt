package org.xmtp.android.library

import android.content.Context
import android.os.Build
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Log
import com.google.crypto.tink.subtle.Base64
import com.google.gson.GsonBuilder
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import org.web3j.crypto.Keys
import org.web3j.crypto.Keys.toChecksumAddress
import org.xmtp.android.library.GRPCApiClient.Companion.makeSubscribeRequest
import org.xmtp.android.library.codecs.ContentCodec
import org.xmtp.android.library.codecs.TextCodec
import org.xmtp.android.library.libxmtp.XMTPLogger
import org.xmtp.android.library.messages.ContactBundle
import org.xmtp.android.library.messages.EncryptedPrivateKeyBundle
import org.xmtp.android.library.messages.Envelope
import org.xmtp.android.library.messages.EnvelopeBuilder
import org.xmtp.android.library.messages.InvitationV1ContextBuilder
import org.xmtp.android.library.messages.Pagination
import org.xmtp.android.library.messages.PrivateKeyBundle
import org.xmtp.android.library.messages.PrivateKeyBundleBuilder
import org.xmtp.android.library.messages.PrivateKeyBundleV1
import org.xmtp.android.library.messages.PrivateKeyBundleV2
import org.xmtp.android.library.messages.SealedInvitationHeaderV1
import org.xmtp.android.library.messages.Topic
import org.xmtp.android.library.messages.decrypted
import org.xmtp.android.library.messages.encrypted
import org.xmtp.android.library.messages.ensureWalletSignature
import org.xmtp.android.library.messages.generate
import org.xmtp.android.library.messages.getPublicKeyBundle
import org.xmtp.android.library.messages.rawData
import org.xmtp.android.library.messages.recoverWalletSignerPublicKey
import org.xmtp.android.library.messages.toPublicKeyBundle
import org.xmtp.android.library.messages.toV2
import org.xmtp.android.library.messages.walletAddress
import org.xmtp.proto.message.api.v1.MessageApiOuterClass
import org.xmtp.proto.message.api.v1.MessageApiOuterClass.BatchQueryResponse
import org.xmtp.proto.message.api.v1.MessageApiOuterClass.QueryRequest
import uniffi.xmtpv3.FfiXmtpClient
import uniffi.xmtpv3.createClient
import uniffi.xmtpv3.generateInboxId
import uniffi.xmtpv3.getInboxIdForAddress
import uniffi.xmtpv3.getVersionInfo
import java.io.File
import java.nio.charset.StandardCharsets
import java.security.KeyStore
import java.text.SimpleDateFormat
import java.time.Instant
import java.util.Date
import java.util.Locale
import java.util.TimeZone
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey

typealias PublishResponse = org.xmtp.proto.message.api.v1.MessageApiOuterClass.PublishResponse
typealias QueryResponse = org.xmtp.proto.message.api.v1.MessageApiOuterClass.QueryResponse
typealias PreEventCallback = suspend () -> Unit

data class ClientOptions(
    val api: Api = Api(),
    val preCreateIdentityCallback: PreEventCallback? = null,
    val preEnableIdentityCallback: PreEventCallback? = null,
    val appContext: Context? = null,
    val enableV3: Boolean = false,
    val dbDirectory: String? = null,
    val dbEncryptionKey: ByteArray? = null,
) {
    data class Api(
        val env: XMTPEnvironment = XMTPEnvironment.DEV,
        val isSecure: Boolean = true,
        val appVersion: String? = null,
    )
}

class Client() {
    lateinit var address: String
    lateinit var privateKeyBundleV1: PrivateKeyBundleV1
    lateinit var apiClient: ApiClient
    lateinit var contacts: Contacts
    lateinit var conversations: Conversations
    var logger: XMTPLogger = XMTPLogger()
    val libXMTPVersion: String = getVersionInfo()
    var installationId: String = ""
    private var v3Client: FfiXmtpClient? = null
    private var dbPath: String = ""
    var inboxId: String = ""

    companion object {
        private const val TAG = "Client"

        var codecRegistry = run {
            val registry = CodecRegistry()
            registry.register(codec = TextCodec())
            registry
        }

        fun register(codec: ContentCodec<*>) {
            codecRegistry.register(codec = codec)
        }

        /**
         * Use the {@param api} to fetch any stored keys belonging to {@param address}.
         *
         * The user will need to be prompted to sign to decrypt each bundle.
         */
        suspend fun authCheck(api: ApiClient, address: String): List<EncryptedPrivateKeyBundle> {
            val topic = Topic.userPrivateStoreKeyBundle(toChecksumAddress(address))
            val res = api.queryTopic(topic)
            return res.envelopesList.mapNotNull {
                try {
                    EncryptedPrivateKeyBundle.parseFrom(it.message)
                } catch (e: Exception) {
                    Log.e(TAG, "discarding malformed private key bundle: ${e.message}", e)
                    null
                }
            }
        }

        /**
         * Use the {@param api} to save the {@param encryptedKeys} for {@param address}.
         *
         * The {@param keys} are used to authorize the publish request.
         */
        suspend fun authSave(
            api: ApiClient,
            v1Key: PrivateKeyBundleV1,
            encryptedKeys: EncryptedPrivateKeyBundle,
        ) {
            val authorizedIdentity = AuthorizedIdentity(v1Key)
            authorizedIdentity.address = v1Key.walletAddress
            val authToken = authorizedIdentity.createAuthToken()
            api.setAuthToken(authToken)
            api.publish(
                envelopes = listOf(
                    EnvelopeBuilder.buildFromTopic(
                        topic = Topic.userPrivateStoreKeyBundle(v1Key.walletAddress),
                        timestamp = Date(),
                        message = encryptedKeys.toByteArray(),
                    ),
                ),
            )
        }

        fun canMessage(peerAddress: String, options: ClientOptions? = null): Boolean {
            val clientOptions = options ?: ClientOptions()
            val api = GRPCApiClient(
                environment = clientOptions.api.env,
                secure = clientOptions.api.isSecure,
            )
            return runBlocking {
                val topics = api.queryTopic(Topic.contact(peerAddress)).envelopesList
                topics.isNotEmpty()
            }
        }
    }

    constructor(
        address: String,
        privateKeyBundleV1: PrivateKeyBundleV1,
        apiClient: ApiClient,
        libXMTPClient: FfiXmtpClient? = null,
        dbPath: String = "",
        installationId: String = "",
        inboxId: String = "",
    ) : this() {
        this.address = address
        this.privateKeyBundleV1 = privateKeyBundleV1
        this.apiClient = apiClient
        this.contacts = Contacts(client = this)
        this.v3Client = libXMTPClient
        this.conversations =
            Conversations(client = this, libXMTPConversations = libXMTPClient?.conversations())
        this.dbPath = dbPath
        this.installationId = installationId
        this.inboxId = inboxId
    }

    fun buildFrom(
        bundle: PrivateKeyBundleV1,
        options: ClientOptions? = null,
        account: SigningKey? = null,
    ): Client {
        return buildFromV1Bundle(bundle, options, account)
    }

    fun create(
        account: SigningKey,
        options: ClientOptions? = null,
    ): Client {
        val clientOptions = options ?: ClientOptions()
        val apiClient =
            GRPCApiClient(
                environment = clientOptions.api.env,
                secure = clientOptions.api.isSecure,
            )
        return create(
            account = account,
            apiClient = apiClient,
            options = options,
        )
    }

    fun create(
        account: SigningKey,
        apiClient: ApiClient,
        options: ClientOptions? = null,
    ): Client {
        val clientOptions = options ?: ClientOptions()
        return runBlocking {
            try {
                val privateKeyBundleV1 = loadOrCreateKeys(
                    account,
                    apiClient,
                    clientOptions
                )

                val (libXMTPClient, dbPath) =
                    ffiXmtpClient(
                        options,
                        account,
                        clientOptions.appContext,
                        privateKeyBundleV1,
                        account.address
                    )

                val client =
                    Client(
                        account.address,
                        privateKeyBundleV1,
                        apiClient,
                        libXMTPClient,
                        dbPath,
                        libXMTPClient?.installationId()?.toHex() ?: "",
                        libXMTPClient?.inboxId() ?: ""
                    )
                client.ensureUserContactPublished()
                client
            } catch (e: java.lang.Exception) {
                throw XMTPException("Error creating client ${e.message}", e)
            }
        }
    }

    fun buildFromBundle(
        bundle: PrivateKeyBundle,
        options: ClientOptions? = null,
        account: SigningKey? = null,
    ): Client =
        buildFromV1Bundle(v1Bundle = bundle.v1, account = account, options = options)

    fun buildFromV1Bundle(
        v1Bundle: PrivateKeyBundleV1,
        options: ClientOptions? = null,
        account: SigningKey? = null,
    ): Client {
        val address = v1Bundle.identityKey.publicKey.recoverWalletSignerPublicKey().walletAddress
        val newOptions = options ?: ClientOptions()
        val apiClient =
            GRPCApiClient(
                environment = newOptions.api.env,
                secure = newOptions.api.isSecure,
            )
        val (v3Client, dbPath) = if (isV3Enabled(options)) {
            runBlocking {
                ffiXmtpClient(
                    options,
                    account,
                    options?.appContext,
                    v1Bundle,
                    address
                )
            }
        } else Pair(null, "")

        return Client(
            address = address,
            privateKeyBundleV1 = v1Bundle,
            apiClient = apiClient,
            libXMTPClient = v3Client,
            dbPath = dbPath,
            installationId = v3Client?.installationId()?.toHex() ?: "",
            inboxId = v3Client?.inboxId() ?: ""
        )
    }

    private fun isV3Enabled(options: ClientOptions?): Boolean {
        return (options != null && options.enableV3 && options.appContext != null)
    }

    private suspend fun ffiXmtpClient(
        options: ClientOptions?,
        account: SigningKey?,
        appContext: Context?,
        privateKeyBundleV1: PrivateKeyBundleV1,
        address: String,
    ): Pair<FfiXmtpClient?, String> {
        var dbPath = ""
        val accountAddress = address.lowercase()
        val v3Client: FfiXmtpClient? =
            if (isV3Enabled(options)) {
                var inboxId = getInboxIdForAddress(
                    logger = logger,
                    host = options!!.api.env.getUrl(),
                    isSecure = options.api.isSecure,
                    accountAddress = accountAddress
                )

                if (inboxId.isNullOrBlank()) {
                    inboxId = generateInboxId(accountAddress, 0.toULong())
                }

                val alias = "xmtp-${options.api.env}-$inboxId"

                val mlsDbDirectory = options.dbDirectory
                val directoryFile = if (mlsDbDirectory != null) {
                    File(mlsDbDirectory)
                } else {
                    File(appContext?.filesDir?.absolutePath, "xmtp_db")
                }
                directoryFile.mkdir()
                dbPath = directoryFile.absolutePath + "/$alias.db3"

                val encryptionKey = if (options.dbEncryptionKey == null) {
                    val keyStore = KeyStore.getInstance("AndroidKeyStore")
                    withContext(Dispatchers.IO) {
                        keyStore.load(null)
                    }

                    val entry = keyStore.getEntry(alias, null)

                    val retrievedKey: SecretKey = if (entry is KeyStore.SecretKeyEntry) {
                        entry.secretKey
                    } else {
                        val keyGenerator =
                            KeyGenerator.getInstance(
                                KeyProperties.KEY_ALGORITHM_AES,
                                "AndroidKeyStore"
                            )
                        val keyGenParameterSpec = KeyGenParameterSpec.Builder(
                            alias,
                            KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT
                        ).setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                            .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                            .setKeySize(256)
                            .build()

                        keyGenerator.init(keyGenParameterSpec)
                        keyGenerator.generateKey()
                    }
                    retrievedKey.encoded
                } else {
                    options.dbEncryptionKey
                }

                createClient(
                    logger = logger,
                    host = options.api.env.getUrl(),
                    isSecure = options.api.isSecure,
                    db = dbPath,
                    encryptionKey = encryptionKey,
                    accountAddress = accountAddress,
                    inboxId = inboxId,
                    nonce = 0.toULong(),
                    legacySignedPrivateKeyProto = privateKeyBundleV1.toV2().identityKey.toByteArray()
                )
            } else {
                null
            }

        if (v3Client != null) {
            v3Client.signatureRequest()?.let { signatureRequest ->
                if (account != null) {
                    account.sign(signatureRequest.signatureText())?.let {
                        signatureRequest.addEcdsaSignature(it.rawData)
                    }
                    v3Client.registerIdentity(signatureRequest)
                } else {
                    throw XMTPException("No signer passed but signer was required.")
                }
            }
        }
        Log.i(TAG, "LibXMTP $libXMTPVersion")
        return Pair(v3Client, dbPath)
    }

    /**
     * This authenticates using [account] acquired from network storage
     *  encrypted using the [wallet].
     *
     *  e.g. this might be called the first time a user logs in from a new device.
     *  The next time they launch the app they can [buildFromV1Key].
     *
     *  If there are stored keys then this asks the [wallet] to
     *  [encrypted] so that we can decrypt the stored [keys].
     *
     *   If there are no stored keys then this generates a new identityKey
     *   and asks the [wallet] to both [createIdentity] and enable Identity Saving
     *   so we can then store it encrypted for the next time.
     */
    private suspend fun loadOrCreateKeys(
        account: SigningKey,
        apiClient: ApiClient,
        options: ClientOptions? = null,
    ): PrivateKeyBundleV1 {
        val keys = loadPrivateKeys(account, apiClient, options)
        return if (keys != null) {
            keys
        } else {
            val v1Keys = PrivateKeyBundleV1.newBuilder().build().generate(account, options)
            val keyBundle = PrivateKeyBundleBuilder.buildFromV1Key(v1Keys)
            val encryptedKeys = keyBundle.encrypted(account, options?.preEnableIdentityCallback)
            authSave(apiClient, keyBundle.v1, encryptedKeys)
            v1Keys
        }
    }

    /**
     *  This authenticates with [keys] directly received.
     *  e.g. this might be called on subsequent app launches once we
     *  have already stored the keys from a previous session.
     */
    private suspend fun loadPrivateKeys(
        account: SigningKey,
        apiClient: ApiClient,
        options: ClientOptions? = null,
    ): PrivateKeyBundleV1? {
        val encryptedBundles = authCheck(apiClient, account.address)
        for (encryptedBundle in encryptedBundles) {
            try {
                val bundle =
                    encryptedBundle.decrypted(account, options?.preEnableIdentityCallback)
                return bundle.v1
            } catch (e: Throwable) {
                print("Error decoding encrypted private key bundle: $e")
                continue
            }
        }
        return null
    }

    suspend fun publishUserContact(legacy: Boolean = false) {
        val envelopes: MutableList<MessageApiOuterClass.Envelope> = mutableListOf()
        if (legacy) {
            val contactBundle = ContactBundle.newBuilder().also {
                it.v1 = it.v1.toBuilder().also { v1Builder ->
                    v1Builder.keyBundle = privateKeyBundleV1.toPublicKeyBundle()
                }.build()
            }.build()

            val envelope = MessageApiOuterClass.Envelope.newBuilder().apply {
                contentTopic = Topic.contact(address).description
                timestampNs = Date().time * 1_000_000
                message = contactBundle.toByteString()
            }.build()

            envelopes.add(envelope)
        }
        val contactBundle = ContactBundle.newBuilder().also {
            it.v2 = it.v2.toBuilder().also { v2Builder ->
                v2Builder.keyBundle = keys.getPublicKeyBundle()
            }.build()
            it.v2 = it.v2.toBuilder().also { v2Builder ->
                v2Builder.keyBundle = v2Builder.keyBundle.toBuilder().also { keyBuilder ->
                    keyBuilder.identityKey =
                        keyBuilder.identityKey.toBuilder().also { idBuilder ->
                            idBuilder.signature =
                                it.v2.keyBundle.identityKey.signature.ensureWalletSignature()
                        }.build()
                }.build()
            }.build()
        }.build()
        val envelope = MessageApiOuterClass.Envelope.newBuilder().apply {
            contentTopic = Topic.contact(address).description
            timestampNs = Date().time * 1_000_000
            message = contactBundle.toByteString()
        }.build()
        envelopes.add(envelope)
        publish(envelopes = envelopes)
    }

    fun getUserContact(peerAddress: String): ContactBundle? {
        return contacts.find(Keys.toChecksumAddress(peerAddress))
    }

    suspend fun query(topic: Topic, pagination: Pagination? = null): QueryResponse {
        return apiClient.queryTopic(topic = topic, pagination = pagination)
    }

    suspend fun batchQuery(requests: List<QueryRequest>): BatchQueryResponse {
        return apiClient.batchQuery(requests)
    }

    suspend fun subscribe(topics: List<String>): Flow<Envelope> {
        return subscribe2(flowOf(makeSubscribeRequest(topics)))
    }

    suspend fun subscribe2(request: Flow<MessageApiOuterClass.SubscribeRequest>): Flow<Envelope> {
        return apiClient.subscribe(request = request)
    }

    suspend fun fetchConversation(
        topic: String?,
        includeGroups: Boolean = false,
    ): Conversation? {
        if (topic.isNullOrBlank()) return null
        return conversations.list(includeGroups = includeGroups).firstOrNull {
            it.topic == topic
        }
    }

    suspend fun publish(envelopes: List<Envelope>): PublishResponse {
        val authorized = AuthorizedIdentity(
            address = address,
            authorized = privateKeyBundleV1.identityKey.publicKey,
            identity = privateKeyBundleV1.identityKey,
        )
        val authToken = authorized.createAuthToken()
        apiClient.setAuthToken(authToken)

        return apiClient.publish(envelopes = envelopes)
    }

    suspend fun ensureUserContactPublished() {
        val contact = getUserContact(peerAddress = address)
        if (contact != null && keys.getPublicKeyBundle() == contact.v2.keyBundle) {
            return
        }

        publishUserContact(legacy = true)
    }

    fun importConversation(conversationData: ByteArray): Conversation {
        val gson = GsonBuilder().create()
        val v2Export = gson.fromJson(
            conversationData.toString(StandardCharsets.UTF_8),
            ConversationV2Export::class.java,
        )
        return try {
            importV2Conversation(export = v2Export)
        } catch (e: java.lang.Exception) {
            val v1Export = gson.fromJson(
                conversationData.toString(StandardCharsets.UTF_8),
                ConversationV1Export::class.java,
            )
            try {
                importV1Conversation(export = v1Export)
            } catch (e: java.lang.Exception) {
                throw XMTPException("Invalid input data", e)
            }
        }
    }

    fun importV2Conversation(export: ConversationV2Export): Conversation {
        val keyMaterial = Base64.decode(export.keyMaterial)
        return Conversation.V2(
            ConversationV2(
                topic = export.topic,
                keyMaterial = keyMaterial,
                context = InvitationV1ContextBuilder.buildFromConversation(
                    conversationId = export.context?.conversationId ?: "",
                    metadata = export.context?.metadata ?: mapOf(),
                ),
                peerAddress = export.peerAddress,
                client = this,
                header = SealedInvitationHeaderV1.newBuilder().build(),
            ),
        )
    }

    fun importV1Conversation(export: ConversationV1Export): Conversation {
        val sentAt = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            Date.from(Instant.parse(export.createdAt))
        } else {
            val df = SimpleDateFormat("yyyy-MM-dd'T'HH:mm:ss", Locale.getDefault())
            df.timeZone = TimeZone.getTimeZone("UTC")
            df.parse(export.createdAt)
        }
        return Conversation.V1(
            ConversationV1(
                client = this,
                peerAddress = export.peerAddress,
                sentAt = sentAt,
            ),
        )
    }

    /**
     * Whether or not we can send messages to [address].
     * @param peerAddress is the address of the client that you want to send messages
     *
     * @return false when [peerAddress] has never signed up for XMTP
     * or when the message is addressed to the sender (no self-messaging).
     */
    fun canMessage(peerAddress: String): Boolean {
        return runBlocking { query(Topic.contact(peerAddress)).envelopesList.size > 0 }
    }

    suspend fun canMessageV3(addresses: List<String>): Map<String, Boolean> {
        v3Client?.let {
            return it.canMessage(addresses)
        }
        throw XMTPException("Error no V3 client initialized")
    }

    fun deleteLocalDatabase() {
        File(dbPath).delete()
    }

    @Deprecated(
        message = "This function is delicate and should be used with caution. App will error if database not properly reconnected. See: reconnectLocalDatabase()",
    )
    fun dropLocalDatabaseConnection() {
        v3Client?.releaseDbConnection()
    }

    suspend fun reconnectLocalDatabase() {
        v3Client?.dbReconnect() ?: throw XMTPException("Error no V3 client initialized")
    }

    val privateKeyBundle: PrivateKeyBundle
        get() = PrivateKeyBundleBuilder.buildFromV1Key(privateKeyBundleV1)

    val v1keys: PrivateKeyBundleV1
        get() = privateKeyBundleV1

    val keys: PrivateKeyBundleV2
        get() = privateKeyBundleV1.toV2()
}
