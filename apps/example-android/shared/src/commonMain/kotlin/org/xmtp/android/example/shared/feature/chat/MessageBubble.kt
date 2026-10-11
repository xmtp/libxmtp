package org.xmtp.android.example.shared.feature.chat

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.focusable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.awaitHorizontalTouchSlopOrCancellation
import androidx.compose.foundation.gestures.horizontalDrag
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.positionChange
import androidx.compose.ui.layout.boundsInWindow
import androidx.compose.ui.layout.findRootCoordinates
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import org.xmtp.android.example.shared.*
import org.xmtp.android.example.shared.designsystem.*

@OptIn(ExperimentalFoundationApi::class, ExperimentalLayoutApi::class)
@Composable
internal fun MessageBubble(
    row: MessageRow,
    showSender: Boolean,
    selected: Boolean,
    restoreFocus: Boolean,
    action: (ChatAction) -> Unit,
    boundsChanged: (Rect) -> Unit,
    extra: @Composable (MessageRow) -> Unit,
) {
    val reduced = LocalReducedMotion.current
    val density = LocalDensity.current
    val haptics = LocalHapticFeedback.current
    val layoutDirection = LocalLayoutDirection.current
    val direction = if (layoutDirection == LayoutDirection.Rtl) -1 else 1
    val threshold = with(density) { 48.dp.toPx() }
    val edge =
        WindowInsets.systemGestures
            .getLeft(density, layoutDirection)
            .coerceAtLeast(with(density) { 24.dp.roundToPx() })
    val endEdge =
        WindowInsets.systemGestures
            .getRight(density, layoutDirection)
            .coerceAtLeast(with(density) { 24.dp.roundToPx() })
    var windowWidth by remember { mutableStateOf(0) }
    val focus = remember(row.id) { FocusRequester() }
    var wasSelected by remember(row.id) { mutableStateOf(false) }
    LaunchedEffect(selected) {
        if (wasSelected && !selected && restoreFocus) focus.requestFocus()
        wasSelected = selected
    }
    var bounds by remember(row.id) { mutableStateOf(Rect.Zero) }
    var drag by remember(row.id) { mutableStateOf(0f) }
    var armed by remember(row.id) { mutableStateOf(false) }
    val currentAction by rememberUpdatedState(action)
    val lift by animateFloatAsState(
        if (selected &&
            !reduced
        ) {
            1.02f
        } else {
            1f
        },
        tween(if (reduced) 0 else MessengerMotion.PICKER),
        label = "Selected message",
    )
    Column(Modifier.fillMaxWidth(), horizontalAlignment = if (row.mine) Alignment.End else Alignment.Start) {
        if (showSender &&
            !row.mine
        ) {
            Text(
                row.sender,
                Modifier.padding(start = 12.dp, bottom = 5.dp),
                style = MaterialTheme.typography.labelLarge,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        Surface(
            modifier =
                Modifier
                    .widthIn(max = 480.dp)
                    .fillMaxWidth(.88f)
                    .onGloballyPositioned {
                        bounds = it.boundsInWindow()
                        windowWidth = it.findRootCoordinates().size.width
                        boundsChanged(bounds)
                    }.focusRequester(focus)
                    .focusable()
                    .graphicsLayer {
                        translationX = if (reduced) 0f else drag * direction
                        scaleX = lift
                        scaleY = lift
                    }.pointerInput(row.id, row.deleted, direction, threshold, edge, endEdge) {
                        fun move(amount: Float) {
                            drag = (drag + amount * direction).coerceIn(0f, threshold * 1.4f)
                            if (drag >= threshold && !armed) {
                                armed = true
                                haptics.performHapticFeedback(HapticFeedbackType.LongPress)
                            }
                            if (drag < threshold) armed = false
                        }
                        awaitEachGesture {
                            val down = awaitFirstDown(requireUnconsumed = false)
                            val originalX = bounds.left + down.position.x
                            if (row.deleted || originalX <= edge ||
                                windowWidth - originalX <= endEdge
                            ) {
                                return@awaitEachGesture
                            }
                            val start =
                                awaitHorizontalTouchSlopOrCancellation(down.id) { change, overSlop ->
                                    move(overSlop)
                                    change.consume()
                                }
                            if (start != null) {
                                val completed =
                                    horizontalDrag(start.id) { change ->
                                        move(change.positionChange().x)
                                        change.consume()
                                    }
                                if (completed && armed) currentAction(ChatAction.Reply(row.id))
                            }
                            drag = 0f
                            armed = false
                        }
                    }.combinedClickable(enabled = !row.deleted, onClick = {
                        action(ChatAction.SelectMessage(row.id))
                    }, onLongClickLabel = "Message actions", onLongClick = {
                        action(
                            ChatAction.SelectMessage(row.id),
                        )
                    }),
            color =
                if (row.mine) {
                    MaterialTheme.colorScheme.primaryContainer
                } else {
                    MaterialTheme.colorScheme.surfaceVariant
                },
            shape = RoundedCornerShape(20.dp),
            shadowElevation = if (selected) 4.dp else 0.dp,
        ) {
            Column(
                Modifier.padding(horizontal = 14.dp, vertical = 10.dp),
                verticalArrangement = Arrangement.spacedBy(5.dp),
            ) {
                row.reply?.let { reply ->
                    Surface(
                        color = MaterialTheme.colorScheme.surface.copy(alpha = .5f),
                        shape = RoundedCornerShape(10.dp),
                    ) {
                        Text(
                            reply,
                            Modifier.padding(10.dp),
                            style = MaterialTheme.typography.bodyMedium,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                }
                Text(
                    row.text,
                    style = MaterialTheme.typography.bodyLarge,
                    color =
                        if (row.mine) {
                            MaterialTheme.colorScheme.onPrimaryContainer
                        } else {
                            MaterialTheme.colorScheme.onSurface
                        },
                )
                if (row.attachment) extra(row)
                Text(
                    if (row.mine) "${row.time} · ${row.sendState.label}" else row.time,
                    Modifier.align(Alignment.End).semantics {
                        if (row.mine &&
                            (row.sendState == MessageSendState.QUEUED || row.sendState == MessageSendState.NOT_SENT)
                        ) {
                            liveRegion =
                                LiveRegionMode.Polite
                        }
                    },
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    style = MaterialTheme.typography.labelSmall,
                )
                if (row.mine && row.sendState == MessageSendState.NOT_SENT) {
                    FlowRow {
                        TextButton(
                            { action(ChatAction.RetrySend(row.id)) },
                            Modifier.heightIn(min = 48.dp),
                        ) { Text("Retry") }
                        TextButton(
                            { action(ChatAction.EditFailedSend(row.id)) },
                            Modifier.heightIn(min = 48.dp),
                        ) { Text("Edit") }
                    }
                }
            }
        }
        if (row.reactions.isNotEmpty()) {
            FlowRow(
                Modifier.widthIn(max = 480.dp).fillMaxWidth(.88f),
                horizontalArrangement = if (row.mine) Arrangement.End else Arrangement.Start,
            ) {
                row.reactions.forEach { reaction ->
                    Surface(
                        modifier =
                            Modifier.padding(top = 4.dp, end = 4.dp).heightIn(min = 48.dp).combinedClickable(
                                onClick = { action(ChatAction.ShowReactors(row.id, reaction.emoji)) },
                                onLongClickLabel = if (reaction.mine) "Remove reaction" else "Add reaction",
                                onLongClick = {
                                    action(ChatAction.React(row.id, reaction.emoji, reaction.mine))
                                    haptics.performHapticFeedback(HapticFeedbackType.LongPress)
                                },
                            ),
                        shape = RoundedCornerShape(24.dp),
                        color =
                            if (reaction.mine) {
                                MaterialTheme.colorScheme.primaryContainer
                            } else {
                                MaterialTheme.colorScheme.surface
                            },
                        border =
                            androidx.compose.foundation.BorderStroke(
                                1.dp,
                                if (reaction.mine) {
                                    MaterialTheme.colorScheme.primary.copy(
                                        alpha = .3f,
                                    )
                                } else {
                                    MaterialTheme.colorScheme.outlineVariant
                                },
                            ),
                    ) {
                        Row(
                            Modifier.padding(horizontal = 12.dp, vertical = 10.dp),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            Text("${reaction.emoji} ${reaction.count}", style = MaterialTheme.typography.bodyMedium)
                        }
                    }
                }
            }
        }
    }
}
