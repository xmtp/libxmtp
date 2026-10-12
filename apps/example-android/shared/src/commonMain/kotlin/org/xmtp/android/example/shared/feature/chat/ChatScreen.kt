package org.xmtp.android.example.shared.feature.chat
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.MutableTransitionState
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.boundsInWindow
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntRect
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Popup
import androidx.compose.ui.window.PopupPositionProvider
import androidx.compose.ui.window.PopupProperties
import kotlinx.coroutines.flow.distinctUntilChanged
import org.xmtp.android.example.shared.*
import kotlin.math.roundToInt

@Composable fun ChatScreen(
    state: ChatState,
    action: (ChatAction) -> Unit,
    shellAction: (ShellAction) -> Unit = {},
    composerExtra: @Composable () -> Unit = {},
    messageExtra: @Composable (MessageRow) -> Unit = {},
) {
    val list = key(state.conversationId) { rememberLazyListState() }
    val text = state.composer.text
    val pendingSend = state.composer.pendingRequestId
    val selected = state.overlay?.messageId
    var selectedBounds by remember(state.conversationId) { mutableStateOf(Rect.Zero) }
    var composerTop by remember(state.conversationId) { mutableStateOf(Float.POSITIVE_INFINITY) }
    val composerFocus = remember(state.conversationId) { FocusRequester() }
    var composerHasFocus by remember(state.conversationId) { mutableStateOf(false) }
    val expanded = state.overlay?.surface == MessageSurface.POPOVER
    val transition = remember(state.conversationId) { MutableTransitionState(false) }
    val currentSelection = state.messages.firstOrNull { it.id == selected && !it.deleted }
    transition.targetState = expanded && currentSelection != null
    val keyboard = LocalSoftwareKeyboardController.current
    var initialRestoreComplete by remember(state.conversationId) { mutableStateOf(false) }
    val latestState by rememberUpdatedState(state)
    LaunchedEffect(
        state.conversationId,
        state.anchor?.messageId,
        state.messages.any { it.id == state.anchor?.messageId },
    ) {
        state.anchor?.takeIf { !initialRestoreComplete }?.let { anchor ->
            val index =
                state.messages.indexOfFirst {
                    it.id ==
                        anchor.messageId
                }
            if (index >= 0) {
                list
                    .scrollToItem(
                        index,
                        anchor.offsetPx,
                    )
                initialRestoreComplete = true
            }
        }
    }
    LaunchedEffect(
        state.conversationId,
        state.messages,
    ) {
        snapshotFlow {
            val pendingAnchor = !initialRestoreComplete && latestState.anchor != null
            if (pendingAnchor) null else list.firstVisibleItemIndex to list.firstVisibleItemScrollOffset
        }.distinctUntilChanged().collect { position ->
            position?.let { (index, offset) ->
                latestState.messages.getOrNull(index)?.let { row ->
                    initialRestoreComplete = true
                    action(
                        ChatAction.Viewport(
                            ScrollAnchor(row.id, row.sentAtNs, offset, index == 0 && offset == 0),
                            index == 0 && offset == 0,
                        ),
                    )
                }
            }
        }
    }
    Column(
        Modifier
            .fillMaxSize(),
    ) {
        state.readerFailure?.message?.let {
            Notice(
                it,
                "Retry reader",
            ) {
                shellAction(ShellAction.RetryReader)
            }
        }
        state.historyNotice?.let {
            Notice(it)
        }
        state.recoveryNotice?.let { Notice(it) }
        if (state.hasOlderRecovery || !state.recoveryAtNewest) {
            Row(Modifier.fillMaxWidth()) {
                if (state.hasOlderRecovery) {
                    TextButton(
                        { action(ChatAction.LoadOlderRecovery) },
                        Modifier.weight(1f).heightIn(min = 48.dp),
                    ) {
                        Text("Older pending messages")
                    }
                }
                if (!state.recoveryAtNewest) {
                    TextButton({ action(ChatAction.LatestRecovery) }, Modifier.weight(1f).heightIn(min = 48.dp)) {
                        Text("Newest pending messages")
                    }
                }
            }
        }
        if (state.unknown) {
            Row {
                TextButton(
                    {
                        action(
                            ChatAction
                                .Consent(true),
                        )
                    },
                    Modifier
                        .heightIn(
                            min =
                                48.dp,
                        ),
                ) {
                    Text("Allow")
                }
                TextButton(
                    {
                        action(
                            ChatAction
                                .Consent(false),
                        )
                    },
                    Modifier
                        .heightIn(
                            min =
                                48.dp,
                        ),
                ) {
                    Text("Block")
                }
            }
        }
        LazyColumn(
            state = list,
            reverseLayout = true,
            modifier =
                Modifier
                    .weight(1f),
            contentPadding =
                PaddingValues(
                    16.dp,
                ),
            verticalArrangement =
                Arrangement
                    .spacedBy(
                        8.dp,
                    ),
        ) {
            items(
                state.messages,
                key = {
                    it.id
                },
            ) { row ->
                var bubbleBounds by remember(row.id) { mutableStateOf(Rect.Zero) }
                Column(
                    Modifier
                        .fillMaxWidth(),
                    horizontalAlignment =
                        if (row.mine) {
                            Alignment.End
                        } else {
                            Alignment.Start
                        },
                ) {
                    Text(
                        "${row.sender} · ${row.day}",
                        style =
                            MaterialTheme.typography.labelSmall,
                    )
                    Column(
                        Modifier
                            .widthIn(
                                max =
                                    320.dp,
                            ).background(
                                if (row.mine) {
                                    outgoing
                                } else {
                                    Color(0xFFF1F3F7)
                                },
                                RoundedCornerShape(
                                    16.dp,
                                ),
                            ).onGloballyPositioned {
                                bubbleBounds = it.boundsInWindow()
                                if (row.id == selected) selectedBounds = bubbleBounds
                            }.clickable(enabled = !row.deleted) {
                                keyboard?.hide()
                                selectedBounds = bubbleBounds
                                action(ChatAction.SelectMessage(row.id))
                            }.padding(
                                12.dp,
                            ),
                    ) {
                        row.reply?.let {
                            Text(
                                "↳ $it",
                                color = blue,
                                style =
                                    MaterialTheme.typography.bodySmall,
                            )
                        }
                        Text(
                            row.text,
                        )
                        if (row.attachment) {
                            messageExtra(row)
                        }
                        Text(
                            "${row.time} · ${row.sendState.label}",
                            style =
                                MaterialTheme.typography.labelSmall,
                        )
                        Row {
                            row.reactions.forEach { reaction ->
                                TextButton(
                                    {
                                        action(
                                            ChatAction
                                                .React(
                                                    row.id,
                                                    reaction.emoji,
                                                    reaction.mine,
                                                ),
                                        )
                                    },
                                    Modifier
                                        .heightIn(
                                            min =
                                                48.dp,
                                        ),
                                ) {
                                    Text("${reaction.emoji} ${reaction.count}")
                                }
                            }
                        }
                    }
                }
            }
            if (state.hasOlder) {
                item {
                    TextButton(
                        {
                            action(
                                ChatAction.LoadOlder,
                            )
                        },
                        Modifier
                            .fillMaxWidth()
                            .heightIn(
                                min =
                                    48.dp,
                            ),
                    ) {
                        Text("Load older")
                    }
                }
            }
        }
        TextButton(
            {
                action(
                    ChatAction.JumpToLatest,
                )
            },
            Modifier
                .fillMaxWidth()
                .heightIn(
                    min =
                        48.dp,
                ),
        ) {
            Text("Jump to latest")
        }
        composerExtra()
        Row(
            Modifier
                .fillMaxWidth()
                .onGloballyPositioned { composerTop = it.boundsInWindow().top }
                .padding(
                    12.dp,
                ),
            verticalAlignment =
                Alignment.CenterVertically,
        ) {
            if (state.features.attachments) {
                CircleAction(
                    "Select file",
                    AppIcons.Add,
                ) {
                    action(
                        ChatAction.SelectAttachment,
                    )
                }
            }
            Column(
                Modifier
                    .weight(
                        1f,
                    ).testTag("message-composer")
                    .background(Color(0xFFF1F3F7), RoundedCornerShape(28.dp)),
            ) {
                state.composer.reply?.let { reply ->
                    Row(
                        Modifier.fillMaxWidth().heightIn(min = 48.dp).padding(start = 16.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Text(
                            reply.firstLine,
                            Modifier.weight(1f),
                            color = blue,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                            style = MaterialTheme.typography.bodySmall,
                        )
                        IconButton(
                            onClick = { action(ChatAction.Reply(null)) },
                            modifier = Modifier.size(48.dp),
                        ) {
                            Icon(AppIcons.Close, "Cancel reply", tint = ink)
                        }
                    }
                }
                val inputInstanceId = state.composer.inputInstanceId
                val restoreComposerFocus = composerHasFocus
                key(inputInstanceId) {
                    LaunchedEffect(Unit) {
                        if (restoreComposerFocus) composerFocus.requestFocus()
                    }
                    OutlinedTextField(
                        androidx.compose.ui.text.input.TextFieldValue(
                            text,
                            androidx.compose.ui.text.TextRange(
                                state.composer.selectionStart.coerceIn(0, text.length),
                                state.composer.selectionEnd.coerceIn(0, text.length),
                            ),
                            composition =
                                state.composer.compositionStart?.let { start ->
                                    state.composer.compositionEnd?.let { end ->
                                        androidx.compose.ui.text.TextRange(
                                            start.coerceIn(0, text.length),
                                            end.coerceIn(0, text.length),
                                        )
                                    }
                                },
                        ),
                        { value ->
                            action(
                                ChatAction.ChangeDraft(
                                    value.text,
                                    value.selection.start,
                                    value.selection.end,
                                    value.composition?.start,
                                    value.composition?.end,
                                    inputInstanceId,
                                ),
                            )
                        },
                        placeholder = {
                            Text("Message")
                        },
                        modifier =
                            Modifier
                                .fillMaxWidth()
                                .focusRequester(composerFocus)
                                .onFocusChanged { composerHasFocus = it.isFocused },
                        maxLines = 5,
                        shape = RoundedCornerShape(28.dp),
                        colors =
                            OutlinedTextFieldDefaults.colors(
                                unfocusedContainerColor = Color(0xFFF1F3F7),
                                focusedContainerColor = Color(0xFFF1F3F7),
                                unfocusedBorderColor = Color.Transparent,
                            ),
                    )
                }
            }
            TextButton(
                {
                    action(ChatAction.SendText(text, 0, state.composer.revision))
                },
                Modifier
                    .heightIn(
                        min =
                            48.dp,
                    ),
                enabled =
                    text
                        .isNotBlank() &&
                        !state.busy && state.composer.enabled && pendingSend == null,
            ) {
                Text("Send")
            }
        }
    }
    if (currentSelection != null && (transition.currentState || transition.targetState)) {
        val gap = with(LocalDensity.current) { 8.dp.toPx() }
        Popup(
            popupPositionProvider = MessagePopupPosition(selectedBounds, composerTop, gap),
            onDismissRequest = { action(ChatAction.DismissOverlay) },
            properties = PopupProperties(focusable = true),
        ) {
            AnimatedVisibility(
                visibleState = transition,
                enter = fadeIn(tween(140)) + slideInVertically(tween(140)) { -8 },
                exit = fadeOut(tween(100)) + slideOutVertically(tween(100)) { -4 },
            ) {
                val maximumHeight = with(LocalDensity.current) { (composerTop - gap).toDp() }
                MessagePopover(
                    currentSelection,
                    state.features,
                    expanded,
                    maximumHeight,
                    action,
                ) { action(ChatAction.DismissOverlay) }
            }
        }
    }
}

private val quickReactions = listOf("👍", "❤️", "😂", "😮", "😢")

private class MessagePopupPosition(
    private val bubble: Rect,
    private val composerTop: Float,
    private val gap: Float,
) : PopupPositionProvider {
    override fun calculatePosition(
        anchorBounds: IntRect,
        windowSize: IntSize,
        layoutDirection: LayoutDirection,
        popupContentSize: IntSize,
    ): IntOffset {
        val x =
            (bubble.center.x - popupContentSize.width / 2)
                .roundToInt()
                .coerceIn(0, (windowSize.width - popupContentSize.width).coerceAtLeast(0))
        val bottom = minOf(windowSize.height.toFloat(), composerTop) - gap
        val above = bubble.top - popupContentSize.height - gap
        val below = bubble.bottom + gap
        val preferred =
            if (above >=
                0
            ) {
                above
            } else if (below + popupContentSize.height <= bottom) {
                below
            } else {
                bottom - popupContentSize.height
            }
        val y =
            preferred
                .roundToInt()
                .coerceIn(0, (bottom - popupContentSize.height).roundToInt().coerceAtLeast(0))
        return IntOffset(x, y)
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun MessagePopover(
    row: MessageRow,
    features: FeatureAvailability,
    expanded: Boolean,
    maximumHeight: androidx.compose.ui.unit.Dp,
    action: (ChatAction) -> Unit,
    dismiss: () -> Unit,
) {
    Surface(
        modifier =
            Modifier
                .testTag(
                    "reaction-popover",
                ).width(304.dp)
                .heightIn(max = maximumHeight.coerceAtLeast(48.dp)),
        color = Color.White,
        shape = RoundedCornerShape(20.dp),
        shadowElevation = 8.dp,
    ) {
        Column(Modifier.verticalScroll(rememberScrollState()).padding(8.dp)) {
            Row {
                quickReactions.forEach { emoji ->
                    TextButton(
                        onClick = {
                            action(ChatAction.React(row.id, emoji, false))
                            dismiss()
                        },
                        modifier = Modifier.size(48.dp),
                        enabled = expanded,
                        contentPadding = PaddingValues(0.dp),
                    ) {
                        Text(emoji, fontSize = with(LocalDensity.current) { (24 / fontScale).sp })
                    }
                }
                IconButton(
                    onClick = {
                        action(ChatAction.ShowEmoji(row.id))
                        dismiss()
                    },
                    modifier = Modifier.size(48.dp),
                    enabled = expanded,
                ) { Icon(AppIcons.More, "More reactions", tint = ink) }
            }
            FlowRow(Modifier.fillMaxWidth()) {
                TextButton(
                    onClick = {
                        action(ChatAction.Reply(row.id))
                        dismiss()
                    },
                    modifier = Modifier.heightIn(min = 48.dp),
                    enabled = expanded,
                ) { Text("Reply") }
                if (row.mine) {
                    TextButton(
                        onClick = {
                            action(ChatAction.DeleteMessage(row.id))
                            dismiss()
                        },
                        modifier = Modifier.heightIn(min = 48.dp),
                        enabled = expanded,
                    ) { Text("Delete message") }
                }
                if (row.sendState == MessageSendState.NOT_SENT || row.sendState == MessageSendState.QUEUED) {
                    TextButton(
                        onClick = {
                            action(ChatAction.RetrySend(row.id))
                            dismiss()
                        },
                        modifier = Modifier.heightIn(min = 48.dp),
                        enabled = expanded,
                    ) { Text("Retry publication") }
                }
            }
            if (row.attachment && features.attachments) {
                TextButton(
                    onClick = {
                        action(ChatAction.OpenAttachment(row.id))
                        dismiss()
                    },
                    modifier = Modifier.heightIn(min = 48.dp),
                    enabled = expanded,
                ) { Text("Download / Open") }
            }
        }
    }
}
