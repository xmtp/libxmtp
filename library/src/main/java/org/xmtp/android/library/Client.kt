package org.xmtp.android.library

import android.os.Build
import com.google.crypto.tink.subtle.Base64
import com.google.gson.GsonBuilder
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.runBlocking
import org.web3j.crypto.Keys
import org.xmtp.android.library.codecs.ContentCodec
import org.xmtp.android.library.codecs.TextCodec
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
import org.xmtp.android.library.messages.recoverWalletSignerPublicKey
import org.xmtp.android.library.messages.toPublicKeyBundle
import org.xmtp.android.library.messages.toV2
import org.xmtp.android.library.messages.walletAddress
import org.xmtp.proto.message.api.v1.MessageApiOuterClass
import java.nio.charset.StandardCharsets
import java.text.SimpleDateFormat
import java.time.Instant
import java.util.Date
import java.util.Locale
import java.util.TimeZone

typealias PublishResponse = org.xmtp.proto.message.api.v1.MessageApiOuterClass.PublishResponse
typealias QueryResponse = org.xmtp.proto.message.api.v1.MessageApiOuterClass.QueryResponse

data class ClientOptions(val api: Api = Api()) {
    data class Api(val env: XMTPEnvironment = XMTPEnvironment.DEV, val isSecure: Boolean = true)
}

class Client() {
    lateinit var address: String
    lateinit var privateKeyBundleV1: PrivateKeyBundleV1
    lateinit var apiClient: ApiClient
    lateinit var contacts: Contacts
    lateinit var conversations: Conversations

    companion object {
        var codecRegistry = run {
            val registry = CodecRegistry()
            registry.register(codec = TextCodec())
            registry
        }

        fun register(codec: ContentCodec<*>) {
            codecRegistry.register(codec = codec)
        }
    }

    constructor(
        address: String,
        privateKeyBundleV1: PrivateKeyBundleV1,
        apiClient: ApiClient,
    ) : this() {
        this.address = address
        this.privateKeyBundleV1 = privateKeyBundleV1
        this.apiClient = apiClient
        this.contacts = Contacts(client = this)
        this.conversations = Conversations(client = this)
    }

    fun buildFrom(bundle: PrivateKeyBundle, options: ClientOptions? = null): Client {
        val address = bundle.v1.identityKey.publicKey.recoverWalletSignerPublicKey().walletAddress
        val clientOptions = options ?: ClientOptions()
        val apiClient =
            GRPCApiClient(environment = clientOptions.api.env, secure = clientOptions.api.isSecure)
        return Client(address = address, privateKeyBundleV1 = bundle.v1, apiClient = apiClient)
    }

    fun create(account: SigningKey, options: ClientOptions? = null): Client {
        val clientOptions = options ?: ClientOptions()
        val apiClient =
            GRPCApiClient(environment = clientOptions.api.env, secure = clientOptions.api.isSecure)
        return create(account = account, apiClient = apiClient)
    }

    fun create(account: SigningKey, apiClient: ApiClient): Client {
        return runBlocking {
            try {
                val privateKeyBundleV1 = loadOrCreateKeys(account, apiClient)
                val client = Client(account.address, privateKeyBundleV1, apiClient)
                client.ensureUserContactPublished()
                client
            } catch (e: java.lang.Exception) {
                throw XMTPException("Error creating client", e)
            }
        }
    }

    fun buildFromBundle(bundle: PrivateKeyBundle, options: ClientOptions? = null): Client =
        buildFromV1Bundle(v1Bundle = bundle.v1, options = options)

    fun buildFromV1Bundle(v1Bundle: PrivateKeyBundleV1, options: ClientOptions? = null): Client {
        val address = v1Bundle.identityKey.publicKey.recoverWalletSignerPublicKey().walletAddress
        val newOptions = options ?: ClientOptions()
        val apiClient =
            GRPCApiClient(environment = newOptions.api.env, secure = newOptions.api.isSecure)
        return Client(address = address, privateKeyBundleV1 = v1Bundle, apiClient = apiClient)
    }

    private suspend fun loadOrCreateKeys(
        account: SigningKey,
        apiClient: ApiClient,
    ): PrivateKeyBundleV1 {
        val keys = loadPrivateKeys(account, apiClient)
        if (keys != null) {
            return keys
        } else {
            val v1Keys = PrivateKeyBundleV1.newBuilder().build().generate(account)
            val keyBundle = PrivateKeyBundleBuilder.buildFromV1Key(v1Keys)
            val encryptedKeys = keyBundle.encrypted(account)
            val authorizedIdentity = AuthorizedIdentity(privateKeyBundleV1 = v1Keys)
            authorizedIdentity.address = account.address
            val authToken = authorizedIdentity.createAuthToken()
            apiClient.setAuthToken(authToken)
            apiClient.publish(
                envelopes = listOf(
                    EnvelopeBuilder.buildFromTopic(
                        topic = Topic.userPrivateStoreKeyBundle(account.address),
                        timestamp = Date(),
                        message = encryptedKeys.toByteArray()
                    )
                )
            )
            return v1Keys
        }
    }

