package org.xmtp.android.example.messenger.feature.group

import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.Semaphore
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.sync.withPermit
import org.xmtp.android.example.messenger.*
import org.xmtp.android.example.messenger.app.DestinationOrigin
import org.xmtp.android.example.messenger.app.DestinationScope
import org.xmtp.android.example.messenger.data.xmtp.ChatQueries
import org.xmtp.android.example.messenger.data.xmtp.ConversationQueries
import org.xmtp.android.example.messenger.feature.AppFeaturePreferences
import org.xmtp.android.example.messenger.feature.FeatureInputFailure
import org.xmtp.android.example.messenger.feature.featureFailure
import org.xmtp.android.example.messenger.metadata.MetadataEditorController
import org.xmtp.android.example.messenger.notifications.NotificationController
import org.xmtp.android.example.shared.*
import org.xmtp.android.example.shared.feature.group.GroupAction
import org.xmtp.android.example.shared.feature.group.GroupEdit
import org.xmtp.android.example.shared.metadata.*
import uniffi.xmtp_sdk.*
import org.xmtp.android.example.shared.feature.group.GroupState as GroupUiState

internal class GroupController(
    private val session: AppSession,
    private val queries: ConversationQueries,
    private val chats: ChatQueries,
    private val preferences: AppFeaturePreferences,
    private val notifications: NotificationController,
    private val reads: Semaphore,
    private val destinations: DestinationScope,
    private val scope: CoroutineScope,
) {
    private val ui = MutableStateFlow(GroupUiState())
    val state: StateFlow<GroupUiState> = ui
    private val metadataUi = MutableStateFlow(MetadataEditorState())
    val metadataState: StateFlow<MetadataEditorState> = metadataUi
    private val metadataMutex = Mutex()
    private val projection = Mutex()

    @Volatile private var conversation: Conversation? = null

    @Volatile private var boundKey: SessionKey? = null

    private data class MetadataBinding(
        val key: SessionKey,
        val token: Long,
        val conversationId: String,
        val controller: MetadataEditorController,
    )

    @Volatile private var metadataBinding: MetadataBinding? = null
    private var metadataJob: Job? = null

    private enum class PendingPhase { WAITING, STARTED, ACCEPTED, CHECK_STATE }

    private data class PendingCommand(
        val token: Any,
        val action: GroupAction,
        val phase: PendingPhase,
    )

    private val pendingMembers = java.util.concurrent.ConcurrentHashMap<String, PendingCommand>()
    private val pendingLeaveCommand =
        java.util.concurrent.atomic
            .AtomicReference<PendingCommand?>()

    private fun memberId(action: GroupAction): String? =
        when (action) {
            is GroupAction.AddMember -> action.inboxId
            is GroupAction.RemoveMember -> action.inboxId
            is GroupAction.SetAdmin -> action.inboxId
            else -> null
        }

    private fun changePending(
        command: Any,
        origin: GroupOrigin,
        action: GroupAction,
        failed: Boolean,
        accepted: Boolean = false,
    ) {
        session.withStateLock {
            val id = memberId(action)
            val pending =
                if (action == GroupAction.RequestRemoval) {
                    pendingLeaveCommand.get()
                } else {
                    id?.let {
                        pendingMembers[it]
                    }
                }
            if (pending == null || pending.token !== command) return@withStateLock
            val next =
                if (failed && pending.phase == PendingPhase.WAITING) {
                    null
                } else {
                    val phase =
                        when {
                            accepted || pending.phase == PendingPhase.ACCEPTED -> PendingPhase.ACCEPTED
                            failed -> PendingPhase.CHECK_STATE
                            else -> PendingPhase.STARTED
                        }
                    pending.copy(phase = phase)
                }
            if (id != null) {
                if (next == null) pendingMembers.remove(id, pending) else pendingMembers.replace(id, pending, next)
            } else {
                pendingLeaveCommand.compareAndSet(pending, next)
            }
            if (session.active.value?.key == origin.owner.key && boundKey == origin.owner.key &&
                conversation?.id() == origin.chat.id()
            ) {
                ui.update {
                    it.copy(
                        pendingMemberIds = pendingMembers.keys.toList(),
                        pendingLeave =
                            pendingLeaveCommand.get() != null ||
                                it.settings.membership == membershipLabel(MembershipState.PENDING_REMOVE),
                    )
                }
            }
        }
    }

    private data class CommandOwner(
        val key: SessionKey,
        val conversationId: String,
    )

    private val busyCommands = java.util.concurrent.ConcurrentHashMap<Any, CommandOwner>()

    private fun hasBusyCommands(
        key: SessionKey,
        id: String,
    ) = busyCommands.values.any { it.key == key && it.conversationId == id }

    private data class GroupOrigin(
        val owner: ActiveSession,
        val chat: Conversation,
        val token: Long,
    )

    internal var beforeGroupWrite: suspend (GroupAction, Int) -> Unit = { _, _ -> }
    internal var onGroupActionFinished: (GroupAction) -> Unit = {}
    internal var beforeConversationNotificationAction: suspend () -> Unit = {}

    fun clear() {
        conversation =
            null
        boundKey =
            null
        pendingMembers.clear()
        pendingLeaveCommand.set(null)
        busyCommands.clear()
        clearMetadata()
        ui.value =
            GroupUiState()
    }

    private fun valid(
        owner: ActiveSession,
        token: Long,
    ) = destinations.accepts(DestinationOrigin(owner.key, token))

    private fun <T> onCurrentScreen(
        owner: ActiveSession,
        token: Long,
        change: () -> T,
    ): T? {
        var result: T? =
            null
        destinations.admit(
            DestinationOrigin(
                owner.key,
                token,
            ),
        ) {
            result =
                change()
        }
        return result
    }

    private fun routeId(): String? =
        when (
            val route =
                destinations.state.value.route
        ) {
            is AppRoute.ConversationDetails -> route.conversationId
            is AppRoute.GroupFields -> route.conversationId
            is AppRoute.OwnFields -> route.conversationId
            else -> null
        }

    private fun acceptsOrigin(origin: GroupOrigin) =
        boundKey == origin.owner.key &&
            valid(
                origin.owner,
                origin.token,
            ) &&
            origin.chat.id() == conversation?.id() &&
            origin.chat.id() == routeId()

    private fun requireOrigin(origin: GroupOrigin) {
        if (!acceptsOrigin(origin)) throw CancellationException("Details changed")
    }

    suspend fun open(
        id: String,
        ownFields: Boolean = false,
    ) {
        val owner = session.active.value ?: return
        val token = destinations.state.value.token
        val chat = queries.find(owner, id) ?: return
        onCurrentScreen(owner, token) {
            if (conversation?.id() != id) {
                clearMetadata()
                pendingMembers.clear()
                pendingLeaveCommand.set(null)
                ui.value =
                    GroupUiState(
                        conversationId =
                        id,
                    )
            }
            conversation = chat
            boundKey = owner.key
            ui.update { it.copy(busy = hasBusyCommands(owner.key, id)) }
        }
        refresh(owner)
    }

    fun dispatch(action: GroupAction) {
        val owner = session.active.value ?: return
        val chat = conversation ?: return
        val origin = GroupOrigin(owner, chat, destinations.state.value.token)
        if (!acceptsOrigin(origin)) return
        when (action) {
            is GroupAction.ChangeAlias -> {
                onCurrentScreen(
                    owner,
                    origin.token,
                ) {
                    if (acceptsOrigin(origin)) {
                        ui.update {
                            it.copy(
                                aliasInput =
                                    action.value,
                            )
                        }
                    }
                }
                return
            }

            is GroupAction.ChangeName -> {
                onCurrentScreen(
                    owner,
                    origin.token,
                ) {
                    if (acceptsOrigin(origin)) {
                        ui.update {
                            it.copy(
                                name =
                                    action.value,
                            )
                        }
                    }
                }
                return
            }

            is GroupAction.ChangeDescription -> {
                onCurrentScreen(
                    owner,
                    origin.token,
                ) {
                    if (acceptsOrigin(origin)) {
                        ui.update {
                            it.copy(
                                description =
                                    action.value,
                            )
                        }
                    }
                }
                return
            }

            is GroupAction.ChangeMemberInput -> {
                onCurrentScreen(
                    owner,
                    origin.token,
                ) {
                    if (acceptsOrigin(origin)) {
                        ui.update {
                            it.copy(
                                memberInput =
                                    action.value,
                            )
                        }
                    }
                }
                return
            }

            is GroupAction.ChangeDisappearingInput -> {
                onCurrentScreen(
                    owner,
                    origin.token,
                ) {
                    if (acceptsOrigin(origin)) {
                        ui.update {
                            it.copy(
                                disappearingInput =
                                    action.value,
                            )
                        }
                    }
                }
                return
            }

            is GroupAction.Edit -> {
                onCurrentScreen(
                    owner,
                    origin.token,
                ) {
                    if (acceptsOrigin(origin)) {
                        ui.update {
                            it.copy(
                                editing =
                                    action.value,
                            )
                        }
                    }
                }
                return
            }

            GroupAction.CancelEdit -> {
                onCurrentScreen(
                    owner,
                    origin.token,
                ) {
                    if (acceptsOrigin(origin)) {
                        ui.update {
                            it.copy(
                                editing =
                                    GroupEdit.NONE,
                                name =
                                    it.settings.title,
                                description =
                                    it.settings.description,
                                disappearingInput =
                                    it.settings.disappearingSeconds,
                                aliasInput =
                                    it.alias,
                            )
                        }
                    }
                }
                return
            }

            else -> {
                Unit
            }
        }
        val command = Any()
        var registered = false
        onCurrentScreen(owner, origin.token) {
            if (acceptsOrigin(origin)) {
                busyCommands[command] = CommandOwner(owner.key, chat.id())
                ui.update { it.copy(busy = true, failure = null) }
                registered = true
            }
        }
        if (!registered) return
        val actionJob =
            owner.work.launch(Dispatchers.IO) {
                var pendingAction = action
                try {
                    requireOrigin(origin)
                    when (action) {
                        GroupAction.Refresh -> {
                            refresh(owner)
                        }

                        is GroupAction.SetNotifications -> {
                            setConversationNotifications(origin, action.enabled)
                        }

                        is GroupAction.SetAlias -> {
                            preferences.setAlias(
                                owner.key,
                                chat.id(),
                                action.value,
                            )
                            onCurrentScreen(
                                owner,
                                origin.token,
                            ) {
                                ui.update {
                                    it.copy(
                                        alias =
                                            action.value.trim(),
                                        aliasInput =
                                            action.value.trim(),
                                    )
                                }
                            }
                        }

                        GroupAction.Block -> {
                            notifications.withPrivacyMutation(owner) {
                                requireOrigin(origin)
                                chat.updateConsentState(ConsentState.DENIED)
                                if (acceptsOrigin(origin)) destinations.navigate(AppRoute.Chats)
                            }
                        }

                        else -> {
                            pendingAction =
                                if (action is GroupAction.AddMember) {
                                    val inbox =
                                        reads.withPermit { queries.resolveInbox(owner, action.inboxId) }
                                            ?: throw FeatureInputFailure("No XMTP account matches this recipient.")
                                    action.copy(inboxId = inbox)
                                } else {
                                    action
                                }
                            requireOrigin(origin)
                            val memberId = memberId(pendingAction)
                            onCurrentScreen(owner, origin.token) {
                                if (acceptsOrigin(origin)) {
                                    val pending = PendingCommand(command, pendingAction, PendingPhase.WAITING)
                                    if (memberId != null) pendingMembers[memberId] = pending
                                    if (action == GroupAction.RequestRemoval) pendingLeaveCommand.set(pending)
                                    ui.update {
                                        it.copy(
                                            pendingMemberIds = pendingMembers.keys.toList(),
                                            pendingLeave = pendingLeaveCommand.get() != null || it.pendingLeave,
                                        )
                                    }
                                }
                            }
                            mutateGroup(origin, pendingAction, command)
                        }
                    }
                } catch (error: Throwable) {
                    changePending(command, origin, pendingAction, failed = true)
                    if (error is CancellationException) throw error
                    if (acceptsOrigin(origin)) {
                        ui.update {
                            it.copy(
                                failure =
                                    featureFailure(
                                        error,
                                        "Cannot update this conversation. Try again.",
                                        "group-action",
                                    ),
                            )
                        }
                    }
                } finally {
                    onGroupActionFinished(action)
                }
            }
        actionJob.invokeOnCompletion {
            session.withCurrent(owner.key) {
                busyCommands.remove(command)
                if (boundKey == owner.key && conversation?.id() == chat.id()) {
                    ui.update { current ->
                        if (current.conversationId == chat.id()) {
                            current.copy(busy = hasBusyCommands(owner.key, chat.id()))
                        } else {
                            current
                        }
                    }
                }
            }
            busyCommands.remove(command)
        }
    }

    suspend fun refresh(owner: ActiveSession) =
        projection.withLock {
            if (conversation == null || boundKey != owner.key || !session.accepts(owner.key)) return@withLock
            val token = destinations.state.value.token
            refreshSettings(owner, token)
            refreshMetadata(owner)
        }

    private suspend fun setConversationNotifications(
        origin: GroupOrigin,
        enabled: Boolean,
    ) {
        beforeConversationNotificationAction()
        notifications.setConversationEnabled(
            origin.owner,
            origin.chat,
            enabled,
        ) { change ->
            onCurrentScreen(
                origin.owner,
                origin.token,
            ) {
                if (origin.chat.id() == conversation?.id()) {
                    change()
                    true
                } else {
                    false
                }
            } == true
        }
        if (acceptsOrigin(origin)) refresh(origin.owner)
    }

    private suspend fun mutateGroup(
        origin: GroupOrigin,
        action: GroupAction,
        command: Any,
    ) {
        val owner = origin.owner
        if (action is GroupAction.AddMember || action is GroupAction.RemoveMember ||
            action == GroupAction.RequestRemoval
        ) {
            notifications.withPrivacyMutation(owner) { mutateGroupOwned(origin, action, command) }
        } else {
            mutateGroupOwned(origin, action, command)
        }
    }

    private suspend fun mutateGroupOwned(
        origin: GroupOrigin,
        action: GroupAction,
        command: Any,
    ) {
        val owner = origin.owner
        val token = origin.token
        val chat = origin.chat
        requireOrigin(origin)
        var writeIndex = 0

        suspend fun admitWrite() {
            beforeGroupWrite(action, writeIndex)
            if (writeIndex == 0) {
                requireOrigin(origin)
            } else if (!session.accepts(owner.key)) {
                throw CancellationException("Session changed during group action")
            }
            writeIndex += 1
            changePending(command, origin, action, failed = false)
        }
        try {
            if (action is GroupAction.SetDisappearing) {
                require(
                    action.seconds >= 0,
                )
                admitWrite()
                chat
                    .updateDisappearingSettings(
                        if (action.seconds == 0L) {
                            null
                        } else {
                            DisappearingSettings(
                                Timestamp(
                                    System
                                        .currentTimeMillis() * 1_000_000,
                                ),
                                Math
                                    .multiplyExact(
                                        action.seconds,
                                        1_000_000_000,
                                    ),
                            )
                        },
                    )
            } else {
                val group =
                    (
                        chat as? Conversation.Group
                    )?.group ?: error("This action needs a group")
                when (action) {
                    is GroupAction.UpdateGroup,
                    -> {
                        admitWrite()
                        group
                            .updateName(
                                action.name,
                            )
                        admitWrite()
                        group
                            .updateDescription(
                                action.description,
                            )
                    }

                    is GroupAction.AddMember,
                    -> {
                        admitWrite()
                        group
                            .addMembers(
                                listOf(
                                    action.inboxId,
                                ),
                            )
                    }

                    is GroupAction.RemoveMember,
                    -> {
                        admitWrite()
                        group
                            .removeMembers(
                                listOf(
                                    action.inboxId,
                                ),
                            )
                    }

                    is GroupAction.SetAdmin,
                    -> {
                        admitWrite()
                        if (action.admin) {
                            group
                                .addAdmin(
                                    action.inboxId,
                                )
                        } else {
                            group
                                .removeAdmin(
                                    action.inboxId,
                                )
                        }
                    }

                    GroupAction.RequestRemoval,
                    -> {
                        admitWrite()
                        group
                            .requestRemoval()
                    }

                    is GroupAction.SetPreset,
                    -> {
                        applyStandardPreset(
                            group,
                            action.adminOnly,
                            beforeWrite = { admitWrite() },
                        )
                    }

                    else -> {
                        Unit
                    }
                }
            }
            changePending(command, origin, action, failed = false, accepted = true)
        } catch (error: Throwable) {
            changePending(command, origin, action, failed = true)
            throw error
        } finally {
            if (valid(owner, token)) refreshSettings(owner, token)
        }
    }

    private suspend fun refreshSettings(
        owner: ActiveSession,
        token: Long,
    ) {
        val chat = conversation ?: return
        val (memberPending, leavePending) =
            session.withStateLock {
                pendingMembers.toMap() to pendingLeaveCommand.get()
            }
        val own =
            owner.client
                .inboxId()
        val members =
            chat
                .members()
        val group =
            (
                chat as? Conversation.Group
            )?.group?.let { queries.groupState(it) }
        val common =
            group?.common ?: (
                chat as Conversation.Dm
            ).dm
                .state()
        val canManage = group?.admins?.contains(own) == true || group?.superAdmins?.contains(own) == true
        val settings =
            ConversationSettings(
                chat.displayTitle(group?.name, members),
                group
                    ?.description
                    .orEmpty(),
                group != null,
                members.map {
                    MemberRow(
                        it.inboxId,
                        when (
                            it.permissionLevel
                        ) {
                            PermissionLevel.ADMIN,
                            -> "Admin"

                            PermissionLevel.SUPER_ADMIN,
                            -> "Super admin"

                            else -> "Member"
                        },
                        canManage && it.inboxId != own && it.permissionLevel !=
                            PermissionLevel.SUPER_ADMIN,
                        displayName = it.handle,
                    )
                },
                policyLabel(group?.permissions?.policyType),
                membershipLabel(group?.membershipState),
                group != null && members.size > 1 &&
                    group.superAdmins.none {
                        it == own
                    } && group.membershipState !=
                    MembershipState.PENDING_REMOVE,
                (
                    (
                        common.disappearingSettings?.retentionNs ?: 0L
                    ) / 1_000_000_000
                ).toString(),
                common.notificationsEnabled,
            )
        val media =
            reads.withPermit {
                chats
                    .history(
                        chat,
                        publishedSelection().copy(
                            limit =
                            50u,
                            contentTypes =
                                listOf(
                                    ContentTypeId(
                                        "xmtp.org",
                                        "remoteStaticAttachment",
                                        1u,
                                        0u,
                                    ),
                                ),
                        ),
                        null,
                        null,
                    ).messages
                    .map {
                        it.toRow(own)
                    }
            }
        val alias = preferences.alias(owner.key.profileId, chat.id()).orEmpty()
        if (valid(
                owner,
                token,
            )
        ) {
            onCurrentScreen(owner, token) {
                if (boundKey != owner.key || conversation?.id() != chat.id()) return@onCurrentScreen
                val completed =
                    memberPending
                        .filter { (id, pending) ->
                            val member = settings.members.firstOrNull { it.inboxId == id }
                            val desired =
                                when (val action = pending.action) {
                                    is GroupAction.AddMember -> {
                                        member != null
                                    }

                                    is GroupAction.RemoveMember -> {
                                        member == null
                                    }

                                    is GroupAction.SetAdmin -> {
                                        member != null &&
                                            (
                                                member.role == "Admin" ||
                                                    member.role == "Super admin"
                                            ) == action.admin
                                    }

                                    else -> {
                                        true
                                    }
                                }
                            (pending.phase != PendingPhase.WAITING && desired) ||
                                pending.phase == PendingPhase.CHECK_STATE
                        }
                completed.forEach { (id, pending) -> pendingMembers.remove(id, pending) }
                if (leavePending != null && group != null &&
                    (
                        group.membershipState !in listOf(MembershipState.ALLOWED, MembershipState.PENDING) ||
                            leavePending.phase == PendingPhase.CHECK_STATE
                    )
                ) {
                    pendingLeaveCommand.compareAndSet(leavePending, null)
                }
                val leaveRequested =
                    pendingLeaveCommand.get() != null &&
                        group?.membershipState in listOf(MembershipState.ALLOWED, MembershipState.PENDING)
                ui.update { currentUi ->
                    currentUi.copy(
                        settings =
                        settings,
                        sharedMedia =
                        media,
                        alias =
                        alias,
                        aliasInput =
                            if (currentUi.aliasInput == currentUi.alias) alias else currentUi.aliasInput,
                        name = if (currentUi.editing == GroupEdit.NONE) settings.title else currentUi.name,
                        description =
                            if (currentUi.editing == GroupEdit.NONE) settings.description else currentUi.description,
                        disappearingInput =
                            if (currentUi.editing == GroupEdit.NONE) {
                                settings.disappearingSeconds
                            } else {
                                currentUi.disappearingInput
                            },
                        pendingMemberIds = pendingMembers.keys.toList(),
                        pendingLeave = group?.membershipState == MembershipState.PENDING_REMOVE || leaveRequested,
                        metadata =
                            CapabilityUi(true),
                        notifications =
                            CapabilityUi(
                                notifications.configured,
                                if (notifications.configured) null else "Notifications are not configured.",
                            ),
                        leave =
                            CapabilityUi(
                                settings.canRequestRemoval,
                                if (settings.canRequestRemoval) null else "This account cannot leave this group.",
                            ),
                    )
                }
            }
        }
    }

    private fun clearMetadata() {
        metadataJob?.cancel()
        metadataJob = null
        metadataBinding = null
        metadataUi.value = MetadataEditorState(busy = true)
    }

    private suspend fun refreshMetadata(owner: ActiveSession) {
        if (!scope.isActive ||
            (
                destinations.state.value.route !is AppRoute.GroupFields &&
                    destinations.state.value.route !is AppRoute.OwnFields
            )
        ) {
            return
        }
        val chat = conversation ?: return
        val token = destinations.state.value.token
        owner.work
            .async {
                metadataMutex.withLock {
                    if (!valid(owner, token)) return@withLock
                    try {
                        var binding = metadataBinding
                        if (binding?.key != owner.key ||
                            binding.token != token || binding.conversationId != chat.id()
                        ) {
                            val offered = reads.withPermit { owner.client.serverConfiguration().applicationComponents }
                            if (!valid(owner, token)) return@withLock
                            val controller =
                                MetadataEditorController(chat, owner.client.inboxId(), {
                                    val current = scope.isActive && valid(owner, token)
                                    current && conversation?.id() == chat.id()
                                }, offered, reads)
                            binding = MetadataBinding(owner.key, token, chat.id(), controller)
                            onCurrentScreen(owner, token) {
                                metadataBinding = binding
                                metadataJob?.cancel()
                                metadataJob =
                                    scope.launch {
                                        controller.state.collect { state ->
                                            onCurrentScreen(owner, token) { metadataUi.value = state }
                                        }
                                    }
                            }
                        }
                        binding.controller.refresh()
                    } catch (error: Throwable) {
                        if (error is CancellationException) throw error
                        onCurrentScreen(owner, token) {
                            metadataUi.value =
                                metadataUi.value.copy(
                                    busy = false,
                                    error =
                                        featureFailure(
                                            error,
                                            "Cannot load these fields. Try again.",
                                            "metadata-read",
                                        ).message,
                                )
                        }
                    }
                }
            }.await()
    }

    fun editMetadata(edit: MetadataEdit) {
        if (!scope.isActive) return
        val binding = metadataBinding ?: return
        val owner = session.active.value ?: return
        if (!destinations.accepts(DestinationOrigin(binding.key, binding.token)) || owner.key != binding.key) return
        owner.work.launch { binding.controller.edit(edit) }
    }
}
