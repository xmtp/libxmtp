package org.xmtp.android.library.codecs

import com.google.protobuf.ByteString
import com.google.protobuf.kotlin.toByteString
import com.google.protobuf.kotlin.toByteStringUtf8
import org.web3j.crypto.Hash
import org.web3j.utils.Numeric
import org.xmtp.android.library.Crypto
import org.xmtp.android.library.XMTPException
import org.xmtp.android.library.toHex
import org.xmtp.proto.message.contents.CiphertextOuterClass.Ciphertext
import java.net.URI
import java.net.URL
import java.security.SecureRandom

data class EncryptedEncodedContent(
    val contentDigest: String,
    val secret: ByteString,
    val salt: ByteString,
    val nonce: ByteString,
    val payload: ByteString,
    val contentLength: Int? = null,
    val filename: String? = null,
)

data class RemoteAttachment(
    val url: URL,
    val contentDigest: String,
    val secret: ByteString,
    val salt: ByteString,
    val nonce: ByteString,
    val scheme: String,
    var contentLength: Int? = null,
    var filename: String? = null,
    var fetcher: Fetcher = HTTPFetcher(),
) {
    fun <T> load(): T? {
        val payload = fetcher.fetch(url)

        if (payload.isEmpty()) {
            throw XMTPException("no remote attachment payload")
        }

        val encrypted = EncryptedEncodedContent(
            contentDigest,
            secret,
            salt,
            nonce,
            payload.toByteString(),
            contentLength,
            filename,
        )

        val decrypted = decryptEncoded(encrypted)

        return decrypted.decoded<T>()
    }

    companion object {
        fun decryptEncoded(encrypted: EncryptedEncodedContent): EncodedContent {
            if (Hash.sha256(encrypted.payload.toByteArray()).toHex() != encrypted.contentDigest) {
                throw XMTPException("contentDigest does not match")
            }

            val aes = Ciphertext.Aes256gcmHkdfsha256.newBuilder().also {
                it.hkdfSalt = encrypted.salt
                it.gcmNonce = encrypted.nonce
                it.payload = encrypted.payload
            }.build()

            val ciphertext = Ciphertext.newBuilder().also {
                it.aes256GcmHkdfSha256 = aes
            }.build()

            val decrypted = Crypto.decrypt(encrypted.secret.toByteArray(), ciphertext)

            return EncodedContent.parseFrom(decrypted)
        }

        fun <T> encodeEncrypted(content: T, codec: ContentCodec<T>): EncryptedEncodedContent {
            val secret = SecureRandom().generateSeed(32)
            val encodedContent = codec.encode(content).toByteArray()
            val ciphertext = Crypto.encrypt(secret, encodedContent) ?: throw XMTPException("ciphertext not created")
            val contentDigest = Hash.sha256(ciphertext.aes256GcmHkdfSha256.payload.toByteArray()).toHex()
            return EncryptedEncodedContent(
                contentDigest = contentDigest,
                secret = secret.toByteString(),
                salt = ciphertext.aes256GcmHkdfSha256.hkdfSalt,
                nonce = ciphertext.aes256GcmHkdfSha256.gcmNonce,
                payload = ciphertext.aes256GcmHkdfSha256.payload,
                contentLength = null,
                filename = null,
            )
        }

        fun from(url: URL, encryptedEncodedContent: EncryptedEncodedContent): RemoteAttachment {
            if (URI(url.toString()).scheme != "https") {
                throw XMTPException("scheme must be https://")
            }

            return RemoteAttachment(
                url = url,
                contentDigest = encryptedEncodedContent.contentDigest,
                secret = encryptedEncodedContent.secret,
                salt = encryptedEncodedContent.salt,
                nonce = encryptedEncodedContent.nonce,
                scheme = URI(url.toString()).scheme,
            )
        }
    }
}

val ContentTypeRemoteAttachment = ContentTypeIdBuilder.builderFromAuthorityId(
    "xmtp.org",
    "remoteStaticAttachment",
    versionMajor = 1,
    versionMinor = 0
)

interface Fetcher {
    fun fetch(url: URL): ByteArray
}

class HTTPFetcher : Fetcher {
    override fun fetch(url: URL): ByteArray {
        return url.readBytes()
    }
}

data class RemoteAttachmentCodec(override var contentType: ContentTypeId = ContentTypeRemoteAttachment) : ContentCodec<RemoteAttachment> {
    override fun encode(content: RemoteAttachment): EncodedContent {
        return EncodedContent.newBuilder().also {
            it.type = ContentTypeRemoteAttachment
            it.putAllParameters(
                mapOf(
                    "contentDigest" to content.contentDigest,
                    "secret" to content.secret.toByteArray().toHex(),
                    "salt" to content.salt.toByteArray().toHex(),
                    "nonce" to content.nonce.toByteArray().toHex(),
                    "scheme" to content.scheme,
                    "contentLength" to content.contentLength.toString(),
                    "filename" to content.filename,
                )
            )
            it.content = content.url.toString().toByteStringUtf8()
        }.build()
    }

    override fun decode(content: EncodedContent): RemoteAttachment {
        val contentDigest = content.parametersMap["contentDigest"] ?: throw XMTPException("missing content digest")
        val secret = content.parametersMap["secret"] ?: throw XMTPException("missing secret")
        val salt = content.parametersMap["salt"] ?: throw XMTPException("missing salt")
        val nonce = content.parametersMap["nonce"] ?: throw XMTPException("missing nonce")
        val scheme = content.parametersMap["scheme"] ?: throw XMTPException("missing scheme")
        val contentLength = content.parametersMap["contentLength"] ?: throw XMTPException("missing contentLength")
        val filename = content.parametersMap["filename"] ?: throw XMTPException("missing filename")
        val encodedContent = content.content ?: throw XMTPException("missing content")

        return RemoteAttachment(
            url = URL(encodedContent.toStringUtf8()),
            contentDigest = contentDigest,
            secret = Numeric.hexStringToByteArray(secret).toByteString(),
            salt = Numeric.hexStringToByteArray(salt).toByteString(),
            nonce = Numeric.hexStringToByteArray(nonce).toByteString(),
            scheme = scheme,
            contentLength = contentLength.toInt(),
            filename = filename,
        )
    }
}
