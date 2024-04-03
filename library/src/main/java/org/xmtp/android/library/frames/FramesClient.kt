package org.xmtp.android.library.frames

import android.util.Base64
import org.xmtp.android.library.Client
import org.xmtp.android.library.XMTPException
import org.xmtp.android.library.frames.FramesConstants.PROTOCOL_VERSION
import org.xmtp.android.library.messages.PrivateKeyBuilder
import org.xmtp.android.library.messages.Signature
import org.xmtp.android.library.messages.getPublicKeyBundle
import org.xmtp.proto.message.contents.PublicKeyOuterClass.SignedPublicKeyBundle
import java.security.MessageDigest
import org.xmtp.proto.message.contents.Frames.FrameActionBody
import org.xmtp.proto.message.contents.Frames.FrameAction
import java.util.Date

class FramesClient(private val xmtpClient: Client, var proxy: OpenFramesProxy = OpenFramesProxy()) {

    suspend fun signFrameAction(inputs: FrameActionInputs): FramePostPayload {
        val opaqueConversationIdentifier = buildOpaqueIdentifier(inputs)
        val frameUrl = inputs.frameUrl
        val buttonIndex = inputs.buttonIndex
        val inputText = inputs.inputText
        val state = inputs.state
        val now = Date().time * 1_000_000
        val frameActionBuilder = FrameActionBody.newBuilder().also { frame ->
            frame.frameUrl = frameUrl
            frame.buttonIndex = buttonIndex
            frame.opaqueConversationIdentifier = opaqueConversationIdentifier
            frame.timestamp = now
            frame.unixTimestamp = now.toInt()
            if (inputText != null) {
                frame.inputText = inputText
            }
            if (state != null) {
                frame.state = state
            }
        }

        val toSign = frameActionBuilder.build()
        val signedAction = Base64.encodeToString(buildSignedFrameAction(toSign), Base64.NO_WRAP)

        val untrustedData = FramePostUntrustedData(frameUrl, now, buttonIndex, inputText, state, xmtpClient.address, opaqueConversationIdentifier, now.toInt())
        val trustedData = FramePostTrustedData(signedAction)

        return FramePostPayload("xmtp@$PROTOCOL_VERSION", untrustedData, trustedData)
    }

    private suspend fun signDigest(digest: ByteArray): Signature {
        val signedPrivateKey = xmtpClient.keys.identityKey
        val privateKey = PrivateKeyBuilder.buildFromSignedPrivateKey(signedPrivateKey)
        return PrivateKeyBuilder(privateKey).sign(digest)
    }

    private fun getPublicKeyBundle(): SignedPublicKeyBundle {
        return xmtpClient.keys.getPublicKeyBundle()
    }

    private suspend fun buildSignedFrameAction(actionBodyInputs: FrameActionBody): ByteArray {
        val digest = sha256(actionBodyInputs.toByteArray())
        val signature = signDigest(digest)

        val publicKeyBundle = getPublicKeyBundle()
        val frameAction = FrameAction.newBuilder().also {
            it.actionBody = actionBodyInputs.toByteString()
            it.signature = signature
            it.signedPublicKeyBundle = publicKeyBundle
        }.build()

        return frameAction.toByteArray()
    }

    private fun buildOpaqueIdentifier(inputs: FrameActionInputs): String {
        return when (inputs.conversationInputs) {
            is ConversationActionInputs.Group -> {
                val groupInputs = inputs.conversationInputs.inputs
                val combined = groupInputs.groupId + groupInputs.groupSecret
                val digest = sha256(combined)
                Base64.encodeToString(digest, Base64.NO_WRAP)
            }
            is ConversationActionInputs.Dm -> {
                val dmInputs = inputs.conversationInputs.inputs
                val conversationTopic = dmInputs.conversationTopic ?: throw XMTPException("No conversation topic")
                val combined = (conversationTopic.lowercase() + dmInputs.participantAccountAddresses.map { it.lowercase() }.sorted().joinToString("")).toByteArray()
                val digest = sha256(combined)
                Base64.encodeToString(digest, Base64.NO_WRAP)
            }
        }
    }

    private fun sha256(input: ByteArray): ByteArray {
        val digest = MessageDigest.getInstance("SHA-256")
        return digest.digest(input)
    }
}
