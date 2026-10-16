package org.xmtp.android.example.messenger.attachments

import android.app.Dialog
import android.widget.TextView
import androidx.activity.compose.setContent
import androidx.compose.material3.Button
import androidx.compose.material3.Text
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.rules.RuleChain
import org.junit.rules.TestRule
import org.xmtp.android.example.MainActivity
import org.xmtp.android.example.messenger.TestAppGraphRule
import org.xmtp.android.example.shared.Appearance
import org.xmtp.android.example.shared.designsystem.MessengerTheme
import org.xmtp.android.example.shared.feature.attachments.AttachmentChoiceSheet
import org.xmtp.android.example.shared.feature.attachments.AttachmentChoiceState
import java.util.concurrent.atomic.AtomicBoolean

class AttachmentPickerInputInstrumentedTest {
    private val graphRule = TestAppGraphRule()
    val compose = createAndroidComposeRule<MainActivity>()

    @get:Rule val rules: TestRule = RuleChain.outerRule(graphRule).around(compose)

    private fun assertInputBlocked() {
        // These were the old test's readiness checks. They pass underneath a modal window.
        compose.onNodeWithContentDescription("Add attachment").assertIsDisplayed().assertIsEnabled()
        assertThrows(ComposeTimeoutException::class.java) {
            compose.openAttachmentChoices(timeoutMillis = 100)
        }
    }

    @Test fun pickerActionWaitsForTheDismissedSheetToLeaveComposition() {
        val mounted = mutableStateOf(true)
        val visible = mutableStateOf(true)
        val hidden = AtomicBoolean()
        var clicks = 0
        compose.runOnUiThread {
            compose.activity.setContent {
                MessengerTheme(Appearance.SYSTEM, reducedMotion = false) {
                    Button(onClick = { clicks++ }, Modifier.semantics { contentDescription = "Add attachment" }) {
                        Text("Add attachment")
                    }
                    if (mounted.value) {
                        AttachmentChoiceSheet(
                            AttachmentChoiceState(1, "picker-input"),
                            visible.value,
                            // Hold disposal after hide, like a delayed host recomposition.
                            onHidden = { hidden.set(true) },
                            action = {},
                        )
                    }
                }
            }
        }
        compose.onNodeWithTag("attachment-choice-sheet").assertIsDisplayed()
        compose.runOnIdle { visible.value = false }
        compose.waitUntil(30_000) { hidden.get() }
        assertInputBlocked()
        compose.runOnIdle { assertEquals("No picker action before sheet disposal", 0, clicks) }
        compose.runOnIdle { mounted.value = false }
        compose.openAttachmentChoices()
        compose.runOnIdle { assertEquals(1, clicks) }
    }

    @Test fun pickerActionWaitsForItsWindowToRegainFocus() {
        var clicks = 0
        compose.runOnUiThread {
            compose.activity.setContent {
                MessengerTheme(Appearance.SYSTEM, reducedMotion = false) {
                    Button(onClick = { clicks++ }, Modifier.semantics { contentDescription = "Add attachment" }) {
                        Text("Add attachment")
                    }
                }
            }
        }
        compose.waitUntil(30_000) { compose.runOnUiThread { compose.activity.hasWindowFocus() } }
        val picker =
            compose.runOnUiThread {
                Dialog(compose.activity).apply {
                    setContentView(TextView(context).apply { text = "Returning picker window" })
                    show()
                }
            }
        try {
            compose.waitUntil(30_000) { compose.runOnUiThread { !compose.activity.hasWindowFocus() } }
            compose.onNodeWithTag("attachment-choice-sheet").assertDoesNotExist()
            assertInputBlocked()
            compose.runOnIdle { assertEquals("No picker action before window focus returns", 0, clicks) }
        } finally {
            compose.runOnUiThread { picker.dismiss() }
        }
        compose.openAttachmentChoices()
        compose.runOnIdle { assertEquals(1, clicks) }
    }
}
