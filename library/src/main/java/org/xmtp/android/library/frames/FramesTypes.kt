package org.xmtp.android.library.frames

typealias AcceptedFrameClients = Map<String, String>

sealed class OpenFrameButton {
    abstract val target: String?
    abstract val label: String

    data class Link(override val target: String, override val label: String) : OpenFrameButton()

    data class Mint(override val target: String, override val label: String) : OpenFrameButton()

    data class Post(override val target: String?, override val label: String) : OpenFrameButton()

    data class PostRedirect(override val target: String?, override val label: String) : OpenFrameButton()
}

data class OpenFrameImage(
    val content: String,
    val aspectRatio: AspectRatio?,
    val alt: String?
)

enum class AspectRatio(val ratio: String) {
    RATIO_1_91_1("1.91.1"),
    RATIO_1_1("1:1")
}

data class TextInput(val content: String)

data class OpenFrameResult(
    val acceptedClients: AcceptedFrameClients,
    val image: OpenFrameImage,
    val postUrl: String?,
    val textInput: TextInput?,
    val buttons: Map<String, OpenFrameButton>?,
    val ogImage: String,
    val state: String?
)

data class GetMetadataResponse(
    val url: String,
    val extractedTags: Map<String, String>
)

data class PostRedirectResponse(
    val originalUrl: String,
    val redirectedTo: String
)

data class OpenFramesUntrustedData(
    val url: String,
    val timestamp: Int,
    val buttonIndex: Int,
    val inputText: String?,
    val state: String?
)

typealias FramesApiRedirectResponse = PostRedirectResponse

data class FramePostUntrustedData(
    val url: String,
    val timestamp: Long,
    val buttonIndex: Int,
    val inputText: String?,
    val state: String?,
    val walletAddress: String,
    val opaqueConversationIdentifier: String,
    val unixTimestamp: Int
)

data class FramePostTrustedData(
    val messageBytes: String
)

data class FramePostPayload(
    val clientProtocol: String,
    val untrustedData: FramePostUntrustedData,
    val trustedData: FramePostTrustedData
)

data class DmActionInputs(
    val conversationTopic: String?,
    val participantAccountAddresses: List<String>
)

data class GroupActionInputs(
    val groupId: ByteArray,
    val groupSecret: ByteArray
)

sealed class ConversationActionInputs {
    data class Dm(val inputs: DmActionInputs) : ConversationActionInputs()
    data class Group(val inputs: GroupActionInputs) : ConversationActionInputs()
}

data class FrameActionInputs(
    val frameUrl: String,
    val buttonIndex: Int,
    val inputText: String?,
    val state: String?,
    val conversationInputs: ConversationActionInputs
)
