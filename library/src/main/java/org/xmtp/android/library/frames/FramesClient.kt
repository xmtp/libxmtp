package org.xmtp.android.library.frames

import android.util.Base64
import com.google.protobuf.kotlin.toByteString
import org.xmtp.android.library.Client
import org.xmtp.android.library.XMTPException
import org.xmtp.android.library.frames.FramesConstants.PROTOCOL_VERSION
import org.xmtp.android.library.hexToByteArray
import org.xmtp.android.library.toHex
import org.xmtp.proto.message.contents.Frames.FrameAction
import org.xmtp.proto.message.contents.Frames.FrameActionBody
import java.security.MessageDigest
import java.util.Date

class FramesClient(private val xmtpClient: Client, var proxy: OpenFramesProxy = OpenFramesProxy()) {

    suspend fun signFrameAction(inputs: FrameActionInputs): FramePostPayload {
        val opaqueConversationIdentifier = buildOpaqueIdentifier(inputs)
        val frameUrl = inputs.frameUrl
        val buttonIndex = inputs.buttonIndex
        val inputText = inputs.inputText
        val state = inputs.state
        val now = Date().time / 1_000
        val frameActionBuilder = FrameActionBody.newBuilder().also { frame ->
            frame.frameUrl = frameUrl
            frame.buttonIndex = buttonIndex
            frame.opaqueConversationIdentifier = opaqueConversationIdentifier
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

        val untrustedData = FramePostUntrustedData(
            frameUrl,
            now,
            buttonIndex,
            inputText,
            state,
            xmtpClient.address,
            opaqueConversationIdentifier,
            now.toInt()
        )
        val trustedData = FramePostTrustedData(signedAction)

        return FramePostPayload("xmtp@$PROTOCOL_VERSION", untrustedData, trustedData)
    }

    private fun signDigest(message: String): ByteArray {
        return xmtpClient.signWithInstallationKey(message)
    }

    private fun buildSignedFrameAction(actionBodyInputs: FrameActionBody): ByteArray {
        val digest = sha256(actionBodyInputs.toByteArray()).toHex()
        val signature = signDigest(digest)

        val frameAction = FrameAction.newBuilder().also {
            it.actionBody = actionBodyInputs.toByteString()
            it.installationSignature = signature.toByteString()
            it.installationId = xmtpClient.installationId.hexToByteArray().toByteString()
            it.inboxId = xmtpClient.inboxId
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
                val conversationTopic =
                    dmInputs.conversationTopic ?: throw XMTPException("No conversation topic")
                val combined =
                    conversationTopic.lowercase() + dmInputs.participantAccountAddresses.map { it.lowercase() }
                        .sorted().joinToString("")
                val digest = sha256(combined.toByteArray())
                Base64.encodeToString(digest, Base64.NO_WRAP)
            }
        }
    }

    private fun sha256(input: ByteArray): ByteArray {
        val digest = MessageDigest.getInstance("SHA-256")
        return digest.digest(input)
    }
}
