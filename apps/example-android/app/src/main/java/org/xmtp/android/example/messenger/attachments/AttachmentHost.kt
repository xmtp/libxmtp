package org.xmtp.android.example.messenger.attachments

import androidx.activity.ComponentActivity
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.*
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.lifecycle.lifecycleScope
import kotlinx.coroutines.launch
import org.xmtp.android.example.messenger.feature.attachments.AttachmentController
import org.xmtp.android.example.shared.ConversationRow
import org.xmtp.android.example.shared.MessageRow
import org.xmtp.android.example.shared.attachments.*
import org.xmtp.android.example.shared.feature.attachments.AttachmentChoiceSheet
import org.xmtp.android.example.shared.feature.attachments.AttachmentChoiceState
import org.xmtp.android.example.shared.label

/** Launches Android requests. Transfer and recovery work belongs to the retained controller. */
internal class AttachmentHost(
    private val activity: ComponentActivity,
    private val controller: AttachmentController,
) {
    private val requests = controller.requests
    private val hostId = requests.attach()
    private val pick =
        activity.registerForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
            requests.launchedSelect()?.let { controller.selected(it.identity, uri) }
        }
    private val save =
        activity.registerForActivityResult(ActivityResultContracts.CreateDocument("application/octet-stream")) { uri ->
            requests.launchedSave()?.let { controller.saved(it.identity, uri) }
        }
    private val effects =
        activity.lifecycleScope.launch {
            requests.state.collect { pending ->
                pending.forEach { request ->
                    controller.launch(request, hostId) {
                        when (request) {
                            is AttachmentRequest.Select -> {
                                pick.launch(
                                    when (request.selection) {
                                        AttachmentSelection.FILE -> arrayOf("*/*")
                                        AttachmentSelection.PHOTO_VIDEO -> arrayOf("image/*", "video/*")
                                    },
                                )
                            }

                            is AttachmentRequest.Save -> {
                                save.launch(request.downloaded.filename ?: "File")
                            }

                            is AttachmentRequest.Open -> {
                                activity.startActivity(request.export.intent)
                            }
                        }
                    }
                }
            }
        }

    fun close() {
        requests.closeHost(hostId)
        effects.cancel()
    }

    internal fun verifiedMessageIds() = controller.verifiedMessageIds()

    internal var beforeDownloadRefresh: suspend () -> Unit
        get() = controller.beforeDownloadRefresh
        set(value) {
            controller.beforeDownloadRefresh = value
        }
    internal var beforeDownload: suspend () -> Unit
        get() = controller.beforeDownload
        set(value) {
            controller.beforeDownload = value
        }
    internal var beforeWatchRecovery: () -> Unit
        get() = controller.beforeWatchRecovery
        set(value) {
            controller.beforeWatchRecovery = value
        }
    internal var beforeWatchRefresh: suspend () -> Unit
        get() = controller.beforeWatchRefresh
        set(value) {
            controller.beforeWatchRefresh = value
        }
    internal var onWatchFinished: () -> Unit
        get() = controller.onWatchFinished
        set(value) {
            controller.onWatchFinished = value
        }
    internal var beforeSupportAdmission: suspend (Boolean, Long) -> Unit
        get() = controller.beforeSupportAdmission
        set(value) {
            controller.beforeSupportAdmission = value
        }

    @Composable fun Composer(conversationId: String?) {
        val currentChoice = controller.choice.collectAsState().value
        var presentedChoice by remember(conversationId) { mutableStateOf<AttachmentChoiceState?>(null) }
        val keyboard = LocalSoftwareKeyboardController.current
        LaunchedEffect(currentChoice) {
            if (currentChoice != null && currentChoice.conversationId == conversationId) {
                presentedChoice = currentChoice
                keyboard?.hide()
            }
        }
        AttachmentComposer(
            controller.drafts.collectAsState().value,
            conversationId,
            controller.error
                .collectAsState()
                .value
                ?.message,
            controller::dispatch,
        )
        val presented = presentedChoice
        if (presented != null && presented.conversationId == conversationId) {
            AttachmentChoiceSheet(
                presented,
                visible = currentChoice?.instanceId == presented.instanceId,
                onHidden = {
                    if (presentedChoice?.instanceId == presented.instanceId) presentedChoice = null
                },
                action = controller::dispatch,
            )
        }
    }

    @Composable fun Message(
        row: MessageRow,
        conversationId: String?,
        onLongPress: (() -> Unit)? = null,
    ) {
        if (!row.attachment || row.deleted) return
        DisposableEffect(row.id, conversationId) {
            val release = controller.display(row.id, row.mine)
            onDispose { release() }
        }
        val cached = controller.downloads.collectAsState().value[row.id]
        val retained = controller.verified.collectAsState().value
        val state =
            cached?.takeIf { !it.canOpen || row.id in retained }
                ?: AttachmentCardState(row.id, row.text, "File", canDownload = true)
        val bitmap = controller.previews.collectAsState().value[row.id]
        AttachmentMessage(
            state,
            bitmap?.asImageBitmap(),
            row.sender,
            "${row.day} ${row.time}",
            controller::dispatch,
            onLongPress,
            row.sendState.label.takeIf { row.mine },
        )
    }

    @Composable fun Actions(
        row: MessageRow,
        enabled: Boolean,
        dismiss: () -> Unit,
    ) {
        val cached = controller.downloads.collectAsState().value[row.id]
        val retained = controller.verified.collectAsState().value
        val state =
            cached?.takeIf { !it.canOpen || row.id in retained }
                ?: AttachmentCardState(row.id, row.text, "File", canDownload = true)
        AttachmentMessageActions(
            state,
            enabled,
            controller::dispatch,
            dismiss,
            row.sender,
            "${row.day} ${row.time}",
            row.sendState.label.takeIf { row.mine },
        )
    }

    @Composable fun Recovery(conversations: List<ConversationRow>) {
        AttachmentRecovery(
            controller.drafts.collectAsState().value,
            conversations,
            controller.error
                .collectAsState()
                .value
                ?.message,
            controller::dispatch,
        )
    }
}
