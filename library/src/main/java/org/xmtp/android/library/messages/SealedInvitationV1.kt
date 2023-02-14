package org.xmtp.android.library.messages

import com.google.protobuf.kotlin.toByteString
import org.xmtp.android.library.CipherText
import org.xmtp.android.library.Crypto
import org.xmtp.android.library.XMTPException

typealias SealedInvitationV1 = org.xmtp.proto.message.contents.Invitation.SealedInvitationV1

class SealedInvitationV1Builder {
    companion object {
        fun buildFromHeader(headerBytes: ByteArray, ciphtertext: CipherText): SealedInvitationV1 {
            return SealedInvitationV1.newBuilder().also {
                it.headerBytes = headerBytes.toByteString()
                it.ciphertext = ciphtertext
            }.build()
        }
    }
}

val SealedInvitationV1.header: SealedInvitationHeaderV1
    get() = SealedInvitationHeaderV1.parseFrom(headerBytes)

fun SealedInvitationV1.getInvitation(viewer: PrivateKeyBundleV2?): InvitationV1 {
    val header = header
    if (!header.sender.identityKey.hasSignature()) {
        throw XMTPException("No signature")
    }
    val secret = if (viewer != null && viewer.identityKey.matches(header.sender.identityKey)) {
        viewer.sharedSecret(
            peer = header.recipient,
            myPreKey = header.sender.preKey,
            isRecipient = false
        )
    } else {
        viewer?.sharedSecret(
            peer = header.sender,
            myPreKey = header.recipient.preKey,
            isRecipient = true
        ) ?: byteArrayOf()
    }
    val decryptedBytes =
        Crypto.decrypt(secret, ciphertext, additionalData = headerBytes.toByteArray())
    return InvitationV1.parseFrom(decryptedBytes)
}
