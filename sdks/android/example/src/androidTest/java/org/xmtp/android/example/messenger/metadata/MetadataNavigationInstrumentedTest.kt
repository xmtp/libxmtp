package org.xmtp.android.example.messenger.metadata

import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.lifecycle.ViewModelProvider
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.*
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.xmtp.android.example.MainActivity
import org.xmtp.android.example.messenger.MessengerViewModel
import org.xmtp.android.example.shared.*
import uniffi.xmtp_sdk.*
import java.util.UUID

/** Check app navigation, shared controls, event refresh and profile reopen. */
class MetadataNavigationInstrumentedTest {
    @get:Rule val compose = createAndroidComposeRule<MainActivity>()
    private val model get() = ViewModelProvider(compose.activity)[MessengerViewModel::class.java]

    private suspend fun until(check: () -> Boolean) = withTimeout(30_000) { while (!check()) delay(20) }

    private fun ref(value: Int) = MetadataFieldRef(value.toUShort(), null)

    private fun input(
        tag: String,
        value: String,
    ) {
        compose.onNodeWithTag("metadata-fields").performScrollToNode(hasTestTag(tag))
        compose.onNodeWithTag(tag).performScrollTo().performTextReplacement(value)
    }

    private suspend fun entry(
        id: Int,
        key: String,
        value: String = "",
        update: Boolean = false,
    ) {
        input("metadata-key-$id", key)
        if (id == 49155) input("metadata-entry-value-$id", value)
        androidx.test.espresso.Espresso
            .closeSoftKeyboard()
        val verb = if (update) "update" else "add"
        compose.onNodeWithTag("metadata-$verb-$id").performScrollTo().performClick()
        until {
            !model.metadataState.value.busy &&
                model.metadataState.value.fields
                    .single { it.id.componentId.toInt() == id }
                    .entries
                    .any { it.key == key && it.value == value }
        }
    }

    private suspend fun deleteEntry(
        id: Int,
        key: String,
    ) {
        val tag = "metadata-delete-$id-$key"
        compose.onNodeWithTag("metadata-fields").performScrollToNode(hasTestTag(tag))
        compose.onNodeWithTag(tag).performScrollTo().performClick()
        until {
            !model.metadataState.value.busy &&
                model.metadataState.value.fields
                    .single { it.id.componentId.toInt() == id }
                    .entries
                    .none { it.key == key }
        }
    }

