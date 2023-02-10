package org.xmtp.android.library.messages

import com.google.protobuf.kotlin.toByteString

typealias SignedContent = org.xmtp.proto.message.contents.Content.SignedContent

class SignedContentBuilder {
    companion object {
        fun builderFromPayload(
            payload: ByteArray,
            sender: SignedPublicKeyBundle?,
            signature: Signature?
        ): SignedContent {
            return SignedContent.newBuilder().also {
                it.payload = payload.toByteString()
                it.sender = sender
                it.signature = signature
            }.build()
        }
    }
}
