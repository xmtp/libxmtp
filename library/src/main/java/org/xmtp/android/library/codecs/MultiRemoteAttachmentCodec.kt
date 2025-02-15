package org.xmtp.android.library.codecs

import com.google.protobuf.ByteString
import org.xmtp.android.library.XMTPException
import org.xmtp.android.library.codecs.RemoteAttachment.Companion.decryptEncoded
import org.xmtp.android.library.codecs.RemoteAttachment.Companion.encodeEncryptedBytes
import uniffi.xmtpv3.FfiMultiRemoteAttachment
import uniffi.xmtpv3.FfiRemoteAttachmentInfo
import uniffi.xmtpv3.decodeMultiRemoteAttachment
import uniffi.xmtpv3.encodeMultiRemoteAttachment
import java.net.URI
import java.net.URL

val ContentTypeMultiRemoteAttachment = ContentTypeIdBuilder.builderFromAuthorityId(
    "xmtp.org",
    "multiRemoteStaticAttachment",
    versionMajor = 1,
    versionMinor = 0,
)

data class MultiRemoteAttachment(
    val remoteAttachments: List<RemoteAttachmentInfo>
)

data class RemoteAttachmentInfo(
    val url: String,
    val filename: String,
    val contentLength: Long,
    val contentDigest: String,
    val nonce: ByteString,
    val scheme: String,
    val salt: ByteString,
    val secret: ByteString
) {
    companion object {
        fun from(url: URL, encryptedEncodedContent: EncryptedEncodedContent): RemoteAttachmentInfo {
            if (URI(url.toString()).scheme != "https") {
                throw XMTPException("scheme must be https://")
            }

            return RemoteAttachmentInfo(
                url = url.toString(),
                contentDigest = encryptedEncodedContent.contentDigest,
                secret = encryptedEncodedContent.secret,
                salt = encryptedEncodedContent.salt,
                nonce = encryptedEncodedContent.nonce,
                scheme = URI(url.toString()).scheme,
                contentLength = encryptedEncodedContent.contentLength?.toLong() ?: 0,
                filename = encryptedEncodedContent.filename ?: ""
            )
        }
    }
}

data class MultiRemoteAttachmentCodec(override var contentType: ContentTypeId = ContentTypeMultiRemoteAttachment) :
    ContentCodec<MultiRemoteAttachment> {

    override fun encode(content: MultiRemoteAttachment): EncodedContent {
        val ffiMultiRemoteAttachment = FfiMultiRemoteAttachment(
            attachments = content.remoteAttachments.map { attachment ->
                FfiRemoteAttachmentInfo(
                    url = attachment.url,
                    filename = attachment.filename,
                    contentDigest = attachment.contentDigest,
                    nonce = attachment.nonce.toByteArray(),
                    scheme = attachment.scheme,
                    salt = attachment.salt.toByteArray(),
                    secret = attachment.secret.toByteArray(),
                    contentLength = attachment.contentLength.toUInt(),
                )
            }
        )
        return EncodedContent.parseFrom(encodeMultiRemoteAttachment(ffiMultiRemoteAttachment))
    }

    override fun decode(content: EncodedContent): MultiRemoteAttachment {
        val ffiMultiRemoteAttachment = decodeMultiRemoteAttachment(content.toByteArray())
        return MultiRemoteAttachment(
            remoteAttachments = ffiMultiRemoteAttachment.attachments.map { attachment ->
                RemoteAttachmentInfo(
                    url = attachment.url,
                    filename = attachment.filename ?: "",
                    contentLength = attachment.contentLength?.toLong() ?: 0,
                    contentDigest = attachment.contentDigest,
                    nonce = attachment.nonce.toProtoByteString(),
                    scheme = attachment.scheme,
                    salt = attachment.salt.toProtoByteString(),
                    secret = attachment.secret.toProtoByteString(),
                )
            }
        )
    }

    override fun fallback(content: MultiRemoteAttachment): String = "MultiRemoteAttachment not supported"

    override fun shouldPush(content: MultiRemoteAttachment): Boolean = true

    companion object {

        fun encryptBytesForLocalAttachment(bytesToEncrypt: ByteArray, filename: String): EncryptedEncodedContent {
            return encodeEncryptedBytes(bytesToEncrypt, filename)
        }

        fun buildRemoteAttachmentInfo(encryptedAttachment: EncryptedEncodedContent, remoteUrl: URL): RemoteAttachmentInfo {
            return RemoteAttachmentInfo.from(remoteUrl, encryptedAttachment)
        }

        fun buildEncryptAttachmentResult(remoteAttachment: RemoteAttachment, encryptedPayload: ByteArray): EncryptedEncodedContent {
            return EncryptedEncodedContent(
                remoteAttachment.contentDigest,
                remoteAttachment.secret,
                remoteAttachment.salt,
                remoteAttachment.nonce,
                encryptedPayload.toProtoByteString(),
                remoteAttachment.contentLength,
                remoteAttachment.filename,
            )
        }

        fun decryptAttachment(encryptedAttachment: EncryptedEncodedContent): EncodedContent {
            val decrypted = decryptEncoded(encryptedAttachment)

            return decrypted
        }
    }
}

private fun ByteArray.toProtoByteString(): ByteString {
    return ByteString.copyFrom(this)
}
