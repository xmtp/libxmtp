package org.xmtp.android.example.messenger

import android.view.ContextThemeWrapper
import android.view.View
import android.view.ViewGroup
import android.widget.TextView
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.emoji2.emojipicker.EmojiPickerView
import androidx.emoji2.emojipicker.RecentEmojiProvider
import androidx.recyclerview.widget.LinearLayoutManager
import androidx.recyclerview.widget.RecyclerView
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.xmtp.android.example.shared.AppIcons
import org.xmtp.android.example.shared.MessengerAction
import org.xmtp.android.example.shared.MessengerState
import org.xmtp.android.example.shared.Screen
import org.xmtp.android.example.shared.designsystem.MessengerSheet
import org.xmtp.android.example.shared.designsystem.MessengerTheme
import org.xmtp.android.example.shared.feature.chat.ChatAction
import org.xmtp.android.example.shared.feature.chat.MessageSurface
import org.xmtp.android.example.shared.toUiState
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

private data class SearchEmoji(
    val value: String,
    val name: String,
)

/** The catalogue contains the Unicode Emoji 16.0 fully qualified set. */
private suspend fun readEmojiCatalogue(context: android.content.Context): List<SearchEmoji> =
    withContext(Dispatchers.IO) {
        context.assets.open("emoji-search.tsv").bufferedReader().useLines { lines ->
            lines
                .filter { !it.startsWith("#") && it.contains('\t') }
                .map { line ->
                    SearchEmoji(line.substringBefore('\t'), line.substringAfter('\t'))
                }.toList()
        }
    }

