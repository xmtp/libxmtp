package org.xmtp.android.example.shared.attachments

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.MutableTransitionState
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.Image
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Popup
import androidx.compose.ui.window.PopupProperties
import org.xmtp.android.example.shared.AppIcons
import org.xmtp.android.example.shared.designsystem.LocalReducedMotion
import org.xmtp.android.example.shared.designsystem.MessengerMotion

@OptIn(ExperimentalFoundationApi::class)
@Composable
fun AttachmentMessage(
    card: AttachmentCardState,
    preview: ImageBitmap?,
    sender: String,
    time: String,
    action: (AttachmentAction) -> Unit,
    onLongPress: (() -> Unit)? = null,
) {
    var metadata by remember(card.id) { mutableStateOf(false) }
    var details by remember(card.id) { mutableStateOf(false) }
    var menu by remember(card.id) { mutableStateOf(false) }
    val image =
        preview != null || card.mimeType?.startsWith("image/") == true ||
            card.filename.substringAfterLast('.', "").lowercase() in
            setOf("jpg", "jpeg", "png", "gif", "webp", "heic", "heif")
    val reducedMotion = LocalReducedMotion.current
    val duration = if (reducedMotion) 0 else MessengerMotion.PICKER
    val menuTransition = remember(card.id) { MutableTransitionState(false) }
    LaunchedEffect(menu) { menuTransition.targetState = menu }
    Box {
        Box(
            Modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(20.dp))
                .testTag("attachment-card-${card.id}")
                .combinedClickable(
                    onClickLabel = if (image) "Show sender and time" else "File details",
                    onClick = { if (image) metadata = !metadata else details = true },
                    onLongClickLabel = "Attachment actions",
                    onLongClick = { if (onLongPress != null) onLongPress() else menu = true },
                ),
        ) {
            if (image) {
                val ratio =
                    preview?.let { it.width.toFloat() / it.height.coerceAtLeast(1) }?.coerceIn(.65f, 1.8f) ?: 1.3f
                if (preview != null) {
                    Image(
                        preview,
                        "Image attachment",
                        Modifier.fillMaxWidth().aspectRatio(ratio),
                        contentScale = ContentScale.Crop,
                    )
                } else {
                    Surface(
                        Modifier.fillMaxWidth().aspectRatio(ratio),
                        color = MaterialTheme.colorScheme.surfaceContainerHigh,
                    ) {
                        Column(
                            Modifier.padding(24.dp),
                            verticalArrangement = Arrangement.Center,
                            horizontalAlignment = Alignment.CenterHorizontally,
                        ) {
                            Icon(AppIcons.File, null, Modifier.size(36.dp))
                            Spacer(Modifier.height(12.dp))
                            Text(
                                if (card.busy) {
                                    "Loading image…"
                                } else if (card.error !=
                                    null
                                ) {
                                    "Image unavailable"
                                } else {
                                    "Image attachment"
                                },
                            )
                            if (!card.busy) {
                                Text(
                                    "Touch and hold for actions",
                                    style = MaterialTheme.typography.bodySmall,
                                )
                            }
                        }
                    }
                }
                AnimatedVisibility(
                    metadata,
                    Modifier.align(Alignment.BottomStart).testTag("attachment-metadata-${card.id}"),
                    enter = fadeIn(tween(duration)),
                    exit = fadeOut(tween(duration)),
                ) {
                    Surface(color = Color.Black.copy(alpha = .65f), contentColor = Color.White) {
                        Text(
                            "$sender · $time",
                            Modifier.fillMaxWidth().padding(12.dp),
                            style = MaterialTheme.typography.labelMedium,
                        )
                    }
                }
            } else {
                Row(
                    Modifier.fillMaxWidth().padding(16.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    Surface(shape = RoundedCornerShape(12.dp), color = MaterialTheme.colorScheme.primaryContainer) {
                        Box(Modifier.size(44.dp), contentAlignment = Alignment.Center) { Icon(AppIcons.File, null) }
                    }
                    Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                        Text(
                            card.filename,
                            maxLines = 2,
                            overflow = TextOverflow.Ellipsis,
                            fontWeight = FontWeight.SemiBold,
                        )
                        Text(
                            attachmentSummary(card),
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                        if (card.error != null || card.busy || !card.canOpen) {
                            Text(
                                if (card.busy) {
                                    "Loading…"
                                } else if (card.error !=
                                    null
                                ) {
                                    "Download failed"
                                } else {
                                    "Not downloaded"
                                },
                                Modifier.semantics { liveRegion = LiveRegionMode.Polite },
                                style = MaterialTheme.typography.labelSmall,
                                color =
                                    if (card.error !=
                                        null
                                    ) {
                                        MaterialTheme.colorScheme.error
                                    } else {
                                        MaterialTheme.colorScheme.onSurfaceVariant
                                    },
                            )
                        }
                    }
                }
            }
            if (card.busy) LinearProgressIndicator(Modifier.align(Alignment.BottomCenter).fillMaxWidth())
        }
        if (menuTransition.currentState || menuTransition.targetState) {
            Popup(
                alignment = Alignment.TopEnd,
                onDismissRequest = { menu = false },
                properties = PopupProperties(focusable = menu),
            ) {
                AnimatedVisibility(
                    visibleState = menuTransition,
                    enter =
                        fadeIn(tween(duration)) +
                            scaleIn(tween(duration), initialScale = if (reducedMotion) 1f else .98f),
                    exit = fadeOut(tween(duration)),
                ) {
                    Surface(
                        Modifier.widthIn(max = 304.dp),
                        shape = RoundedCornerShape(24.dp),
                        color = MaterialTheme.colorScheme.surface,
                        shadowElevation = 8.dp,
                    ) {
                        Column(Modifier.padding(8.dp)) {
                            AttachmentMessageActions(card, menu, action, { menu = false }, sender, time)
                        }
                    }
                }
            }
        }
    }
    if (details) AttachmentDetails(card, sender, time) { details = false }
}

