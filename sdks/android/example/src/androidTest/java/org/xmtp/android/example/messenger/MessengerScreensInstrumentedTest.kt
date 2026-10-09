package org.xmtp.android.example.messenger
import android.content.ContentValues
import android.graphics.Bitmap
import android.provider.MediaStore
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.*
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.xmtp.android.example.BuildConfig
import org.xmtp.android.example.ExampleApp
import org.xmtp.android.example.MainActivity
import uniffi.xmtp_sdk.*
import java.security.SecureRandom

class MessengerScreensInstrumentedTest {
    @get:Rule val compose = createAndroidComposeRule<MainActivity>()

    private fun screen(name: String) {
        androidx.test.espresso.Espresso
            .closeSoftKeyboard()
        compose
            .waitForIdle()
        val bitmap =
            checkNotNull(
                InstrumentationRegistry
                    .getInstrumentation()
                    .uiAutomation
                    .takeScreenshot(),
            )
        val resolver =
            compose.activity.contentResolver
        val values =
            ContentValues().apply {
                put(
                    MediaStore.Images.Media.DISPLAY_NAME,
                    "$name.png",
                )
                put(
                    MediaStore.Images.Media.MIME_TYPE,
                    "image/png",
                )
                put(
                    MediaStore.Images.Media.RELATIVE_PATH,
                    "Pictures/XmtpMessengerProof",
                )
                put(
                    MediaStore.Images.Media.IS_PENDING,
                    1,
                )
            }
        val uri =
            checkNotNull(
                resolver
                    .insert(
                        MediaStore.Images.Media.EXTERNAL_CONTENT_URI,
                        values,
                    ),
            )
        checkNotNull(
            resolver
                .openOutputStream(uri),
        ).use {
            bitmap
                .compress(
                    Bitmap.CompressFormat.PNG,
                    100,
                    it,
                )
        }
        resolver
            .update(
                uri,
                ContentValues().apply {
                    put(
                        MediaStore.Images.Media.IS_PENDING,
                        0,
                    )
                },
                null,
                null,
            )
        bitmap
            .recycle()
    }

    private fun visible(text: String) {
        compose
            .waitUntil(30_000) {
                compose
                    .onAllNodesWithText(text)
                    .fetchSemanticsNodes()
                    .isNotEmpty()
            }
    }

    @Test fun realSessionRendersSetupListMessagesAndSettings() =
        runBlocking {
            val app =
                compose.activity
                    .application as ExampleApp
            val session =
                app.session
            AndroidStreamLifecycle.enabled = false
            resumeStreams()
            var peer: SDKClient? = null
            var peerReader: Job? = null
            try {
                compose
                    .onNodeWithContentDescription("XMTP logo")
                    .assertExists()
                screen("setup")
                compose
                    .onNodeWithText("Connect")
                    .performClick()
                compose
                    .waitUntil(30_000) {
                        session.active.value != null
                    }
                val owner =
                    checkNotNull(
                        session.active.value,
                    )
                peer =
                    SDKClient
                        .create(
                            app,
                            localSignerFromPrivateKey(
                                SecureRandom()
                                    .generateSeed(32),
                            ),
                            ClientOptions(
                                backend =
                                    BackendSource
                                        .Options(
                                            BackendOptions(
                                                url =
                                                    BuildConfig.XMTP_BACKEND_URL,
                                            ),
                                        ),
                                storage =
                                    StorageOptions(
                                        location =
                                            StorageLocation.InMemory,
                                    ),
                                deviceSync = false,
                            ),
                        )
                val second = checkNotNull(peer)
                peerReader =
                    launch {
                        second.conversations
                            .streamAllMessages()
                            .collect {
                            }
                    }
                val group =
                    owner.client.conversations
                        .createGroup(
                            listOf(
                                second
                                    .inboxId(),
                            ),
                            CreateGroupOptions(
                                name = "Navy proof group",
                                description = "Current SDK group",
                                permissions =
                                    GroupPermissionMode.AllMembers,
                            ),
                        )
                visible("Navy proof group")
                screen("conversations")
                compose.onNodeWithContentDescription("New conversation").performClick()
                screen("create")
                compose.onAllNodesWithText("Group").onFirst().performClick()
                screen("create-group")
                compose.onNodeWithContentDescription("Back").performClick()
                compose
                    .onNodeWithText("Navy proof group")
                    .performClick()
                withTimeout(30_000) {
                    while (second.conversations
                            .getById(
                                group
                                    .id(),
                            ) == null
                    ) {
                        delay(50)
                    }
                }
                checkNotNull(
                    second.conversations
                        .getById(
                            group
                                .id(),
                        ),
                ).sendText("Hello from the other member")
                visible("Hello from the other member")
                compose
                    .onNodeWithText("Message")
                    .performTextInput("Hello from this device")
                compose
                    .onNodeWithText("Send")
                    .performClick()
                visible("Hello from this device")
                compose
                    .onNodeWithText("Hello from the other member")
                    .performClick()
                compose
                    .onNodeWithText("Reply")
                    .performClick()
                compose
                    .onNodeWithText("Message")
                    .performTextInput("A reply from this device")
                compose
                    .onNodeWithText("Send")
                    .performClick()
                visible("A reply from this device")
                screen("messages")
                compose
                    .onNodeWithContentDescription("Conversation settings")
                    .performClick()
                visible("Members")
                screen("settings")
                compose.onNodeWithContentDescription("Back").performClick()
                compose.onNodeWithContentDescription("Back").performClick()
                compose.onNodeWithContentDescription("Settings").performClick()
                screen("app-settings")
                assertEquals(
                    "Navy proof group",
                    group
                        .state()
                        .name,
                )
            } finally {
                peerReader?.cancelAndJoin()
                withContext(NonCancellable) {
                    peer?.end()
                    if (session.active.value !=
                        null
                    ) {
                        session
                            .deleteAccount()
                    }
                }
                AndroidStreamLifecycle.enabled = true
            }
        }
}
