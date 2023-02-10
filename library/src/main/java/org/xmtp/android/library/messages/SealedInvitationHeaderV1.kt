package org.xmtp.android.library.messages

typealias SealedInvitationHeaderV1 = org.xmtp.proto.message.contents.Invitation.SealedInvitationHeaderV1

class SealedInvitationHeaderV1Builder {
    companion object {
        fun buildFromSignedPublicBundle(
            sender: SignedPublicKeyBundle,
            recipient: SignedPublicKeyBundle,
            createdNs: Long
        ): SealedInvitationHeaderV1 {
            return SealedInvitationHeaderV1.newBuilder().also {
                it.sender = sender
                it.recipient = recipient
                it.createdNs = createdNs
            }.build()
        }
    }
}
