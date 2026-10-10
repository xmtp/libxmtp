package org.xmtp.android.example.messenger

import android.view.View
import android.view.ViewGroup
import android.widget.TextView
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.emoji2.emojipicker.EmojiPickerView
import androidx.emoji2.emojipicker.RecentEmojiProvider
import androidx.recyclerview.widget.LinearLayoutManager
import androidx.recyclerview.widget.RecyclerView
import org.xmtp.android.example.shared.AppIcons
import org.xmtp.android.example.shared.MessengerAction
import org.xmtp.android.example.shared.MessengerState
import org.xmtp.android.example.shared.Screen
import kotlin.math.ceil
import kotlin.math.floor

private class MemoryEmojiRecents : RecentEmojiProvider {
    private val entries = ArrayDeque<String>()

    @Synchronized override fun recordSelection(emoji: String) {
        entries.remove(emoji)
        entries.addFirst(emoji)
        while (entries.size > 30) entries.removeLast()
    }

    override suspend fun getRecentEmojiList(): List<String> = synchronized(this) { entries.toList() }
}

private class CategoryTargets(
    context: android.content.Context,
    private val target: Int,
) : LinearLayoutManager(context, HORIZONTAL, false) {
    override fun checkLayoutParams(lp: RecyclerView.LayoutParams): Boolean {
        lp.width = target
        lp.height = target
        return true
    }
}

/** Keep the native category strip scrollable with 48 dp targets. */
private fun sizePickerCategories(picker: EmojiPickerView) {
    val target = ceil(48 * picker.resources.displayMetrics.density).toInt()

    fun visit(view: View) {
        if (view is RecyclerView) {
            val manager = view.layoutManager
            if (manager is LinearLayoutManager && manager.orientation == RecyclerView.HORIZONTAL &&
                manager !is CategoryTargets
            ) {
                view.layoutManager = CategoryTargets(view.context, target)
                view.layoutParams = view.layoutParams.apply { height = target }
            }
        }
        if (view is TextView && view.minimumHeight < target) {
            view.minimumHeight = target
            view.layoutParams = view.layoutParams.apply { height = ViewGroup.LayoutParams.WRAP_CONTENT }
        }
        if (view is ViewGroup) repeat(view.childCount) { visit(view.getChildAt(it)) }
    }
    visit(picker)
}

/** The Android picker intercepts only its own action. Other feature actions keep their owner. */
@Composable internal fun ReactionPickerHost(
    state: MessengerState,
    action: (MessengerAction) -> Unit,
    content: @Composable ((MessengerAction) -> Unit) -> Unit,
) {
    var target by remember(state.inbox, state.conversationId, state.screen) { mutableStateOf<String?>(null) }
    val current =
        state.messages
            .firstOrNull {
                it.id == target && !it.deleted
            }.takeIf { state.screen == Screen.TIMELINE }
    val recents = remember { MemoryEmojiRecents() }
    val latestState by rememberUpdatedState(state)
    val latestAction by rememberUpdatedState(action)
    val keyboard = LocalSoftwareKeyboardController.current
    content { value ->
        if (value is MessengerAction.Feature && value.name == "pick-reaction") {
            keyboard?.hide()
            target = value.value
        } else {
            action(value)
        }
    }
    LaunchedEffect(current?.id) { if (current == null) target = null }
    if (current != null) {
        Dialog(onDismissRequest = { target = null }, properties = DialogProperties(usePlatformDefaultWidth = false)) {
            Column(
                Modifier
                    .fillMaxWidth()
                    .padding(8.dp)
                    .background(Color.White, RoundedCornerShape(20.dp))
                    .padding(8.dp),
            ) {
                Row(Modifier.fillMaxWidth().heightIn(min = 48.dp)) {
                    Text("Choose reaction", Modifier.weight(1f).padding(12.dp), color = Color(0xFF172034))
                    IconButton(onClick = { target = null }, modifier = Modifier.size(48.dp)) {
                        Icon(AppIcons.Close, "Close emoji picker", tint = Color(0xFF172034))
                    }
                }
                BoxWithConstraints(Modifier.fillMaxWidth()) {
                    val columns = floor((maxWidth.value - 16) / 48).toInt().coerceAtLeast(1)
                    val density = LocalDensity.current
                    AndroidView(
                        factory = { context ->
                            EmojiPickerView(context).apply {
                                emojiGridColumns = columns
                                emojiGridRows = 4f
                                setRecentEmojiProvider(recents)
                                setOnEmojiPickedListener { item ->
                                    val active = latestState
                                    if (active.screen == Screen.TIMELINE && active.inbox == state.inbox &&
                                        active.conversationId == state.conversationId &&
                                        active.messages.any { it.id == current.id && !it.deleted }
                                    ) {
                                        latestAction(MessengerAction.React(current.id, item.emoji, false))
                                    }
                                    target = null
                                }
                                viewTreeObserver.addOnGlobalLayoutListener { sizePickerCategories(this) }
                            }
                        },
                        update = { picker ->
                            if (picker.emojiGridColumns != columns) picker.emojiGridColumns = columns
                            picker.minimumHeight = with(density) { 240.dp.roundToPx() }
                        },
                        modifier = Modifier.fillMaxWidth().height(320.dp),
                    )
                }
            }
        }
    }
}
