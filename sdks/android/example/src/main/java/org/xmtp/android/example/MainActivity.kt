package org.xmtp.android.example

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.viewModels
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import org.xmtp.android.example.messenger.MessengerViewModel
import org.xmtp.android.example.shared.MessengerScreens

class MainActivity : ComponentActivity() {
    private val model: MessengerViewModel by viewModels()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            MessengerScreens(model.state.collectAsStateWithLifecycle().value, model::dispatch)
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
}
