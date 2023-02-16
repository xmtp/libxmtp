package org.xmtp.android.library

data class ConversationV1Export(
    var version: String,
    var peerAddress: String,
    var createdAt: String,
)

data class ConversationV2Export(
    var version: String,
    var topic: String,
    var keyMaterial: String,
    var peerAddress: String,
    var createdAt: String,
    var context: ConversationV2ContextExport? = null,
)

data class ConversationV2ContextExport(
    var conversationId: String,
    var metadata: Map<String, String>,
)
