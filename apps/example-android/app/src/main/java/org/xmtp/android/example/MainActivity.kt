package org.xmtp.android.example

import android.Manifest
import android.content.Intent
import android.os.Build
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.activity.viewModels
import androidx.compose.runtime.key
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.lifecycleScope
import kotlinx.coroutines.launch
import org.xmtp.android.example.messenger.MessengerViewModel
import org.xmtp.android.example.messenger.ReactionPickerHost
import org.xmtp.android.example.messenger.attachments.AttachmentHost
import org.xmtp.android.example.shared.MessengerScreens
import org.xmtp.android.example.shared.Screen
import org.xmtp.android.example.shared.metadata.MetadataScreen

class MainActivity : ComponentActivity() {
    private val model: MessengerViewModel by viewModels()
    internal lateinit var attachments: AttachmentHost
        private set

    private val notificationPermission =
        registerForActivityResult(ActivityResultContracts.RequestPermission()) {
            model.notifications.permissionChanged()
        }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        attachments = AttachmentHost(this, model)
        lifecycleScope.launch {
            model.notifications.permissionRequest.collect { request ->
                requestNotificationPermission(request)
            }
        }
        lifecycleScope.launch { model.openPush(intent) }
        setContent {
            val state = model.state.collectAsStateWithLifecycle().value
            ReactionPickerHost(state, model::dispatch) { action ->
                MessengerScreens(
                    state,
                    action,
                    extraScreen = { screen ->
                        when (screen) {
                            Screen.DRAFTS -> {
                                attachments.Recovery()
                            }

                            Screen.GROUP_FIELDS, Screen.MY_FIELDS -> {
                                key(model.screenToken()) {
                                    MetadataScreen(
                                        model.metadataState.collectAsStateWithLifecycle().value,
                                        screen == Screen.MY_FIELDS,
                                        model::editMetadata,
                                    )
                                }
                            }

                            else -> {
                                Unit
                            }
                        }
                    },
                    composerExtra = { attachments.Composer() },
                    messageExtra = { row -> attachments.Message(row) },
                )
            }
        }
    }

    private fun requestNotificationPermission(request: Long) {
        if (request <= 0 || !model.notifications.configured || Build.VERSION.SDK_INT < 33) return
        val owner = model.session.active.value ?: return
        model.session.withCurrent(owner.key) {
            if (model.notifications.enabled.value && model.notifications.permissionRequest.value == request) {
                model.notifications.permissionRequested()
                notificationPermission.launch(Manifest.permission.POST_NOTIFICATIONS)
            }
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        lifecycleScope.launch { model.openPush(intent) }
    }

    override fun onResume() {
        super.onResume()
        model.foreground(true)
    }

    override fun onPause() {
        model.foreground(false)
        super.onPause()
    }

    override fun onDestroy() {
        attachments.close()
        super.onDestroy()
    }
}
