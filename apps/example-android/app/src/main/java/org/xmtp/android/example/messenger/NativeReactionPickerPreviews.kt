package org.xmtp.android.example.messenger

import android.content.res.Configuration
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.tooling.preview.PreviewParameter
import androidx.compose.ui.tooling.preview.PreviewParameterProvider
import androidx.compose.ui.unit.LayoutDirection
import org.xmtp.android.example.shared.*
import org.xmtp.android.example.shared.feature.chat.ChatFixtures
import org.xmtp.android.example.shared.feature.conversations.ConversationsFixtures

class NativeEmojiQueryPreviewProvider : PreviewParameterProvider<String> {
    override val values = sequenceOf("", "smile", "flag", "skin tone")
}

/** Fixed Android picker states. No session, AppGraph, or SDK object is created. */
@Preview(name = "Native emoji categories and search", widthDp = 360, heightDp = 780, showBackground = true)
@Preview(name = "Native emoji dark", widthDp = 320, heightDp = 680, uiMode = Configuration.UI_MODE_NIGHT_YES, showBackground = true)
@Preview(name = "Native emoji large text", widthDp = 320, heightDp = 680, fontScale = 2f, showBackground = true)
@Preview(name = "Native emoji landscape", widthDp = 760, heightDp = 400, showBackground = true)
@Composable
fun NativeReactionPickerPreview(@PreviewParameter(NativeEmojiQueryPreviewProvider::class) query: String) {
    NativeEmojiPreviewContent(query)
}

@Preview(name = "Native emoji RTL", widthDp = 320, heightDp = 680, fontScale = 2f, locale = "ar", uiMode = Configuration.UI_MODE_NIGHT_YES, showBackground = true)
@Composable
fun NativeReactionPickerRtlPreview(@PreviewParameter(NativeEmojiQueryPreviewProvider::class) query: String) {
    CompositionLocalProvider(LocalLayoutDirection provides LayoutDirection.Rtl) { NativeEmojiPreviewContent(query) }
}

@Composable
private fun NativeEmojiPreviewContent(query: String) {
    val chat = ChatFixtures.emoji.copy(overlay = ChatFixtures.emoji.overlay?.copy(emojiQuery = query))
    val state = MessengerState(
        screen = Screen.TIMELINE,
        inbox = "sample-own-inbox",
        conversationId = chat.conversationId,
        conversationTitle = chat.title,
        messages = chat.messages,
        shellState = ShellState(AppRoute.Chat(chat.conversationId.orEmpty()), reducedMotion = true),
        chatState = chat,
        conversationsState = ConversationsFixtures.populated,
    )
    ReactionPickerHost(state, {}) { action -> MessengerScreens(state, action) }
}
