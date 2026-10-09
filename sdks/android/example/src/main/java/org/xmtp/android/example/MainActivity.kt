package org.xmtp.android.example

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.viewModels
import androidx.compose.runtime.key
import androidx.lifecycle.compose.collectAsStateWithLifecycle
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

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        attachments = AttachmentHost(this, model)
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