    @Test fun screensSaveAndRefreshConversationValues() =
        runBlocking {
            val url = checkNotNull(InstrumentationRegistry.getArguments().getString("metadataBackendUrl"))
            val context = compose.activity.applicationContext
            val peerPath = context.filesDir.resolve("metadata-peer/${UUID.randomUUID()}")
            val lifecycle = AndroidStreamLifecycle.enabled
            var peer: SDKClient? = null
            try {
                AndroidStreamLifecycle.enabled = false
                resumeStreams()
                model.session.signOut()
                model.session.connect(url, "", false)
                val owner = checkNotNull(model.session.active.value)
                until { model.state.value.inbox == owner.client.inboxId() }
                peer =
                    SDKClient.create(
                        context,
                        generateLocalSigner(),
                        ClientOptions(
                            backend = BackendSource.Options(BackendOptions(url = url)),
                            storage = StorageOptions(location = StorageLocation.Directory(peerPath.absolutePath)),
                        ),
                    )
                val group = owner.client.conversations.createGroup(listOf(peer.inboxId()))
                peer.conversations.sync()
                val other = checkNotNull(peer.conversations.getById(group.id()))
                model.dispatch(MessengerAction.OpenConversation(group.id()))
                until { model.state.value.conversationId == group.id() && !model.state.value.busy }
                compose.onNodeWithContentDescription("Conversation settings").performClick()
                until { model.state.value.screen == Screen.CONVERSATION_SETTINGS && model.state.value.settings.group }
                compose.onNodeWithText("Group fields").performScrollTo().performClick()
                until {
                    !model.metadataState.value.busy &&
                        model.metadataState.value.fields
                            .any { it.id.componentId == 0xc001.toUShort() }
                }
                compose.onNodeWithTag("metadata-value-49153").performTextReplacement("UI title")
                androidx.test.espresso.Espresso
                    .closeSoftKeyboard()
                compose.onNodeWithTag("metadata-set-49153").performScrollTo().performClick()
                until {
                    !model.metadataState.value.busy &&
                        model.metadataState.value.fields
                            .any { it.scalar == "UI title" }
                }
                other.sync()
                assertEquals(MetadataValue.Scalar(FieldValue.String("UI title")), other.metadataValue(ref(0xc001)))
                input("metadata-value-49154", "0080ff")
                androidx.test.espresso.Espresso
                    .closeSoftKeyboard()
                compose.onNodeWithTag("metadata-set-49154").performScrollTo().performClick()
                until {
                    !model.metadataState.value.busy &&
                        model.metadataState.value.fields
                            .any { it.scalar == "0080ff" }
                }
                entry(49155, "01", "00ff")
                entry(49155, "02", "7f")
                entry(49155, "01", "80", update = true)
                deleteEntry(49155, "01")
                entry(49156, "00ff")
                entry(49156, "80")
                deleteEntry(49156, "80")
                entry(49157, peer.inboxId())
                entry(49157, owner.client.inboxId())
                deleteEntry(49157, owner.client.inboxId())
                other.sync()
                val bytes = ((other.metadataValue(ref(0xc002)) as MetadataValue.Scalar).v1 as FieldValue.Bytes).v1
                assertArrayEquals(byteArrayOf(0, -128, -1), bytes)
                assertNull(other.mapValue(ref(0xc003), FieldKey.Bytes(byteArrayOf(1))))
                assertArrayEquals(
                    byteArrayOf(127),
                    (
                        other.mapValue(
                            ref(0xc003),
                            FieldKey.Bytes(byteArrayOf(2)),
                        ) as FieldValue.Bytes
                    ).v1,
                )
                val keys = (other.metadataValue(ref(0xc004)) as MetadataValue.Set).v1
                assertArrayEquals(byteArrayOf(0, -1), (keys.single() as FieldKey.Bytes).v1)
                assertEquals(
                    listOf(FieldKey.InboxId(peer.inboxId())),
                    (other.metadataValue(ref(0xc005)) as MetadataValue.Set).v1,
                )
                other.updateMetadataField(ref(0xc001), ComponentMutation.Replace(FieldValue.String("Peer title")))
                until {
                    !model.metadataState.value.busy &&
                        model.metadataState.value.fields
                            .any { it.scalar == "Peer title" }
                }
                println("METADATA_UI event refreshed=Peer title")

                model.dispatch(MessengerAction.Navigate(Screen.CONVERSATION_SETTINGS))
                until { model.state.value.screen == Screen.CONVERSATION_SETTINGS && !model.state.value.busy }
                group.updateUserData(listOf(UserFieldUpdate(ref(0x800c), FieldValue.String("Old name"))))
                compose.onNodeWithText("My fields").performScrollTo().performClick()
                until {
                    !model.metadataState.value.busy &&
                        model.metadataState.value.fields
                            .any { it.id.componentId == 0xc006.toUShort() }
                }
                compose.onNodeWithTag("metadata-fields").performScrollToNode(hasTestTag("metadata-value-49158"))
                compose.onNodeWithTag("metadata-value-49158").performTextReplacement("UI own note")
                group.updateUserData(listOf(UserFieldUpdate(ref(0x800c), FieldValue.String("New name"))))
                until {
                    model.metadataState.value.fields
                        .any { it.scalar == "New name" }
                }
                compose.onNodeWithTag("metadata-value-49158").assertTextContains("UI own note")
                androidx.test.espresso.Espresso
                    .closeSoftKeyboard()
                compose.onNodeWithText("Save changed fields").performScrollTo().performClick()
                until {
                    !model.metadataState.value.busy &&
                        model.metadataState.value.fields
                            .any { it.scalar == "UI own note" }
                }
                other.sync()
                assertEquals(
                    FieldValue.String("UI own note"),
                    other
                        .userData(
                            listOf(ref(0xc006)),
                            listOf(owner.client.inboxId()),
                        ).getValue(owner.client.inboxId())
                        .single()
                        .value,
                )
                assertEquals(
                    FieldValue.String("New name"),
                    other
                        .userData(
                            listOf(ref(0x800c)),
                            listOf(owner.client.inboxId()),
                        ).getValue(owner.client.inboxId())
                        .single()
                        .value,
                )
                val dm = owner.client.conversations.createDm(peer.inboxId())
                peer.conversations.sync()
                val otherDm = checkNotNull(peer.conversations.getById(dm.id()))
                model.dispatch(MessengerAction.OpenConversation(dm.id()))
                until { model.state.value.conversationId == dm.id() && !model.state.value.busy }
                compose.onNodeWithContentDescription("Conversation settings").performClick()
                until {
                    model.state.value.screen == Screen.CONVERSATION_SETTINGS &&
                        !model.state.value.settings.group && !model.state.value.busy
                }
                compose.onNodeWithText("Group fields").assertDoesNotExist()
                compose.onNodeWithText("My fields").performScrollTo().performClick()
                until {
                    !model.metadataState.value.busy &&
                        model.metadataState.value.fields
                            .any { it.id.componentId == 0xc006.toUShort() }
                }
                compose.onNodeWithTag("metadata-fields").performScrollToNode(hasTestTag("metadata-value-49158"))
                compose.onNodeWithTag("metadata-value-49158").performTextReplacement("DM own note")
                compose.onNodeWithTag("metadata-fields").performScrollToNode(hasTestTag("metadata-value-49159"))
                compose.onNodeWithTag("metadata-value-49159").performTextReplacement("00ff")
                androidx.test.espresso.Espresso
                    .closeSoftKeyboard()
                compose.onNodeWithText("Save changed fields").performScrollTo().performClick()
                until {
                    !model.metadataState.value.busy &&
                        model.metadataState.value.fields
                            .any { it.scalar == "DM own note" }
                }
                otherDm.sync()
                val dmFields =
                    otherDm
                        .userData(
                            listOf(ref(0xc006), ref(0xc007)),
                            listOf(owner.client.inboxId()),
                        ).getValue(owner.client.inboxId())
                        .associate { it.field.componentId to it.value }
                assertEquals(FieldValue.String("DM own note"), dmFields[0xc006.toUShort()])
                assertArrayEquals(byteArrayOf(0, -1), (dmFields[0xc007.toUShort()] as FieldValue.Bytes).v1)
                other.sync()
                assertEquals(
                    FieldValue.String("UI own note"),
                    other
                        .userData(
                            listOf(ref(0xc006)),
                            listOf(owner.client.inboxId()),
                        ).getValue(owner.client.inboxId())
                        .single()
                        .value,
                )
                val own = owner.client.inboxId()
                model.session.signOut()
                until { model.state.value.screen == Screen.START }
                assertTrue(
                    model.metadataState.value.fields
                        .isEmpty(),
                )
                model.session.connect(url, "", false)
                until { model.state.value.inbox == own }
                model.dispatch(MessengerAction.OpenConversation(dm.id()))
                until { model.state.value.conversationId == dm.id() && !model.state.value.busy }
                model.dispatch(MessengerAction.Navigate(Screen.MY_FIELDS))
                until {
                    !model.metadataState.value.busy &&
                        model.metadataState.value.fields
                            .any { it.scalar == "DM own note" }
                }
                assertNull(model.metadataState.value.error)
                println("METADATA_UI native group=UI own note dm=DM own note:00ff reopen=retained")
            } finally {
                withContext(NonCancellable) {
                    peer?.end()
                    peerPath.deleteRecursively()
                    if (model.session.active.value != null) model.session.deleteAccount()
                    model.session.signOut()
                    AndroidStreamLifecycle.enabled = lifecycle
                }
            }
        }
}