    private suspend fun loadPrivateKeys(
        account: SigningKey,
        apiClient: ApiClient,
    ): PrivateKeyBundleV1? {
        val topics: List<Topic> = listOf(Topic.userPrivateStoreKeyBundle(account.address))
        val res = apiClient.queryTopics(topics = topics)
        for (envelope in res.envelopesList) {
            try {
                val encryptedBundle = EncryptedPrivateKeyBundle.parseFrom(envelope.message)
                val bundle = encryptedBundle.decrypted(account)
                return bundle.v1
            } catch (e: Throwable) {
                print("Error decoding encrypted private key bundle: $e")
                continue
            }
        }
        return null
    }

    fun publishUserContact(legacy: Boolean = false) {
        val envelopes: MutableList<MessageApiOuterClass.Envelope> = mutableListOf()
        if (legacy) {
            val contactBundle = ContactBundle.newBuilder().also {
                it.v1Builder.keyBundle = privateKeyBundleV1.toPublicKeyBundle()
            }.build()

            val envelope = MessageApiOuterClass.Envelope.newBuilder().apply {
                contentTopic = Topic.contact(address).description
                timestampNs = Date().time * 1_000_000
                message = contactBundle.toByteString()
            }.build()

            envelopes.add(envelope)
        }
        val contactBundle = ContactBundle.newBuilder().also {
            it.v2Builder.keyBundle = keys.getPublicKeyBundle()
            it.v2Builder.keyBundleBuilder.identityKeyBuilder.signature = it.v2.keyBundle.identityKey.signature.ensureWalletSignature()
        }.build()
        val envelope = MessageApiOuterClass.Envelope.newBuilder().apply {
            contentTopic = Topic.contact(address).description
            timestampNs = Date().time * 1_000_000
            message = contactBundle.toByteString()
        }.build()
        envelopes.add(envelope)
        runBlocking { publish(envelopes = envelopes) }
    }

    fun getUserContact(peerAddress: String): ContactBundle? {
        return contacts.find(Keys.toChecksumAddress(peerAddress))
    }

    suspend fun query(topics: List<Topic>, pagination: Pagination? = null): QueryResponse {
        return apiClient.queryTopics(topics = topics, pagination = pagination)
    }

    suspend fun subscribe(topics: List<String>): Flow<Envelope> {
        return apiClient.subscribe(topics = topics)
    }

    suspend fun subscribeTopic(topics: List<Topic>): Flow<Envelope> {
        return subscribe(topics.map { it.description })
    }

    fun fetchConversation(topic: String?): Conversation? {
        if (topic.isNullOrBlank()) return null
        return conversations.list().firstOrNull { it.topic == topic }
    }

    fun publish(envelopes: List<Envelope>): PublishResponse {
        val authorized = AuthorizedIdentity(
            address = address,
            authorized = privateKeyBundleV1.identityKey.publicKey,
            identity = privateKeyBundleV1.identityKey
        )
        val authToken = authorized.createAuthToken()
        apiClient.setAuthToken(authToken)

        return runBlocking { apiClient.publish(envelopes = envelopes) }
    }

    fun ensureUserContactPublished() {
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
            ConversationV2Export::class.java
        )
        try {
            return importV2Conversation(export = v2Export)
        } catch (e: java.lang.Exception) {
            val v1Export = gson.fromJson(
                conversationData.toString(StandardCharsets.UTF_8),
                ConversationV1Export::class.java
            )
            try {
                return importV1Conversation(export = v1Export)
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
                    metadata = export.context?.metadata ?: mapOf()
                ),
                peerAddress = export.peerAddress,
                client = this,
                header = SealedInvitationHeaderV1.newBuilder().build()
            )
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
                sentAt = sentAt
            )
        )
    }

    fun canMessage(peerAddress: String): Boolean {
        return runBlocking { query(listOf(Topic.contact(peerAddress))).envelopesList.size > 0 }
    }

    val privateKeyBundle: PrivateKeyBundle
        get() = PrivateKeyBundleBuilder.buildFromV1Key(privateKeyBundleV1)

    val v1keys: PrivateKeyBundleV1
        get() = privateKeyBundleV1

    val keys: PrivateKeyBundleV2
        get() = privateKeyBundleV1.toV2()
}
