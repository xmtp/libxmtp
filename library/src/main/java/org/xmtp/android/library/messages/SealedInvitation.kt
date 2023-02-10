package org.xmtp.android.library.messages

import com.google.protobuf.kotlin.toByteString
import org.xmtp.android.library.CipherText
import org.xmtp.android.library.Crypto
import java.util.Date

typealias SealedInvitation = org.xmtp.proto.message.contents.Invitation.SealedInvitation

class SealedInvitationBuilder {
    companion object {
        fun buildFromV1(
            sender: PrivateKeyBundleV2,
            recipient: SignedPublicKeyBundle,
            created: Date,
            invitation: InvitationV1
        ): SealedInvitation {
            val header = SealedInvitationHeaderV1Builder.buildFromSignedPublicBundle(
                sender.getPublicKeyBundle(),
                recipient,
                (created.time * 1_000_000)
            )
            val secret = sender.sharedSecret(
                peer = recipient,
                myPreKey = sender.preKeysList[0].publicKey,
                isRecipient = false
            )
            val headerBytes = header.toByteArray()
            val invitationBytes = invitation.toByteArray()
            val ciphertext = Crypto.encrypt(secret, invitationBytes, additionalData = headerBytes)
            return buildFromCipherText(headerBytes, ciphertext)
        }

        fun buildFromCipherText(headerBytes: ByteArray, ciphertext: CipherText?): SealedInvitation {
            return SealedInvitation.newBuilder().apply {
                v1Builder.headerBytes = headerBytes.toByteString()
                v1Builder.ciphertext = ciphertext
            }.build()
        }
    }
}

fun SealedInvitation.involves(contact: ContactBundle): Boolean {
    val contactSignedPublicKeyBundle = contact.toSignedPublicKeyBundle()
    return v1.header.recipient.equals(contactSignedPublicKeyBundle) || v1.header.sender.equals(
        contactSignedPublicKeyBundle
    )
}
