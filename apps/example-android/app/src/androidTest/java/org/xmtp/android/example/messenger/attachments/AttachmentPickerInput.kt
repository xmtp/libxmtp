package org.xmtp.android.example.messenger.attachments

import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.AndroidComposeTestRule
import androidx.lifecycle.Lifecycle
import org.xmtp.android.example.MainActivity

/** Result delivery and Compose idleness can precede Android window teardown after cancel or Back. */
internal fun AndroidComposeTestRule<*, MainActivity>.openAttachmentChoices(timeoutMillis: Long = 30_000) {
    waitUntil(timeoutMillis) {
        val sheetRemoved = onAllNodesWithTag("attachment-choice-sheet").fetchSemanticsNodes().isEmpty()
        val windowReady =
            runOnUiThread {
                activity.lifecycle.currentState == Lifecycle.State.RESUMED && activity.hasWindowFocus()
            }
        sheetRemoved && windowReady
    }
    onNodeWithContentDescription("Add attachment").assertIsDisplayed().assertIsEnabled().performClick()
}
