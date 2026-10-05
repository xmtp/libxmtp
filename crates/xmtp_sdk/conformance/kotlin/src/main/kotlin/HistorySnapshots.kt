import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.async
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeout
import uniffi.xmtp_sdk.*

// verifies: DMS-016
internal suspend fun checkHistorySnapshots(backend: BackendOptions) =
    coroutineScope {
        withTimeout(60_000) {
            val options =
                ClientOptions(
                    backend = BackendSource.Options(backend),
                    storage = StorageOptions(location = StorageLocation.InMemory),
                    deviceSync = false,
                )
            val sender = SDKClient.create(generateLocalSigner(), options)
            try {
                val receiver = SDKClient.create(generateLocalSigner(), options)
                try {
                    val group = sender.conversations().createGroup(listOf(receiver.inboxId()))
                    group.sendText("older history")
                    val recent = group.sendText("recent history")
                    val dm = sender.conversations().createDm(receiver.inboxId())
                    val direct = dm.sendText("direct history")
                    receiver.conversations().syncAll(null)
                    val storedGroup =
                        (
                            checkNotNull(
                                receiver.conversations().getById(group.id()),
                            ) as Conversation.Group
                        ).group
                    val storedDm = (checkNotNull(receiver.conversations().getById(dm.id())) as Conversation.Dm).dm
                    val snapshot = storedGroup.messageHistorySnapshot(1u)
                    check(snapshot.messages.map { it.id } == listOf(recent))
                    check(snapshot.messages.all { it.deliveryCursor != null })
                    check(snapshot.cursor.isNotEmpty())
                    check(storedDm.messageHistorySnapshot(1u).messages.map { it.id } == listOf(direct))
                    val collection = receiver.conversations().messageHistorySnapshot(20u)
                    check(collection.messages.map { it.id }.containsAll(listOf(recent, direct)))
                    val groups =
                        receiver.conversations().messageHistorySnapshot(
                            20u,
                            MessageReaderOptions(conversationKind = ConversationKind.GROUP),
                        )
                    check(groups.messages.isNotEmpty() && groups.messages.all { it.conversationId == group.id() })
                    check(storedGroup.messageHistorySnapshot(0u).messages.isEmpty())
                    check(
                        runCatching {
                            receiver.conversations().messageHistorySnapshot(
                                1u,
                                MessageReaderOptions(from = collection.cursor),
                            )
                        }.exceptionOrNull() is XmtpException.InvalidArgument,
                    )
                    val next =
                        async {
                            receiver
                                .messages(
                                    storedGroup,
                                    ConversationMessageReaderOptions(from = snapshot.cursor),
                                ).first()
                        }
                    val after = group.sendText("after snapshot")
                    check(next.await().id == after)
                } finally {
                    withContext(NonCancellable) { receiver.end() }
                }
            } finally {
                withContext(NonCancellable) { sender.end() }
            }
        }
        println("Kotlin history snapshots keep recent messages, selection, and the atomic replay boundary")
    }
