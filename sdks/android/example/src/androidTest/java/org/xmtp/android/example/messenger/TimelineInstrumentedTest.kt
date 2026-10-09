package org.xmtp.android.example.messenger
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.*
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.xmtp.android.example.BuildConfig
import org.xmtp.android.example.shared.MessageRow
import org.xmtp.android.example.shared.ScrollAnchor
import uniffi.xmtp_sdk.*
import java.security.SecureRandom

@RunWith(AndroidJUnit4::class)
class TimelineInstrumentedTest {
    @Test fun directCollectorReconcilesIdsAndUnreadUsesInsertionTime() =
        runBlocking {
            val context =
                InstrumentationRegistry
                    .getInstrumentation()
                    .targetContext.applicationContext
            AndroidStreamLifecycle.enabled = false
            resumeStreams()
            val session = AppSession(context)
            var peer: SDKClient? = null
            var reader: Job? = null
            val received =
                java.util.concurrent.ConcurrentHashMap<
                    String,
                    Message,
                >()
            session.onMessage = {
                _,
                message,
                ->
                received[
                    message.id,
                ] = message
            }
            try {
                session
                    .connect(
                        BuildConfig.XMTP_BACKEND_URL,
                        "",
                        false,
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
                reader =
                    launch {
                        second.conversations
                            .streamAllMessages()
                            .collect {
                            }
                    }
                val peerChat =
                    second.conversations
                        .createDm(
                            owner.client
                                .inboxId(),
                        )
                val id =
                    peerChat
                        .sendText("incoming")
                withTimeout(30_000) {
                    while (!received
                            .containsKey(id)
                    ) {
                        delay(50)
                    }
                }
                val chat =
                    checkNotNull(
                        owner.client.conversations
                            .getById(
                                checkNotNull(received[id]).conversationId,
                            ),
                    )
                assertEquals(
                    1uL,
                    chat
                        .countMessages(
                            incomingSelection(
                                owner.client
                                    .inboxId(),
                            ),
                        ),
                )
                val inserted =
                    checkNotNull(received[id]).insertedAt.ns
                assertEquals(
                    0uL,
                    chat
                        .countMessages(
                            incomingSelection(
                                owner.client
                                    .inboxId(),
                                inserted,
                            ),
                        ),
                )
                assertEquals(
                    inserted,
                    incomingSelection(
                        owner.client
                            .inboxId(),
                        inserted,
                    ).insertedAfter?.ns,
                )
                assertEquals(
                    1,
                    received.values.count {
                        it.id == id
                    },
                )
                val queried = CompletableDeferred<Unit>()
                val release = CompletableDeferred<Unit>()
                var foregroundNewest = true
                val logical =
                    logicalConversationKey(
                        chat,
                        owner.client
                            .inboxId(),
                    )
                val marking =
                    launch {
                        markLocalRead(
                            eligible = {
                                foregroundNewest &&
                                    session
                                        .accepts(
                                            owner.key,
                                        )
                            },
                            latestIncoming = {
                                val row =
                                    chat
                                        .messages(
                                            incomingSelection(
                                                owner.client
                                                    .inboxId(),
                                            ).copy(
                                                sortBy =
                                                    MessageSortBy.INSERTED_AT,
                                                limit = 1u,
                                            ),
                                        ).firstOrNull()
                                queried
                                    .complete(Unit)
                                release
                                    .await()
                                row?.insertedAt?.ns
                            },
                            currentMarker = {
                                session.preferences
                                    .marker(
                                        owner.key.profileId,
                                        logical,
                                    ).insertedAtNs
                            },
                            saveMarker = {
                                session.preferences
                                    .saveMarker(
                                        owner.key.profileId,
                                        logical,
                                        it,
                                    )
                            },
                        )
                    }
                queried
                    .await()
                foregroundNewest = false
                release
                    .complete(Unit)
                marking
                    .join()
                assertNull(
                    session.preferences
                        .marker(
                            owner.key.profileId,
                            logical,
                        ).insertedAtNs,
                )
                foregroundNewest = true
                markLocalRead(
                    {
                        foregroundNewest &&
                            session
                                .accepts(
                                    owner.key,
                                )
                    },
                    {
                        chat
                            .messages(
                                incomingSelection(
                                    owner.client
                                        .inboxId(),
                                ).copy(
                                    sortBy =
                                        MessageSortBy.INSERTED_AT,
                                    limit = 1u,
                                ),
                            ).firstOrNull()
                            ?.insertedAt
                            ?.ns
                    },
                    {
                        session.preferences
                            .marker(
                                owner.key.profileId,
                                logical,
                            ).insertedAtNs
                    },
                    {
                        session.preferences
                            .saveMarker(
                                owner.key.profileId,
                                logical,
                                it,
                            )
                    },
                )
                assertEquals(
                    inserted,
                    session.preferences
                        .marker(
                            owner.key.profileId,
                            logical,
                        ).insertedAtNs,
                )
                session
                    .retryReader()
                assertEquals(
                    1,
                    chat
                        .messages(publishedSelection())
                        .count {
                            it.id == id
                        },
                )
                val late =
                    peerChat
                        .sendText("late incoming message")
                withTimeout(30_000) {
                    while (!received
                            .containsKey(late)
                    ) {
                        delay(50)
                    }
                }
                val lateRow =
                    checkNotNull(
                        owner.client.conversations
                            .getMessageById(late),
                    ).toRow(
                        owner.client
                            .inboxId(),
                    )
                val saved =
                    ScrollAnchor(
                        late,
                        lateRow.sentAtNs,
                        17,
                        false,
                    )
                val cache =
                    TranscriptCache<MessageRow>(
                        {
                            it.id
                        },
                        {
                            it.sentAtNs
                        },
                    )
                cache
                    .put(
                        chat
                            .id(),
                        chat
                            .messages(publishedSelection())
                            .map {
                                it
                                    .toRow(
                                        owner.client
                                            .inboxId(),
                                    )
                            },
                    )
                assertEquals(
                    saved,
                    restoreAnchor(
                        saved,
                        checkNotNull(
                            cache
                                .get(
                                    chat
                                        .id(),
                                ),
                        ),
                    ).anchor,
                )
                repeat(3) { index ->
                    val other =
                        owner.client.conversations
                            .createGroup(
                                emptyList(),
                                CreateGroupOptions(name = "cache $index"),
                            )
                    cache
                        .put(
                            other
                                .id(),
                            other
                                .messages(publishedSelection())
                                .map {
                                    it
                                        .toRow(
                                            owner.client
                                                .inboxId(),
                                        )
                                },
                        )
                }
                assertNull(
                    cache
                        .get(
                            chat
                                .id(),
                        ),
                )
                checkNotNull(
                    owner.client.conversations
                        .getMessageById(late),
                ).deleteLocally()
                val bounded =
                    chat
                        .messages(
                            publishedSelection()
                                .copy(
                                    sentBefore =
                                        Timestamp(
                                            lateRow.sentAtNs + 1,
                                        ),
                                    limit = 51u,
                                ),
                        ).take(500)
                        .map {
                            it
                                .toRow(
                                    owner.client
                                        .inboxId(),
                                )
                        }
                val restored =
                    restoreAnchor(
                        saved,
                        bounded,
                    )
                assertTrue(
                    restored.changed,
                )
                assertEquals(
                    id,
                    restored.anchor?.messageId,
                )
                assertEquals(
                    0,
                    restored.anchor?.offsetPx,
                )
            } finally {
                reader?.cancelAndJoin()
                withContext(NonCancellable) {
                    peer?.end()
                    session
                        .deleteAccount()
                }
                AndroidStreamLifecycle.enabled = true
            }
        }
}