@Composable
fun AttachmentMessageActions(
    card: AttachmentCardState,
    enabled: Boolean,
    action: (AttachmentAction) -> Unit,
    dismiss: () -> Unit,
    sender: String = "",
    time: String = "",
) {
    var details by remember(card.id) { mutableStateOf(false) }
    if (card.canDownload) {
        TextButton(
            {
                action(AttachmentAction.Download(card.id))
                dismiss()
            },
            Modifier.fillMaxWidth().heightIn(min = 48.dp),
            enabled =
                enabled && !card.busy,
        ) {
            Text(if (card.error == null) "Download" else "Retry download")
        }
    }
    if (card.canOpen) {
        TextButton(
            {
                action(AttachmentAction.Open(card.id))
                dismiss()
            },
            Modifier.fillMaxWidth().heightIn(min = 48.dp),
            enabled =
                enabled && !card.busy,
        ) { Text("Open") }
    }
    if (card.canSave) {
        TextButton(
            {
                action(AttachmentAction.Save(card.id))
                dismiss()
            },
            Modifier.fillMaxWidth().heightIn(min = 48.dp),
            enabled =
                enabled && !card.busy,
        ) { Text("Save") }
    }
    TextButton({ details = true }, Modifier.fillMaxWidth().heightIn(min = 48.dp), enabled = enabled) { Text("Details") }
    if (details) {
        AttachmentDetails(card, sender, time) {
            details = false
            dismiss()
        }
    }
}

@Composable
private fun AttachmentDetails(
    card: AttachmentCardState,
    sender: String,
    time: String,
    dismiss: () -> Unit,
) {
    AlertDialog(
        onDismissRequest = dismiss,
        title = { Text("Attachment details") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(card.filename)
                Text(attachmentSummary(card))
                if (card.imageWidth != null &&
                    card.imageHeight != null
                ) {
                    Text("${card.imageWidth} × ${card.imageHeight}")
                }
                if (sender.isNotBlank()) Text(sender)
                if (time.isNotBlank()) Text(time)
                Text(card.status)
                card.error?.let { Text(it, color = MaterialTheme.colorScheme.error) }
            }
        },
        confirmButton = { TextButton(dismiss) { Text("Done") } },
    )
}

private fun attachmentSummary(card: AttachmentCardState) =
    buildList {
        add(attachmentType(card.mimeType))
        card.verifiedSizeBytes?.let { add(attachmentSize(it)) }
    }.joinToString(" · ")
