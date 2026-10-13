package org.xmtp.android.example.messenger.feature.conversations

import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.sync.Semaphore
import kotlinx.coroutines.sync.withPermit
import org.xmtp.android.example.messenger.*
import org.xmtp.android.example.messenger.app.DestinationOrigin
import org.xmtp.android.example.messenger.app.DestinationScope
import org.xmtp.android.example.messenger.data.xmtp.ConversationQueries
import org.xmtp.android.example.messenger.feature.FeatureInputFailure
import org.xmtp.android.example.messenger.feature.featureFailure
import org.xmtp.android.example.shared.*
import org.xmtp.android.example.shared.feature.conversations.*
import uniffi.xmtp_sdk.*
import java.util.concurrent.atomic.AtomicLong

internal class CreateConversationController(
    private val session: AppSession,
    private val queries: ConversationQueries,
    private val reads: Semaphore,
    private val destinations: DestinationScope,
) {
    private val ui = MutableStateFlow(CreateConversationState())
    val state: StateFlow<CreateConversationState> = ui
    private val generation = AtomicLong()

    private data class CommandOwner(
        val key: SessionKey,
        val generation: Long,
    )

    private val busyCommands = mutableMapOf<Any, CommandOwner>()

    private fun hasBusyCommands(): Boolean {
        val key = session.active.value?.key
        val currentGeneration = generation.get()
        return busyCommands.values.any { it.key == key && it.generation == currentGeneration }
    }

    private var pastedIds = emptySet<String>()
    private var excludedPastedIds = emptySet<String>()

    fun clear() =
        session.withStateLock {
            generation.incrementAndGet()
            busyCommands.clear()
            pastedIds =
                emptySet()
            excludedPastedIds =
                emptySet()
            ui.value =
                CreateConversationState()
        }

    suspend fun open(group: Boolean = ui.value.group) {
        val owner = session.active.value ?: return
        val origin = destinations.capture(owner)
        destinations.admit(origin) {
            ui.update { it.copy(group = group, busy = hasBusyCommands()) }
        }
        val configuration = reads.withPermit { owner.client.serverConfiguration() }
        val known =
            queries.list(
                owner,
                ListConversationsOptions(
                    limit =
                    50u,
                    consentStates =
                        listOf(ConsentState.ALLOWED),
                ),
            )
        val recipients =
            coroutineScope {
                known
                    .take(20)
                    .map { conversation ->
                        async {
                            reads.withPermit {
                                conversation.members().take(50).map {
                                    RecipientUi(it.inboxId, it.handle ?: it.inboxId.take(12))
                                }
                            }
                        }
                    }.awaitAll()
                    .flatten()
                    .distinctBy {
                        it.inboxId
                    }.filterNot {
                        it.inboxId == owner.client.inboxId()
                    }.take(100)
            }
        if (destinations.accepts(origin)) {
            ui.update {
                it.copy(
                    knownRecipients =
                    recipients,
                    maxMembers =
                        configuration.mls.maxGroupMembers
                            .coerceAtMost(Int.MAX_VALUE.toULong())
                            .toInt(),
                )
            }
        }
    }

    fun dispatch(action: CreateConversationAction) {
        val route = destinations.state.value.route
        if (route != AppRoute.CreateConversation &&
            !(
                action is CreateConversationAction.ChangeMode &&
                    (
                        route == AppRoute.Chats ||
                            route == AppRoute.Requests
                    )
            )
        ) {
            return
        }
        when (action) {
            is CreateConversationAction.ChangeMode -> {
                generation.incrementAndGet()
                ui.update {
                    it.copy(
                        group =
                            action.group,
                        step =
                            CreateStep.RECIPIENTS,
                        busy =
                        false,
                        failure =
                        null,
                    )
                }
            }

            is CreateConversationAction.ChangeRecipients -> {
                generation.incrementAndGet()
                excludedPastedIds =
                    emptySet()
                ui.update {
                    it.copy(
                        recipients =
                            action.value,
                        busy =
                        false,
                        failure =
                        null,
                    )
                }
            }

            is CreateConversationAction.ToggleRecipient -> {
                generation.incrementAndGet()
                ui.update { current ->
                    val existing =
                        current.selectedRecipients.any {
                            it.inboxId == action.inboxId
                        }
                    if (existing) excludedPastedIds += action.inboxId else excludedPastedIds -= action.inboxId
                    current.copy(
                        busy =
                        false,
                        selectedRecipients =
                            if (existing) {
                                current.selectedRecipients.filterNot {
                                    it.inboxId == action.inboxId
                                }
                            } else {
                                current.selectedRecipients +
                                    (
                                        current.knownRecipients.firstOrNull {
                                            it.inboxId == action.inboxId
                                        } ?: RecipientUi(
                                            action.inboxId,
                                            action.inboxId.take(12),
                                        )
                                    ).copy(
                                        selected =
                                        true,
                                    )
                            },
                    )
                }
            }

            is CreateConversationAction.ChangeName -> {
                ui.update { it.copy(name = action.value) }
            }

            is CreateConversationAction.ChangeDescription -> {
                ui.update { it.copy(description = action.value) }
            }

            is CreateConversationAction.ChangePreset -> {
                ui.update { it.copy(adminOnly = action.adminOnly) }
            }

            CreateConversationAction.Back -> {
                ui.update { it.copy(step = CreateStep.RECIPIENTS) }
            }

            CreateConversationAction.Next -> {
                runAction {
                    owner,
                    origin,
                    token,
                    ->
                    resolve(
                        owner,
                        origin,
                        token,
                    )
                    if (generation.get() == token &&
                        destinations.accepts(origin)
                    ) {
                        ui.update {
                            it.copy(
                                step =
                                    CreateStep.REVIEW,
                            )
                        }
                    }
                }
            }

            CreateConversationAction.ResolveRecipients -> {
                runAction(::resolve)
            }

            CreateConversationAction.Create -> {
                runAction { owner, origin, token ->
                    resolve(owner, origin, token)
                    val value = ui.value
                    if (generation.get() != token ||
                        !destinations.accepts(origin)
                    ) {
                        throw CancellationException("Recipients changed")
                    }
                    val recipients = value.selectedRecipients.map { it.inboxId }.distinct()
                    if (recipients.isEmpty()) throw FeatureInputFailure("Add at least one recipient.")
                    if (!value.group &&
                        recipients.size != 1
                    ) {
                        throw FeatureInputFailure("A direct message needs one recipient.")
                    }
                    val maxMembers =
                        value.maxMembers ?: reads.withPermit {
                            owner.client
                                .serverConfiguration()
                                .mls.maxGroupMembers
                                .coerceAtMost(Int.MAX_VALUE.toULong())
                                .toInt()
                        }
                    if (value.group &&
                        recipients.size + 1 > maxMembers
                    ) {
                        throw FeatureInputFailure("This server allows fewer group members.")
                    }
                    if (generation.get() != token ||
                        !destinations.accepts(origin)
                    ) {
                        throw CancellationException("Recipients changed")
                    }
                    val chat =
                        queries.create(
                            owner,
                            value.group,
                            recipients,
                            value.name,
                            value.description,
                            value.adminOnly,
                        )
                    if (destinations.accepts(origin)) destinations.navigate(AppRoute.Chat(chat.id()))
                }
            }
        }
    }

    private fun runAction(action: suspend (ActiveSession, DestinationOrigin, Long) -> Unit) {
        if (ui.value.busy || destinations.state.value.route != AppRoute.CreateConversation) return
        val owner = session.active.value ?: return
        val origin = destinations.capture(owner)
        val token = generation.get()
        val command = Any()
        var registered = false
        destinations.admit(origin) {
            if (generation.get() == token && !hasBusyCommands()) {
                busyCommands[command] = CommandOwner(owner.key, token)
                ui.update { it.copy(busy = true, failure = null) }
                registered = true
            }
        }
        if (!registered) return
        val actionJob =
            owner.work.launch(Dispatchers.IO) {
                try {
                    action(owner, origin, token)
                } catch (error: Throwable) {
                    if (error is CancellationException) throw error
                    if (destinations.accepts(origin) &&
                        generation.get() == token
                    ) {
                        ui.update {
                            it.copy(
                                failure =
                                    featureFailure(
                                        error,
                                        "Cannot create this conversation. Check the recipients and try again.",
                                        "conversation-create",
                                    ),
                            )
                        }
                    }
                }
            }
        actionJob.invokeOnCompletion {
            session.withStateLock {
                busyCommands.remove(command)
                ui.update { it.copy(busy = hasBusyCommands()) }
            }
        }
    }

    private suspend fun resolve(
        owner: ActiveSession,
        origin: DestinationOrigin,
        token: Long,
    ) {
        val entered =
            ui.value.recipients
                .split(',')
                .map(String::trim)
                .filter(String::isNotEmpty)
        val rows = mutableListOf<RecipientUi>()
        for (value in entered) {
            if (!destinations.accepts(origin) ||
                generation.get() != token
            ) {
                throw CancellationException("Recipients changed")
            }
            try {
                val inbox =
                    reads.withPermit { queries.resolveInbox(owner, value) }
                        ?: throw FeatureInputFailure("No XMTP account matches this recipient.")
                val handle =
                    try {
                        reads.withPermit { queries.inboxHandle(owner, inbox) }
                    } catch (error: Throwable) {
                        if (error is CancellationException) throw error
                        null
                    }
                rows += RecipientUi(inbox, handle ?: inbox.take(12), selected = true)
            } catch (error: Throwable) {
                if (error is CancellationException) throw error
                rows +=
                    RecipientUi(
                        value,
                        value.take(12),
                        failure =
                            featureFailure(
                                error,
                                "Cannot resolve this recipient.",
                                "recipient",
                            ),
                    )
            }
        }
        if (!destinations.accepts(origin) ||
            generation.get() != token
        ) {
            throw CancellationException("Recipients changed")
        }
        val invalid = rows.firstOrNull { it.failure != null }
        ui.update { current ->
            current.copy(
                selectedRecipients =
                    (
                        current.selectedRecipients.filter {
                            it.failure == null &&
                                it.inboxId !in pastedIds
                        } +
                            rows.filter {
                                it.inboxId !in excludedPastedIds
                            }
                    ).distinctBy {
                        it.inboxId
                    },
            )
        }
        pastedIds = rows.map { it.inboxId }.toSet()
        if (invalid != null) throw FeatureInputFailure(invalid.failure!!.message)
        if (ui.value.selectedRecipients.isEmpty()) throw FeatureInputFailure("Add at least one recipient.")
    }
}