/** Android owns the complete emoji picker. Shared state owns its selected message and search. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun ReactionPickerHost(
    state: MessengerState,
    action: (MessengerAction) -> Unit,
    content: @Composable ((MessengerAction) -> Unit) -> Unit,
) {
    val ui = state.toUiState()
    MessengerTheme(ui.shell.appearance, ui.shell.reducedMotion) {
        ReactionPickerContent(state, action, content)
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun ReactionPickerContent(
    state: MessengerState,
    action: (MessengerAction) -> Unit,
    content: @Composable ((MessengerAction) -> Unit) -> Unit,
) {
    val chat = state.toUiState().chat
    val requestedOverlay = chat.overlay?.takeIf { it.surface == MessageSurface.EMOJI && !chat.unknown }
    var presentedOverlay by remember(state.inbox, state.conversationId, state.screen) {
        mutableStateOf<org.xmtp.android.example.shared.feature.chat.MessageOverlayState?>(null)
    }
    LaunchedEffect(requestedOverlay) { if (requestedOverlay != null) presentedOverlay = requestedOverlay }
    val overlay = presentedOverlay
    val current =
        chat.messages.firstOrNull { it.id == overlay?.messageId && !it.deleted }.takeIf {
            state.screen ==
                Screen.TIMELINE
        }
    val recents = remember { MemoryEmojiRecents() }
    val latestState by rememberUpdatedState(state)
    val latestAction by rememberUpdatedState(action)
    val keyboard = LocalSoftwareKeyboardController.current
    content { value ->
        if (value is MessengerAction.PickReaction) {
            keyboard?.hide()
            action(MessengerAction.ChatFeature(ChatAction.ShowEmoji(value.messageId)))
        } else {
            if (value is MessengerAction.ChatFeature && value.action is ChatAction.ShowEmoji) keyboard?.hide()
            action(value)
        }
    }
    if (current != null && overlay != null) {
        val context = LocalContext.current
        val haptics = LocalHapticFeedback.current
        val catalogueContext = context.applicationContext ?: context
        val catalogue by produceState<List<SearchEmoji>>(emptyList(), catalogueContext) {
            value =
                readEmojiCatalogue(catalogueContext)
        }
        val query = overlay.emojiQuery
        val results =
            remember(catalogue, query) {
                val words =
                    query
                        .trim()
                        .lowercase()
                        .split(Regex("\\s+"))
                        .filter { it.isNotEmpty() }
                catalogue.filter { item ->
                    words.all { item.name.contains(it, ignoreCase = true) || item.value.contains(it) }
                }
            }
        val close = {
            action(
                MessengerAction.ChatFeature(
                    ChatAction.CloseSheet(current.id, MessageSurface.EMOJI, overlay.instanceId),
                ),
            )
        }
        val changeQuery: (String) -> Unit = { value ->
            val active = latestState
            val activeOverlay = active.toUiState().chat.overlay
            if (active.screen == Screen.TIMELINE && active.inbox == state.inbox &&
                active.conversationId == state.conversationId &&
                activeOverlay != null && activeOverlay.messageId == overlay.messageId &&
                activeOverlay.surface == MessageSurface.EMOJI &&
                activeOverlay.instanceId == overlay.instanceId
            ) {
                latestAction(MessengerAction.ChatFeature(ChatAction.ChangeEmojiQuery(value)))
            }
        }
        val select: (String) -> Unit = { emoji ->
            val active = latestState
            val activeChat = active.toUiState().chat
            val activeOverlay = activeChat.overlay
            if (active.screen == Screen.TIMELINE && active.inbox == state.inbox &&
                active.conversationId == state.conversationId &&
                activeOverlay != null &&
                activeOverlay.messageId == current.id &&
                activeOverlay.surface == MessageSurface.EMOJI &&
                activeOverlay.instanceId == overlay.instanceId &&
                !activeChat.unknown &&
                activeChat.messages.any { it.id == current.id && !it.deleted }
            ) {
                recents.recordSelection(emoji)
                latestAction(
                    MessengerAction.React(
                        current.id,
                        emoji,
                        current.reactions.firstOrNull { it.emoji == emoji }?.mine == true,
                    ),
                )
                haptics.performHapticFeedback(HapticFeedbackType.LongPress)
                latestAction(MessengerAction.ChatFeature(ChatAction.DismissOverlay))
            }
        }
        MessengerSheet(onDismiss = close, visible = requestedOverlay != null, onHidden = {
            if (presentedOverlay?.instanceId ==
                overlay.instanceId &&
                presentedOverlay?.messageId == overlay.messageId
            ) {
                presentedOverlay = null
            }
        }) {
            Column(
                Modifier
                    .fillMaxWidth()
                    .imePadding()
                    .padding(horizontal = 16.dp)
                    .testTag("emoji-sheet"),
            ) {
                Row(Modifier.fillMaxWidth(), verticalAlignment = androidx.compose.ui.Alignment.CenterVertically) {
                    Text("Choose a reaction", Modifier.weight(1f), style = MaterialTheme.typography.titleLarge)
                    IconButton(close, Modifier.size(48.dp), enabled = requestedOverlay != null) {
                        Icon(AppIcons.Close, "Close emoji picker")
                    }
                }
                OutlinedTextField(
                    query,
                    changeQuery,
                    Modifier.fillMaxWidth().padding(vertical = 12.dp).testTag("emoji-search"),
                    label = {
                        Text("Search emoji")
                    },
                    singleLine = true,
                    enabled =
                        requestedOverlay?.instanceId == overlay.instanceId &&
                            requestedOverlay?.messageId == overlay.messageId,
                    shape = RoundedCornerShape(16.dp),
                )
                if (query.isBlank()) {
                    BoxWithConstraints(Modifier.fillMaxWidth()) {
                        val columns = floor(maxWidth.value / 48).toInt().coerceAtLeast(1)
                        val density = LocalDensity.current
                        val background = MaterialTheme.colorScheme.surface.toArgb()
                        val foreground = MaterialTheme.colorScheme.onSurface.toArgb()
                        val dark = MaterialTheme.colorScheme.surface.luminance() < .5f
                        AndroidView(
                            factory = { parent ->
                                EmojiPickerView(
                                    ContextThemeWrapper(
                                        parent,
                                        if (dark) {
                                            android.R.style.Theme_Material_NoActionBar
                                        } else {
                                            android.R.style.Theme_Material_Light_NoActionBar
                                        },
                                    ),
                                ).apply {
                                    emojiGridColumns = columns
                                    setRecentEmojiProvider(recents)
                                    setOnEmojiPickedListener { select(it.emoji) }
                                    viewTreeObserver.addOnGlobalLayoutListener {
                                        sizePickerCategories(this)
                                        tintPicker(this, background, foreground)
                                    }
                                }
                            },
                            update = { picker ->
                                if (picker.emojiGridColumns != columns) picker.emojiGridColumns = columns
                                picker.minimumHeight = with(density) { 240.dp.roundToPx() }
                                picker.setOnEmojiPickedListener { select(it.emoji) }
                                tintPicker(picker, background, foreground)
                            },
                            modifier = Modifier.fillMaxWidth().heightIn(min = 240.dp, max = 360.dp),
                        )
                    }
                } else {
                    Text(
                        "${results.size} results",
                        style = MaterialTheme.typography.labelMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                    if (catalogue.isEmpty()) LinearProgressIndicator(Modifier.fillMaxWidth())
                    if (catalogue.isNotEmpty() &&
                        results.isEmpty()
                    ) {
                        Text("No emoji match this search.", Modifier.padding(vertical = 24.dp))
                    }
                    LazyVerticalGrid(
                        GridCells.Adaptive(48.dp),
                        Modifier.fillMaxWidth().heightIn(max = 360.dp),
                        contentPadding = PaddingValues(vertical = 12.dp),
                    ) {
                        items(results, key = { it.value }) { item ->
                            TextButton(
                                { select(item.value) },
                                Modifier.size(48.dp).semantics {
                                    contentDescription =
                                        item.name
                                },
                                enabled = requestedOverlay != null,
                                contentPadding = PaddingValues(0.dp),
                            ) {
                                Text(item.value, fontSize = with(LocalDensity.current) { (26 / fontScale).sp })
                            }
                        }
                    }
                }
                Spacer(Modifier.height(24.dp))
            }
        }
    }
}

private fun tintPicker(
    view: View,
    background: Int,
    foreground: Int,
) {
    if (view is ViewGroup) {
        view.setBackgroundColor(background)
        repeat(view.childCount) { tintPicker(view.getChildAt(it), background, foreground) }
    }
    if (view is TextView) view.setTextColor(foreground)
}
