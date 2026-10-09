package org.xmtp.android.example.messenger
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.*
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.xmtp.android.example.BuildConfig
import uniffi.xmtp_sdk.*
import java.io.File
import java.security.SecureRandom
import java.util.UUID

@RunWith(AndroidJUnit4::class)
class MessengerActionsInstrumentedTest {
    private val context get() =
        InstrumentationRegistry
            .getInstrumentation()
            .targetContext.applicationContext

    private suspend fun until(
        stage: String,
        check: suspend () -> Boolean,
    ) {
        assertTrue(
            "Timed out at $stage",
            withTimeoutOrNull(30_000) {
                while (!check()) delay(50)
                true
            } == true,
        )
        android.util.Log
            .i(
                "MessengerProof",
                stage,
            )
    }

    @Test fun acceptedIdTextReplyReactionRetryAndDeleteUseOneStoredRow() =
        runBlocking {
            AndroidStreamLifecycle.enabled = false
            resumeStreams()
            val session = AppSession(context)
            var peer: SDKClient? = null
            var peerReader: Job? = null
            try {
                session
                    .connect(
                        BuildConfig.XMTP_BACKEND_URL,
                        "",
                        true,
                    )
                val owner =
                    checkNotNull(
                        session.active.value,
                    )
                peer =
                    SDKClient
                        .create(
                            context,
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
                val chat =
                    Conversation
                        .Group(
                            owner.client.conversations
                                .createGroup(
                                    listOf(
                                        second
                                            .inboxId(),
                                    ),
                                    CreateGroupOptions(
                                        name = "Before",
                                        permissions =
                                            GroupPermissionMode.AllMembers,
                                    ),
                                ),
                        )
                val coordinator =
                    SendCoordinator(
                        session.preferences,
                        session::accepts,
                    )
                var queues = 0
                val id =
                    coordinator
                        .queue(
                            owner.key,
                            owner.client,
                            chat,
                            reconcile = {
                            },
                        ) {
                            queues += 1
                            chat
                                .sendText(
                                    "one text",
                                    SendOptions(optimistic = true),
                                )
                        }
                until("text publication") {
                    owner.client.conversations
                        .getMessageById(id)
                        ?.deliveryStatus ==
                        DeliveryStatus.PUBLISHED
                }
                coordinator
                    .retry(
                        owner.key,
                        owner.client,
                        chat,
                        id,
                    ) {
                    }
                assertEquals(
                    1,
                    queues,
                )
                assertEquals(1uL, chat.countMessages(publishedSelection()))
                assertEquals(
                    1,
                    chat
                        .messages()
                        .count {
                            it.id == id
                        },
                )
                val parent =
                    checkNotNull(
                        owner.client.conversations
                            .getMessageById(id),
                    )
                val reply =
                    coordinator
                        .queue(
                            owner.key,
                            owner.client,
                            chat,
                            reconcile = {
                            },
                        ) {
                            parent
                                .reply(
                                    "one reply",
                                    SendOptions(optimistic = true),
                                )
                        }
                val reaction =
                    coordinator
                        .queue(
                            owner.key,
                            owner.client,
                            chat,
                            reconcile = {
                            },
                        ) {
                            parent
                                .react(
                                    Reaction(
                                        "👍",
                                        ReactionAction.ADDED,
                                        ReactionSchema.UNICODE,
                                    ),
                                    SendOptions(optimistic = true),
                                )
                        }
                until("reaction add") {
                    checkNotNull(
                        owner.client.conversations
                            .getMessageById(id),
                    ).reactions.any {
                        it.reaction.content == "👍"
                    }
                }
                coordinator
                    .queue(
                        owner.key,
                        owner.client,
                        chat,
                        reconcile = {
                        },
                    ) {
                        parent
                            .react(
                                Reaction(
                                    "👍",
                                    ReactionAction.REMOVED,
                                    ReactionSchema.UNICODE,
                                ),
                                SendOptions(optimistic = true),
                            )
                    }
                until("reaction removal") {
                    checkNotNull(
                        owner.client.conversations
                            .getMessageById(id),
                    ).toRow(
                        owner.client
                            .inboxId(),
                    ).reactions
                        .isEmpty()
                }
                assertTrue(
                    checkNotNull(
                        owner.client.conversations
                            .getMessageById(id),
                    ).reactions.any {
                        it.reaction.action ==
                            ReactionAction.REMOVED
                    },
                )
                assertEquals(
                    id,
                    owner.client.conversations
                        .getMessageById(reply)
                        ?.inReplyTo
                        ?.id,
                )
                val group =
                    (
                        chat as Conversation.Group
                    ).group
                group
                    .updateName("After")
                group
                    .updateDescription("Current description")
                assertEquals(
                    "After",
                    group
                        .state()
                        .name,
                )
                group
                    .addAdmin(
                        second
                            .inboxId(),
                    )
                assertTrue(
                    group
                        .state()
                        .admins
                        .contains(
                            second
                                .inboxId(),
                        ),
                )
                group
                    .removeAdmin(
                        second
                            .inboxId(),
                    )
                assertFalse(
                    group
                        .state()
                        .admins
                        .contains(
                            second
                                .inboxId(),
                        ),
                )
                applyStandardPreset(
                    group,
                    true,
                )
                assertEquals(
                    GroupPolicyType.ADMIN_ONLY,
                    group
                        .state()
                        .permissions.policyType,
                )
                applyStandardPreset(
                    group,
                    false,
                )
                assertEquals(
                    GroupPolicyType.ALL_MEMBERS,
                    group
                        .state()
                        .permissions.policyType,
                )
                var permissionWrites = 0
                val failedPreset =
                    runCatching {
                        standardPresetWrites(true) {
                            kind,
                            policy,
                            field,
                            ->
                            permissionWrites += 1
                            group
                                .updatePermission(
                                    kind,
                                    if (permissionWrites == 2) {
                                        PermissionPolicy.OTHER
                                    } else {
                                        policy
                                    },
                                    field,
                                )
                        }
                    }.exceptionOrNull()
                assertNotNull(failedPreset)
                assertEquals(
                    2,
                    permissionWrites,
                )
                assertEquals(
                    GroupPolicyType.CUSTOM,
                    group
                        .state()
                        .permissions.policyType,
                )
                applyStandardPreset(
                    group,
                    false,
                )
                group
                    .updateDisappearingSettings(
                        DisappearingSettings(
                            Timestamp(
                                System
                                    .currentTimeMillis() * 1_000_000,
                            ),
                            1_000_000_000L,
                        ),
                    )
                assertTrue(
                    group
                        .state()
                        .common.isDisappearingEnabled,
                )
                group
                    .updateDisappearingSettings(null)
                group
                    .updateDisappearingSettings(
                        DisappearingSettings(
                            Timestamp(
                                System
                                    .currentTimeMillis() * 1_000_000,
                            ),
                            5_000_000_000L,
                        ),
                    )
                val expiring =
                    coordinator
                        .queue(
                            owner.key,
                            owner.client,
                            chat,
                            reconcile = {
                            },
                        ) {
                            chat
                                .sendText(
                                    "short retention",
                                    SendOptions(optimistic = true),
                                )
                        }
                until("expiry hides retained content") {
                    owner.client.conversations
                        .getMessageById(expiring) == null
                }
                assertFalse(
                    chat
                        .messages(publishedSelection())
                        .any {
                            it.id == expiring
                        },
                )
                group
                    .updateDisappearingSettings(null)
                until("peer group join") {
                    second.conversations
                        .getById(
                            group
                                .id(),
                        ) != null
                }
                val peerGroup =
                    (
                        checkNotNull(
                            second.conversations
                                .getById(
                                    group
                                        .id(),
                                ),
                        ) as Conversation.Group
                    ).group
                peerGroup
                    .requestRemoval()
                assertEquals(
                    MembershipState.PENDING_REMOVE,
                    peerGroup
                        .state()
                        .membershipState,
                )
                assertTrue(
                    peerGroup
                        .members()
                        .any {
                            it.inboxId ==
                                second
                                    .inboxId()
                        },
                )
                chat
                    .deleteMessage(id)
                until("message deletion") {
                    owner.client.conversations
                        .getMessageById(id)
                        ?.toRow(
                            owner.client
                                .inboxId(),
                        )?.deleted == true
                }
                val deleted =
                    owner.client.conversations
                        .getMessageById(id)
                        ?.toRow(
                            owner.client
                                .inboxId(),
                        )
                assertEquals(
                    "Message deleted",
                    deleted?.text,
                )
            } finally {
                peerReader?.cancelAndJoin()
                withContext(NonCancellable) {
                    peer?.end()
                    session
                        .deleteAccount()
                }
                AndroidStreamLifecycle.enabled = true
            }
        }
}
