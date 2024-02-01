package org.xmtp.android.library

import kotlinx.coroutines.runBlocking
import org.xmtp.android.library.codecs.ContentCodec
import org.xmtp.android.library.codecs.EncodedContent
import org.xmtp.android.library.codecs.compress
import org.xmtp.android.library.libxmtp.Message
import org.xmtp.android.library.messages.DecryptedMessage
import org.xmtp.android.library.messages.PagingInfoSortDirection
import org.xmtp.proto.message.api.v1.MessageApiOuterClass
import uniffi.xmtpv3.FfiGroup
import uniffi.xmtpv3.FfiListMessagesOptions
import java.util.Date
import kotlin.time.Duration.Companion.nanoseconds
import kotlin.time.DurationUnit

class Group(val client: Client, private val libXMTPGroup: FfiGroup) {
    val id: ByteArray
        get() = libXMTPGroup.id()

    val createdAt: Date
        get() = Date(libXMTPGroup.createdAtNs() / 1_000_000)

    fun send(text: String): String {
        return send(prepareMessage(content = text, options = null))
    }

    fun <T> send(content: T, options: SendOptions? = null): String {
        val preparedMessage = prepareMessage(content = content, options = options)
        return send(preparedMessage)
    }

    fun send(encodedContent: EncodedContent): String {
        runBlocking {
            libXMTPGroup.send(contentBytes = encodedContent.toByteArray())
        }
        return id.toString()
    }

    fun <T> prepareMessage(content: T, options: SendOptions?): EncodedContent {
        val codec = Client.codecRegistry.find(options?.contentType)

        fun <Codec : ContentCodec<T>> encode(codec: Codec, content: Any?): EncodedContent {
            val contentType = content as? T
            if (contentType != null) {
                return codec.encode(contentType)
            } else {
                throw XMTPException("Codec type is not registered")
            }
        }

        var encoded = encode(codec = codec as ContentCodec<T>, content = content)
        val fallback = codec.fallback(content)
        if (!fallback.isNullOrBlank()) {
            encoded = encoded.toBuilder().also {
                it.fallback = fallback
            }.build()
        }
        val compression = options?.compression
        if (compression != null) {
            encoded = encoded.compress(compression)
        }
        return encoded
    }

    suspend fun sync() {
        libXMTPGroup.sync()
    }

    fun messages(
        limit: Int? = null,
        before: Date? = null,
        after: Date? = null,
        direction: PagingInfoSortDirection = MessageApiOuterClass.SortDirection.SORT_DIRECTION_DESCENDING,
    ): List<DecodedMessage> {
        return runBlocking {
            val messages = libXMTPGroup.findMessages(
                opts = FfiListMessagesOptions(
                    sentBeforeNs = before?.time?.nanoseconds?.toLong(DurationUnit.NANOSECONDS),
                    sentAfterNs = after?.time?.nanoseconds?.toLong(DurationUnit.NANOSECONDS),
                    limit = limit?.toLong()
                )
            ).map {
                Message(client, it).decode()
            }
            when (direction) {
                MessageApiOuterClass.SortDirection.SORT_DIRECTION_ASCENDING -> messages
                else -> messages.reversed()
            }
        }
    }

    fun decryptedMessages(
        limit: Int? = null,
        before: Date? = null,
        after: Date? = null,
        direction: PagingInfoSortDirection = MessageApiOuterClass.SortDirection.SORT_DIRECTION_DESCENDING,
    ): List<DecryptedMessage> {
        return runBlocking {
            val messages = libXMTPGroup.findMessages(
                opts = FfiListMessagesOptions(
                    sentBeforeNs = before?.time?.nanoseconds?.toLong(DurationUnit.NANOSECONDS),
                    sentAfterNs = after?.time?.nanoseconds?.toLong(DurationUnit.NANOSECONDS),
                    limit = limit?.toLong()
                )
            ).map {
                decrypt(Message(client, it))
            }
            when (direction) {
                MessageApiOuterClass.SortDirection.SORT_DIRECTION_ASCENDING -> messages
                else -> messages.reversed()
            }
        }
    }

    fun decrypt(message: Message): DecryptedMessage {
        return DecryptedMessage(
            id = message.id.toHex(),
            topic = message.id.toHex(),
            encodedContent = message.decode().encodedContent,
            senderAddress = message.senderAddress,
            sentAt = Date()
        )
    }

    fun addMembers(addresses: List<String>) {
        runBlocking { libXMTPGroup.addMembers(addresses) }
    }

    fun removeMembers(addresses: List<String>) {
        runBlocking { libXMTPGroup.removeMembers(addresses) }
    }

    fun memberAddresses(): List<String> {
        return runBlocking {
            libXMTPGroup.listMembers().map { it.accountAddress }
        }
    }
}
