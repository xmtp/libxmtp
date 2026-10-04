"""Explicit design-row decisions for the SDK source inventory.

An unlisted declaration is an open item. No file-name or common-name fallback
may silently approve it. References name the Section 11.4 sub-table.
"""

from __future__ import annotations

import re
from dataclasses import dataclass
from pathlib import Path


@dataclass(frozen=True)
class Decision:
    status: str
    final: str
    ref: str
    note: str
    open: bool = False


def decision(status: str, final: str, ref: str, note: str = "") -> Decision:
    return Decision(status, final, ref, note)


RULES: dict[tuple[str, str], Decision] = {}


def add(
    sdk: str,
    owner: str,
    names: str,
    status: str,
    ref: str,
    *,
    finals: str | None = None,
    note: str = "",
) -> None:
    source_names = names.split()
    target_names = finals.split() if finals else source_names
    if len(source_names) != len(target_names):
        raise ValueError((source_names, target_names))
    for source, target in zip(source_names, target_names):
        current = f"{owner}.{source}" if owner else source
        final = (
            "—"
            if status == "approved removal"
            else (
                target
                if re.match(r"^[A-Z][A-Za-z0-9_]*(?:\.|\()", target)
                else f"{owner}.{target}"
                if owner
                else target
            )
        )
        key = (sdk, current)
        if key in RULES:
            raise ValueError(f"duplicate rule: {key}")
        RULES[key] = decision(status, final, ref, note)


for sdk in ("Swift", "Kotlin"):
    ref = f"11.4 {sdk}, Messages, codecs, preferences, values; 11.1-11.2"
    add(
        sdk,
        "Reaction",
        "reference",
        "generated",
        ref,
        finals="ReactionV2Content.reference",
        note="The codec value keeps the target message ID.",
    )
    add(
        sdk,
        "Reaction",
        "referenceInboxId",
        "generated",
        ref,
        finals="ReactionV2Content.referenceInboxId",
        note="The codec value keeps the optional target sender inbox ID.",
    )


# Design SDK-040 retained-content exception: the legacy wrapper exposes the
# binding record that keeps exact received bytes and the typed decode cause.
# The façade carries the same evidence on MessageContent.Unknown.
for name in ("UndecodableContent", "ContentDecodeFailureKind"):
    add(
        "Swift",
        "",
        name,
        "generated",
        "11.4 Swift, Messages, codecs, preferences, values; 11.1-11.2",
        finals="MessageContent.Unknown",
        note="Retained undecodable content under the design SDK-040 exception: exact received bytes, optional received type and uncompressed envelope, received fallback, and ErrorDetails (MalformedEnvelope or CodecDecodeFailed).",
    )
add(
    "Kotlin",
    "DecodedMessageV2",
    "undecodable",
    "generated",
    "11.4 Kotlin, Messages, codecs, preferences, values; 11.1-11.2",
    finals="MessageContent.Unknown",
    note="Retained undecodable content on the V2 route under the design SDK-040 exception; the new SDK retains original bytes and typed ErrorDetails, with optional type and uncompressed envelope.",
)


for sdk in ("Swift", "Kotlin"):
    client_owner = "Client" if sdk == "Swift" else "Client.Companion"
    ref = f"11.4 {sdk}, Client and options"
    add(
        sdk,
        client_owner,
        "create",
        "static runtime",
        ref,
        note="Host wrapper owns codecs and closures (11.1; plan Decisions).",
    )
    add(
        sdk,
        client_owner,
        "build",
        "static runtime",
        ref,
        note=(
            "Host wrapper owns codecs and closures (11.1; plan Decisions). "
            "Build requires a stored identity and does not fetch configuration "
            f"for an empty database. The current {sdk} build starts offline "
            "with an inbox ID."
        ),
    )
    add(sdk, client_owner, "createInMemory", "approved removal", ref)
    add(
        sdk,
        client_owner,
        "connectToApiBackend getOrCreateInboxId inboxStatesForInboxIds keyPackageStatusesForInstallationIds getNewestMessageMetadata",
        "generated",
        ref,
        finals="Backend.connect Client.inboxID(for:) Client.inboxStates Client.keyPackageStatuses Client.newestMessageMetadata",
    )
    add(
        sdk,
        client_owner,
        "ffiApplySignatureRequest ffiRevokeInstallations ffiRevokeAllOtherInstallations ffiRevokeIdentity ffiAddIdentity ffiSignatureRequest ffiRegisterIdentity",
        "generated",
        ref,
        finals="unsafeApplySignatureRequest unsafeRevokeInstallationsSignatureRequest unsafeRevokeAllOtherInstallationsSignatureRequest unsafeRemoveAccountSignatureRequest unsafeAddAccountSignatureRequest unsafeCreateInboxSignatureRequest register",
    )
    add(
        sdk,
        client_owner,
        "ffiCreateClient",
        "approved removal",
        ref,
        note="Deprecated binding entry point; create/build are host wrappers.",
    )
    add(
        sdk, "Client", "inboxStatesForInboxIds", "generated", ref, finals="inboxStates"
    ) if sdk == "Kotlin" else None
    add(
        sdk,
        "Client",
        "keyPackageStatusesForInstallationIds getNewestMessageMetadata",
        "generated",
        ref,
        finals="keyPackageStatuses newestMessageMetadata",
    ) if sdk == "Kotlin" else None
    add(sdk, "Client", "inboxIdFromIdentity", "generated", ref, finals="inboxID(for:)")
    add(
        sdk,
        "Client",
        "verifySignature verifySignatureWithInstallationId",
        "generated",
        ref,
        finals="verifySignedWithInstallationKey verifySignedWithPublicKey",
    )
    add(
        sdk,
        "Client",
        "deleteLocalDatabase dropLocalDatabaseConnection reconnectLocalDatabase",
        "generated",
        ref,
        finals="storage.delete end() storage.reconnect",
    )
    add(
        sdk,
        "Client",
        "createArchive importArchive archiveMetadata",
        "generated",
        ref,
        finals="archives.exportToFile archives.importFromFile archives.metadataFromFile",
    )
    add(
        sdk,
        "Client",
        "debugInformation libXMTPVersion publicIdentity environment dbPath",
        "generated",
        ref,
        finals="diagnostics libxmtpVersion identity options.storage.label storage.path",
    )
    add(
        sdk,
        "Client",
        "register",
        "approved removal",
        ref,
        note="Global codec registration moves to ClientOptions.codecs.",
    ) if sdk == "Swift" else None
    add(sdk, "Client", "addAccount", "generated", ref, finals="unsafeAddAccount")
    add(
        sdk,
        "Client",
        "getXMTPLogFilePaths clearXMTPLogs activatePersistentLibXMTPLogWriter deactivatePersistentLibXMTPLogWriter",
        "platform helper",
        "2, platform files",
        note=f"Moves to Logging.{'swift' if sdk == 'Swift' else 'kt'}.",
    ) if sdk == "Swift" else None
    add(
        sdk,
        "Client.Companion",
        "getXMTPLogFilePaths clearXMTPLogs activatePersistentLibXMTPLogWriter deactivatePersistentLibXMTPLogWriter",
        "platform helper",
        "2, platform files",
        note="Moves to Logging.kt.",
    ) if sdk == "Kotlin" else None
    add(
        sdk,
        "Conversations",
        "findGroup findConversation findConversationByTopic findDmByInboxId findDmByIdentity findMessage",
        "generated",
        f"11.4 {sdk}, Conversations",
        finals="getByID getByID getByID getDmByInboxID getDmByIdentity getMessageByID",
    )
    add(
        sdk,
        "Conversations",
        "findEnrichedMessage fromWelcome streamMessageDeletions",
        "approved removal",
        f"11.4 {sdk}, Conversations",
    )
    add(
        sdk,
        "Conversations",
        "newConversation newConversationWithIdentity findOrCreateDm findOrCreateDmWithIdentity",
        "generated",
        f"11.4 {sdk}, Conversations",
        finals="createDm createDm createDm createDm",
    )
    add(
        sdk,
        "Conversations",
        "newGroup newGroupCustomPermissions newGroupWithIdentities newGroupCustomPermissionsWithIdentities",
        "generated",
        f"11.4 {sdk}, Conversations",
        finals="createGroup createGroup createGroup createGroup",
        note="Identity forms are same-name overloads (plan Decision 6).",
    )
    add(
        sdk,
        "Conversations",
        "syncAllConversations getHmacKeys",
        "generated",
        f"11.4 {sdk}, Conversations",
        finals="syncAll hmacKeys",
    )
    for owner in ("Conversation", "Group", "Dm"):
        cref = f"11.4 {sdk}, Conversation, Group, Dm"
        add(
            sdk,
            owner,
            "lastActivityAtNs" if sdk == "Swift" else "lastActivityNs",
            "generated",
            cref,
            finals="lastActivityAt(contentTypes?)",
            note="Plan Decisions: optional filter is a method outside state().",
        )
        add(
            sdk,
            owner,
            "getHmacKeys getLastReadTimes streamMessages processMessage getDebugInformation",
            "generated",
            cref,
            finals="hmacKeys lastReadTimes stream processStreamedMessage debugInfo",
        )
        add(sdk, owner, "enrichedMessages endStream", "approved removal", cref)
        add(
            sdk,
            owner,
            "updateDisappearingMessageSettings clearDisappearingMessageSettings",
            "generated",
            cref,
            finals="updateDisappearingSettings updateDisappearingSettings",
        )
        add(
            sdk,
            owner,
            "createdAt createdAtNs",
            "generated",
            cref,
            finals="createdAt.date createdAt.ns",
            note="Plan Decisions: one Timestamp with date and ns views.",
        )
        add(sdk, owner, "messageReader", "generated", cref)
        add(
            sdk,
            owner,
            "isActive consentState pausedForVersion isDisappearingMessagesEnabled disappearingMessageSettings commitLogForkStatus notificationsEnabled",
            "generated",
            cref,
            finals="state().isActive state().consentState state().pausedForVersion state().isDisappearingEnabled state().disappearingSettings state().commitLogForkStatus state().notificationsEnabled",
        )
        if sdk == "Kotlin":
            add(sdk, owner, "encodeContent", "approved removal", cref)
        if owner == "Group":
            add(
                sdk,
                owner,
                "name imageUrl description appData membershipState permissionPolicySet",
                "generated",
                cref,
                finals="state().name state().imageUrl state().description state().appData state().membershipState state().permissions.policySet",
            )
            add(
                sdk,
                owner,
                "updateAddMemberPermission updateRemoveMemberPermission updateAddAdminPermission updateRemoveAdminPermission updateNamePermission updateDescriptionPermission updateImageUrlPermission",
                "generated",
                cref,
                finals="updatePermission updatePermission updatePermission updatePermission updatePermission updatePermission updatePermission",
            )
            add(sdk, owner, "leaveGroup", "generated", cref, finals="requestRemoval")
            add(sdk, owner, "proposalsEnabled unstable", "approved removal", cref)
            add(sdk, owner, "peerInboxIds members", "generated", cref)
        if owner == "Dm":
            add(sdk, owner, "peerInboxId", "generated", cref, finals="peerInboxID")
        if owner in {"Conversation", "Group"}:
            add(
                sdk, owner, "members", "generated", cref
            ) if owner == "Conversation" else None
        add(
            sdk,
            owner,
            "countMessages messages send prepareMessage publishMessages publishMessage deleteMessage sync lastMessage messageHistorySnapshot beginningDeliveryCursor setNotifications updateConsentState",
            "generated",
            cref,
        )
    add(
        sdk,
        "MessageReader",
        "next updateScope updateFilter catchUpSnapshot catchUpChanged",
        "generated",
        "11.2, readers",
    )
    add(
        sdk,
        "MessageReader",
        "messages",
        "static runtime",
        "11.2, readers",
        finals="stream()",
    )
    add(
        sdk,
        "MessageReader",
        "close",
        "generated",
        "plan Decisions, reader end",
        finals="end()",
    )
    add(sdk, "DeliveryCursor", "", "generated", "11.2, readers") if False else None

for sdk in ("Node", "Browser"):
    ref = f"11.4 {sdk}, Client and options" if sdk == "Node" else "11.4 Browser"
    add(
        sdk,
        "Client",
        "create",
        "static runtime",
        ref,
        note="Host wrapper owns codecs and closures (11.1; plan Decisions).",
    )
    add(
        sdk,
        "Client",
        "build",
        "static runtime",
        ref,
        note=(
            "Host wrapper owns codecs and closures (11.1; plan Decisions). "
            "The façade build requires a stored identity and does not fetch "
            f"configuration for an empty database. The current {sdk} build does "
            "not: it resolves the inbox ID from the backend and creates the "
            "client online without allowOffline, so an empty database fetches "
            "configuration and needs no stored identity."
        ),
    )
    add(
        sdk,
        "Client",
        "fetchLatestInboxUpdatesCount fetchOwnInboxUpdatesCount fetchKeyPackageStatuses fetchInboxIdByIdentifier fetchInboxStates",
        "generated",
        ref,
        finals="latestInboxUpdatesCount ownInboxUpdatesCount keyPackageStatuses inboxID(for:) inboxStates",
    )
    add(sdk, "Client", "unsafe_addAccount", "generated", ref, finals="unsafeAddAccount")
    add(
        sdk,
        "Client",
        "unsafe_applySignatureRequest",
        "generated",
        ref,
        finals="unsafeApplySignatureRequest(request)"
        if sdk == "Browser"
        else "unsafeApplySignatureRequest",
        note="The request object replaces the signer and request ID pair."
        if sdk == "Browser"
        else "",
    )
    add(
        sdk,
        "Client",
        "unsafe_addSignature",
        "approved removal",
        ref,
        note="SignatureRequest.sign replaces this helper.",
    ) if sdk == "Node" else None
    add(
        sdk,
        "Client",
        "createArchive importArchive archiveMetadata",
        "generated",
        ref,
        finals=(
            "archives.exportToFile archives.importFromFile archives.metadataFromFile"
            if sdk == "Node"
            else "archives.exportToBytes archives.importFromBytes archives.metadataFromBytes"
        ),
    )
    add(sdk, "Client", "debugInformation", "generated", ref, finals="diagnostics")
    add(
        sdk,
        "Client",
        "close",
        "generated",
        "plan Decisions, client end",
        finals="end()",
    )
    add(sdk, "Client", "isRegistered", "generated", ref, finals="isRegistered()")
    add(
        sdk,
        "Conversations",
        "getConversationById getDmByInboxId fetchDmByIdentifier getMessageById",
        "generated",
        f"11.4 {sdk}, Conversations"
        if sdk == "Node"
        else "11.4 Browser; 11.4 Node, Conversations",
        finals="getByID getDmByInboxID getDmByIdentity getMessageByID",
    )
    add(
        sdk,
        "Conversations",
        "createGroup createGroupWithIdentifiers createGroupOptimistic createDm createDmWithIdentifier",
        "generated",
        f"11.4 {sdk}, Conversations"
        if sdk == "Node"
        else "11.4 Browser; 11.4 Node, Conversations",
        finals="createGroup createGroup createGroupOptimistic createDm createDm",
        note="Identity forms are TypeScript unions (plan Decision 6).",
    )
    add(
        sdk,
        "Conversations",
        "streamMessageDeletions streamDeletedMessages",
        "approved removal",
        "11.8, live events; 11.4 Node, Conversations",
    )
    for owner in ("Conversation", "Group", "Dm"):
        cref = (
            f"11.4 {sdk}, Conversation, Group, Dm"
            if sdk == "Node"
            else "11.4 Browser; 11.4 Node, Conversation, Group, Dm"
        )
        add(
            sdk,
            owner,
            "createdAt createdAtNs",
            "generated",
            cref,
            finals="createdAt.date createdAt.ns",
            note="Plan Decisions: one Timestamp with date and ns views.",
        ) if owner == "Conversation" else None
        add(
            sdk,
            owner,
            "id topic addedByInboxId",
            "generated",
            cref,
            finals="id topic addedByInboxID",
        ) if owner == "Conversation" else None
        add(
            sdk,
            owner,
            "messageDisappearingSettings isMessageDisappearingEnabled",
            "generated",
            cref,
            finals="state().disappearingSettings state().isDisappearingEnabled",
        ) if owner == "Conversation" else None
        add(
            sdk, owner, "peerInboxId", "generated", cref, finals="peerInboxID"
        ) if owner == "Dm" else None
        add(
            sdk,
            owner,
            "updateMessageDisappearingSettings removeMessageDisappearingSettings",
            "generated",
            cref,
            finals="updateDisappearingSettings updateDisappearingSettings",
        ) if owner == "Conversation" else None
        add(
            sdk, owner, "messages countMessages sync", "generated", cref
        ) if owner == "Conversation" else None
    add(
        sdk,
        "DecodedMessage",
        "numReplies",
        "static runtime",
        "11.4 Node, Conversation, Group, Dm; 11.7",
        finals="Message.replyCount",
    )
    add(
        sdk,
        "Preferences",
        "streamConsent streamPreferences",
        "approved removal",
        "11.8, live events",
    )
    add(
        sdk,
        "Preferences",
        "getConsentState setConsentStates",
        "generated",
        "11.4 Node, Messages, codecs, preferences, values",
        finals="consentState setConsentStates",
    )
    add(
        sdk,
        "Preferences",
        "inboxState fetchInboxState getInboxStates fetchInboxStates",
        "generated",
        "11.4 Node, Conversation, Group, Dm",
        finals="Client.inboxState Client.inboxState Client.inboxStates Client.inboxStates",
        note="The old preference name maps to the Client method.",
    )

for sdk in ("Swift", "Kotlin", "Node", "Browser"):
    for owner in ("Conversation", "Group", "Dm"):
        cref = (
            f"11.4 {sdk}, Conversation, Group, Dm"
            if sdk != "Browser"
            else "11.4 Browser; 11.4 Node, Conversation, Group, Dm"
        )
        fields = "isActive consentState pausedForVersion notificationsEnabled"
        if sdk in ("Node", "Browser"):
            add(
                sdk,
                owner,
                fields,
                "generated",
                cref,
                finals="state().isActive state().consentState state().pausedForVersion state().notificationsEnabled",
            ) if owner == "Conversation" else None

for sdk in ("Swift", "Kotlin"):
    ref = f"11.4 {sdk}, Client and options"
    add(
        sdk,
        "ClientOptions",
        "api codecs preAuthenticateToInboxCallback dbEncryptionKey dbDirectory deviceSyncEnabled forkRecoveryOptions dbPoolOptions",
        "generated",
        ref,
        finals="backend codecs handlers.preAuthenticate storage.encryptionKey storage.location deviceSync forkRecovery storage.pool",
    )
    add(
        sdk,
        "ClientOptions",
        "waitForRegistrationVisible",
        "approved removal",
        ref,
        note="Registration always waits for visibility; the old option leaves the final API.",
    )
    add(sdk, "ClientOptions", "unstableChangeCallbacks", "approved removal", ref)
    add(
        sdk,
        "ClientOptions.Api",
        "backendUrl env appVersion authCallback",
        "generated",
        ref,
        finals="BackendOptions.url StorageOptions.label BackendOptions.appVersion BackendOptions.credentials",
    )
    add(
        sdk, "ClientOptions.Api", "init", "generated", ref, finals="BackendOptions.init"
    )
    add(sdk, "ClientOptions", "init", "static runtime", ref)
    add(
        sdk, "ClientOptions", "debugEventsEnabled", "approved removal", ref
    ) if sdk == "Swift" else None
    add(
        sdk,
        "ClientOptions",
        "appContext",
        "platform helper",
        "2, AndroidPlatform.kt",
        finals="StorageOptions(context)",
    ) if sdk == "Kotlin" else None
    add(
        sdk,
        "Client",
        "inboxID installationID conversations preferences",
        "generated",
        ref,
    ) if sdk == "Swift" else None
    add(sdk, "Client", "isInMemory", "generated", ref)
    add(
        sdk,
        "Conversations",
        "newGroupOptimistic",
        "generated",
        f"11.4 {sdk}, Conversations",
        finals="createGroupOptimistic",
    )
    add(
        sdk,
        "Group",
        "addedByInboxId",
        "generated",
        f"11.4 {sdk}, Conversation, Group, Dm",
        finals="addedByInboxID",
    )
    add(
        sdk,
        "Dm",
        "addedByInboxId",
        "generated",
        f"11.4 {sdk}, Conversation, Group, Dm",
        finals="addedByInboxID",
    ) if sdk == "Swift" else None

for sdk in ("Node", "Browser"):
    ref = (
        "11.4 Node, Client and options"
        if sdk == "Node"
        else "11.4 Browser; 11.4 Node, Client and options"
    )
    add(
        sdk,
        "Client",
        "inboxId installationId",
        "generated",
        ref,
        finals="inboxID installationID",
    )
    add(sdk, "Client", "env", "generated", ref, finals="options.storage.label")
    add(
        sdk,
        "Client",
        "signWithInstallationKey verifySignedWithInstallationKey verifySignedWithPublicKey",
        "generated",
        ref,
    )
    add(
        sdk,
        "Client",
        "register revokeAllOtherInstallations revokeInstallations removeAccount changeRecoveryIdentifier canMessage syncAllDeviceSyncGroups serverConfiguration refreshServerConfiguration fetchServerConfiguration",
        "generated",
        ref,
    )
    add(
        sdk,
        "Conversation",
        "metadata",
        "generated",
        "11.4 Browser",
        note="The immutable creatorInboxId and kind fields replace this method.",
    ) if sdk == "Browser" else None
    add(sdk, "Conversations", "syncAll", "generated", "11.4 Node, Conversations")
    add(
        sdk,
        "Group",
        "name imageUrl description appData permissions",
        "generated",
        "11.4 Node, Conversation, Group, Dm",
        finals="state().name state().imageUrl state().description state().appData state().permissions",
    )
    add(
        sdk,
        "Group",
        "admins superAdmins",
        "generated",
        "11.4 Browser",
        finals="state().admins state().superAdmins",
    ) if sdk == "Browser" else None
    add(
        sdk,
        "Group",
        "listAdmins listSuperAdmins isPendingRemoval",
        "generated",
        "11.4 Node, Conversation, Group, Dm",
        finals="state().admins state().superAdmins state().membershipState",
    )
    add(
        sdk,
        "NetworkOptions",
        "backendUrl env appVersion authCallback",
        "generated",
        ref,
        finals="BackendOptions.url StorageOptions.label BackendOptions.appVersion BackendOptions.credentials",
    )
    add(sdk, "StorageOptions", "dbPath", "generated", ref, finals="location")
    add(
        sdk,
        "StorageOptions",
        "dbEncryptionKey",
        "approved removal" if sdk == "Browser" else "generated",
        ref,
        finals="encryptionKey" if sdk == "Node" else None,
        note="Browser accepted this key but ignored it." if sdk == "Browser" else "",
    )
    if sdk == "Node":
        add(
            sdk,
            "StorageOptions",
            "maxDbPoolSize minDbPoolSize useSingleConnection",
            "generated",
            ref,
            finals="pool.max pool.min singleConnection",
        )
    add(
        sdk,
        "DeviceSyncOptions",
        "disableDeviceSync",
        "generated",
        ref,
        finals="ClientOptions.deviceSync.enabled",
    )
    add(
        sdk,
        "ContentOptions",
        "codecs",
        "static runtime",
        ref,
        finals="ClientOptions.codecs",
    )
    add(
        sdk,
        "OtherOptions",
        "structuredLogging loggingLevel workerConfig disableAutoRegister",
        "generated",
        ref,
        finals="LoggingOptions.structured LoggingOptions.level ClientOptions.workers ClientOptions.registration.auto",
    )
    add(
        sdk,
        "OtherOptions",
        "waitForRegistrationVisible",
        "approved removal",
        ref,
        note="Registration always waits for visibility; the old option leaves the final API.",
    )
    if sdk == "Node":
        add(
            sdk,
            "OtherOptions",
            "otelEndpoint otelServiceName otelSampleRatio resourceAttributes",
            "generated",
            ref,
            finals="LoggingOptions.otel.endpoint LoggingOptions.otel.serviceName LoggingOptions.otel.sampleRatio LoggingOptions.resourceAttributes",
        )
        add(
            sdk,
            "OtherOptions",
            "nonce",
            "generated",
            ref,
            finals="ClientOptions.registration.nonce",
        )
        add(
            sdk,
            "OtherOptions",
            "unstableChangeCallbacks unstableChangeCallbacks.appData",
            "approved removal",
            ref,
        )
    else:
        add(
            sdk,
            "OtherOptions",
            "performanceLogging",
            "generated",
            ref,
            finals="LoggingOptions.performance",
        )
    add(
        sdk,
        "EnrichedReply",
        "referenceId content inReplyTo",
        "generated",
        "11.4 Node, Conversation, Group, Dm",
        finals="ReplyParent.referenceID ReplyParent.content Message.inReplyTo",
    )
    add(
        sdk,
        "EnrichedReply",
        "contentType",
        "generated",
        "11.4 Browser" if sdk == "Browser" else "11.4 Node, Conversation, Group, Dm",
    )
    add(
        sdk,
        "Signer",
        "type signMessage getIdentifier",
        "generated",
        "11.4 Node, Client and options",
        finals="kind sign identity",
    )
    add(
        sdk,
        "StreamOptions",
        "retryAttempts retryDelay retryOnFail onFail onRetry onRestart disableSync",
        "approved removal",
        "11.4 Node, Conversations",
    )
    add(
        sdk,
        "StreamOptions",
        "onError onEnd onValue",
        "static runtime",
        "11.4 Node, Conversations",
        finals="onError onClose onValue",
    )

add(
    "Kotlin",
    "Client.Companion",
    "register",
    "approved removal",
    "11.4 Kotlin, Client and options",
    note="The global codec registration method moves to ClientOptions.codecs; Client.register() registers an identity.",
)
for sdk in ("Swift", "Kotlin"):
    add(
        sdk,
        "GroupSyncSummary",
        "numEligible numSynced",
        "generated",
        "11.1, GroupSyncSummary",
        finals="eligible synced",
    )
for sdk in ("Node", "Browser"):
    ref = (
        "11.4 Node, Client and options"
        if sdk == "Node"
        else "11.4 Browser; 11.4 Node, Client and options"
    )
    add(sdk, "", "Identifier", "generated", ref, finals="PublicIdentity")
    add(
        sdk,
        "",
        "SendOpts SendMessageOpts",
        "generated",
        "11.4 Node, Conversation, Group, Dm",
        finals="SendOptions SendOptions",
        note="The two option shapes merge into one SendOptions record.",
    )


RECORD_ROOTS = {
    "ClientOptions",
    "BackendOptions",
    "StorageOptions",
    "DeviceSyncOptions",
    "RegistrationOptions",
    "ForkRecoveryOptions",
    "WorkerOptions",
    "ServerConfiguration",
    "NotificationConfig",
    "NotificationState",
    "NotificationOverride",
    "ConsentRecord",
    "ConsentState",
    "ConsentEntity",
    "EntryType",
    "InboxState",
    "Installation",
    "Member",
    "PermissionPolicySet",
    "GroupMembershipResult",
    "GroupMembershipCapabilities",
    "DisappearingMessageSettings",
    "ArchiveOptions",
    "ArchiveElement",
    "ArchiveMetadata",
    "ConversationDebugInfo",
    "ApiStats",
    "IdentityStats",
    "EncodedContent",
    "ContentTypeID",
    "ContentTypeId",
    "GroupUpdated",
    "SendOptions",
    "RemoteAttachment",
    "MultiRemoteAttachment",
    "Attachment",
    "Reaction",
    "Reply",
    "TransactionReference",
    "ServerConfiguration",
    "MessageMetadata",
    "StreamFailureDetails",
    "DeliveryCursor",
    "CatchUpSummary",
    "PublicIdentity",
    "IdentityKind",
    "SigningKey",
    "SignedData",
    "SignerType",
    "SignatureRequest",
    "Signer",
    "ConversationKind",
    "ConversationOrder",
    "ConversationType",
    "MessageKind",
    "Credential",
    "AuthConfiguration",
    "RetentionConfiguration",
    "LimitsConfiguration",
    "MlsConfiguration",
    "NotificationChannel",
    "GroupSyncSummary",
    "MessageHistorySnapshot",
    "EncryptedEncodedContent",
    "ReadReceipt",
    "LeaveRequest",
    "DeleteMessageRequest",
    "DeletedMessage",
    "WalletSendCalls",
    "AppDataChange",
    "SignatureKind",
    "StandardContentType",
    "Intent",
    "Actions",
}


REMOVED_TYPES = {
    "Swift": {
        "ClientError",
        "ConversationError",
        "Dm.ConversationError",
        "NotificationError",
        "VisibilityConfirmationOptions",
        "MessageVisibilityOptions",
        "ConversationFilterType",
        "UnstableGroup",
        "AppDataChangeHandler",
        "Topic",
        "KeyUtil",
        "PrivateKey",
        "EncodedContentCompression",
        "EntryType",
        "ServerConfigurationError",
        "ConfigurationUnavailableError",
        "ConfigurationInvalidError",
        "BackendMismatchError",
        "ClientVersionTooOldError",
        "AuthRequiredError",
        "ChainNotAcceptedError",
    },
    "Kotlin": {
        "XMTPException",
        "NotificationError",
        "VisibilityConfirmationOptions",
        "MessageVisibilityOptions",
        "Conversations.ConversationFilterType",
        "UnstableGroup",
        "AppDataChangeHandler",
        "Topic",
        "KeyUtil",
        "PrivateKeyBuilder",
        "PrivateKey",
        "Crypto",
        "EncodedContentCompression",
        "EntryType",
        "DelicateApi",
        "UnstableApi",
        "Util",
        "ConfigurationUnavailableException",
        "ConfigurationInvalidException",
        "BackendMismatchException",
        "ClientVersionTooOldException",
        "AuthRequiredException",
        "ChainNotAcceptedException",
    },
    "Node": {
        "VisibilityConfirmationOptions",
        "StreamFailedError",
        "StreamInvalidRetryAttemptsError",
    },
    "Browser": {
        "VisibilityConfirmationOptions",
        "SafeConversation",
        "SafeSigner",
        "WorkerBridge",
        "WorkerAuth",
        "Opfs",
        "StreamFailedError",
        "StreamInvalidRetryAttemptsError",
        "GroupNotFoundError",
        "StreamNotFoundError",
        "OpfsNotInitializedError",
        "OpfsInitializationError",
    },
}


def codec_member(sdk: str, name: str, ref: str, note: str) -> Decision:
    """A codec member under the typed host protocol: the content type is
    `type`, and the Swift value type is the `Value` associated type."""
    owner, _, member = name.partition(".")
    renames = {"contentType": "type", "T": "Value"}
    if member in renames:
        renamed = f"{owner}.{renames[member]}"
        note = f"{note} Renamed to `{renamed}` by the typed host protocol.".strip()
        return decision("static runtime", renamed, ref, note)
    return decision("static runtime", spelling(name), ref, note)


def spelling(name: str) -> str:
    name = re.sub(
        r"(?:^|(?<=\.))unsafe_([a-z])",
        lambda m: (
            ("." if m.group(0).startswith(".") else "") + "unsafe" + m.group(1).upper()
        ),
        name,
    )
    return name


def stock_final(name: str) -> str:
    name = re.sub(r"IDs(?=[A-Z]|\b)", "Ids", name)
    return re.sub(r"ID(?=[A-Z]|\b)", "Id", name)


def enum_value(name: str) -> str:
    if name.isupper() and "_" not in name:
        return name.lower()
    parts = name.lower().split("_")
    if len(parts) > 1:
        return parts[0] + "".join(part.title() for part in parts[1:])
    return name[:1].lower() + name[1:]


def covered_open_export(sdk: str, name: str) -> Decision | None:
    """Map exports named by the schema even when no 11.4 table lists them."""
    if sdk in {"Swift", "Kotlin"}:
        if name == "ForkRecoveryPolicy" or name.startswith("ForkRecoveryPolicy."):
            leaf = name.partition(".")[2]
            return decision(
                "generated",
                "ForkRecoveryOptions.policy" + ("." + enum_value(leaf) if leaf else ""),
                "11.1, ForkRecoveryOptions",
            )
        if name == "DbPoolOptions" or name.startswith("DbPoolOptions."):
            leaf = name.partition(".")[2]
            if leaf == "init":
                return decision(
                    "approved removal",
                    "—",
                    "11.1, StorageOptions.pool",
                    "Old constructor folds into StorageOptions.",
                )
            target = {"maxPoolSize": "max", "minPoolSize": "min"}.get(leaf, leaf)
            return decision(
                "generated",
                "StorageOptions.pool" + ("." + target if leaf else ""),
                "11.1, StorageOptions.pool",
            )
        if name == "GroupMembershipState" or name.startswith("GroupMembershipState."):
            leaf = name.partition(".")[2]
            if leaf == "Companion":
                return decision(
                    "approved removal",
                    "—",
                    "11.2, GroupState.membershipState",
                    "Converter companion is internal.",
                )
            return decision(
                "generated",
                "GroupMembershipState" + ("." + enum_value(leaf) if leaf else ""),
                "11.2, GroupState.membershipState",
            )
        if name == "MlsExtensionType" or name.startswith("MlsExtensionType."):
            return decision(
                "generated",
                name,
                "11.2, Group.membershipCapabilities",
                "Extension type is part of GroupMembershipCapabilities.",
            )
        if name in {"InstallationCapabilities", "InboxCapabilities"} or name.startswith(
            ("InstallationCapabilities.", "InboxCapabilities.")
        ):
            if name.endswith(".init"):
                return decision(
                    "approved removal",
                    "—",
                    "11.2, Group.membershipCapabilities",
                    "FFI wrapper constructor is internal.",
                )
            return decision(
                "generated",
                spelling(name),
                "11.2, Group.membershipCapabilities",
                "Generated capability record.",
            )
        if name == "PermissionLevel" or name.startswith("PermissionLevel."):
            leaf = name.partition(".")[2]
            final = "Member.permissionLevel" + ("." + enum_value(leaf) if leaf else "")
            return decision("generated", final, "11.2, Member.permissionLevel")
        if name == "PermissionOption" or name.startswith("PermissionOption."):
            leaf = name.partition(".")[2]
            if leaf == "Companion":
                return decision(
                    "approved removal",
                    "—",
                    "11.2, PermissionPolicy",
                    "Conversion helpers become internal.",
                )
            final = "PermissionPolicy" + ("." + enum_value(leaf) if leaf else "")
            if leaf:
                return Decision(
                    "generated",
                    final,
                    "open",
                    "11.2 names PermissionPolicy but does not specify this case.",
                    True,
                )
            return decision(
                "generated",
                final,
                "11.2, Group.updatePermission and PermissionPolicySet",
            )
        if name == "GroupPermissionPreconfiguration" or name.startswith(
            "GroupPermissionPreconfiguration."
        ):
            leaf = name.partition(".")[2]
            if leaf == "Companion":
                return decision(
                    "approved removal",
                    "—",
                    "11.2, CreateGroupOptions.permissions",
                    "Conversion helpers become internal.",
                )
            return decision(
                "generated",
                "CreateGroupOptions.permissions"
                + ("." + enum_value(leaf) if leaf else ""),
                "11.2, CreateGroupOptions.permissions",
            )
        if name == "CommitLogForkStatus" or name.startswith("CommitLogForkStatus."):
            leaf = name.partition(".")[2]
            return decision(
                "generated",
                "ConversationState.commitLogForkStatus" + ("." + leaf if leaf else ""),
                "11.2, ConversationState",
            )
        if name == "MessageDeliveryStatus" or name.startswith("MessageDeliveryStatus."):
            leaf = name.partition(".")[2]
            if leaf.lower() == "all":
                return decision(
                    "approved removal",
                    "—",
                    "11.2, ListMessagesOptions",
                    "An absent filter includes all statuses.",
                )
            return decision(
                "generated",
                "DeliveryStatus" + ("." + enum_value(leaf) if leaf else ""),
                "11.2, MessageData.deliveryStatus",
            )
        if name in {"SortDirection", "MessageSortBy"} or name.startswith(
            ("SortDirection.", "MessageSortBy.")
        ):
            root, _, leaf = name.partition(".")
            target = (
                "ListMessagesOptions.direction"
                if root == "SortDirection"
                else "ListMessagesOptions.sortBy"
            )
            value = {"SENT_TIME": "sentAt", "INSERTED_TIME": "insertedAt"}.get(
                leaf, enum_value(leaf)
            )
            return decision(
                "generated",
                target + ("." + value if leaf else ""),
                "11.2, ListMessagesOptions",
            )
        if name in {
            "StreamFailureKind",
            "StreamBarrierReason",
            "StreamBarrierCauseKind",
            "StreamBarrierCause",
            "StreamBarrierTopic",
            "StreamBarrierFailure",
        }:
            return decision(
                "generated",
                name,
                f"11.4 {sdk}, Messages, codecs, preferences, values; 11.2, readers",
            )
        if name == "MessageCatchUpSnapshot":
            return decision(
                "generated", "CatchUp", "11.2, MessageReader.catchUpSnapshot"
            )
        if name == "SigningKeyDescription" or name.startswith("SigningKeyDescription."):
            leaf = name.partition(".")[2]
            return decision(
                "generated",
                "AuthConfiguration.keys" + ("." + leaf if leaf else ""),
                "11.1, AuthConfiguration",
            )
        if name == "MultiRemoteAttachmentError" or name.startswith(
            "MultiRemoteAttachmentError."
        ):
            return decision(
                "approved removal",
                "—",
                "11.1, XmtpError; 11.4 Swift, Client and options",
                "The error folds into the common error model.",
            )
        if name == "CipherText":
            return decision(
                "approved removal",
                "—",
                f"11.4 {sdk}, Messages, codecs, preferences, values; 11.1, attachment encryption",
                "The protobuf ciphertext wrapper is replaced by Rust encryption records.",
            )
        if sdk == "Swift" and name == "String.hexToData":
            return decision(
                "approved removal",
                "—",
                "11.1, ID custom types",
                "Validated ID conversion replaces the old hex helper.",
            )
        if sdk == "Kotlin" and name in {
            "Conversation.client",
            "Conversations.client",
            "Group.client",
            "Dm.client",
        }:
            return decision(
                "approved removal",
                "—",
                "11.1-11.2, generated live objects",
                "The old wrapper's internal client field is not public on the facade.",
            )
        if name == "PreferenceType" or name.startswith("PreferenceType."):
            return decision(
                "approved removal",
                "—",
                "11.8, hmacKeysUpdated event",
                "The preference stream becomes a live event.",
            )
    if sdk == "Kotlin" and name == "Throwable.streamFailureDetails":
        return decision(
            "approved removal",
            "—",
            "11.4 Kotlin, Client and options",
            "Typed XmtpException details replace this extension.",
        )
    if sdk in {"Node", "Browser"}:
        if (
            name == "ResolveValue"
            or name.startswith("ResolveValue.")
            or name == "AsyncStreamProxy"
            or name.startswith("AsyncStreamProxy.")
        ):
            return decision("static runtime", name, "11.2, Stream; 5, reader adapters")
        if (
            name == "MessageAcknowledgement"
            or name.startswith("MessageAcknowledgement.")
            or name == "MessageDelivery"
            or name.startswith("MessageDelivery.")
        ):
            return decision(
                "static runtime", name, "11.2, MessageStream; 5, reader adapters"
            )
        if name == "MessageReaderSource" or name.startswith("MessageReaderSource."):
            leaf = name.partition(".")[2]
            if leaf in {"conversationType", "consentStates"}:
                return decision(
                    "approved removal",
                    "—",
                    "11.2, MessageReader.updateFilter",
                    "The reader filter is set through updateFilter, without public adapter fields.",
                )
            target = {"nextDelivery": "next", "close": "end()"}.get(leaf, leaf)
            return decision(
                "generated",
                "MessageReader" + ("." + target if leaf else ""),
                "11.2, MessageReader",
            )
        if name in {
            "Client.libxmtpVersion",
            "Client.appVersion",
            "Client.options",
            "Client.installationIdBytes",
        }:
            return decision("generated", name, "11.1, Client immutable fields")
        if name == "Client.signer":
            return decision(
                "approved removal",
                "—",
                "11.1, Client and Signer",
                "The client does not expose its signer.",
            )
        if name == "Client.accountIdentifier":
            return decision("generated", "Client.identity", "11.1, Client.identity")
        if name in {"Conversations.topic", "Conversations.options"}:
            return decision(
                "approved removal",
                "—",
                "11.2, Conversations",
                "These adapter fields are not on the final Conversations object.",
            )
        if name in {"EOASigner", "EOASigner.type", "SCWSigner", "SCWSigner.type"}:
            return decision(
                "approved removal",
                "—",
                "11.4 Node, Client and options; 11.4 Browser; 11.1, Signer",
                "The signer foreign trait and app wallet signer replace these old shapes.",
            )
        if (
            name == "ServerConfigurationError"
            or name.startswith("ServerConfigurationError.")
            or name in {"throwServerConfigurationError", "toServerConfigurationError"}
        ):
            return decision(
                "approved removal",
                "—",
                "11.4 Node, Client and options; 11.1, XmtpError",
                "Typed XmtpError variants replace the old wrapper.",
            )
        if (
            name == "StreamFailureCause"
            or name.startswith("StreamFailureCause.")
            or name == "UnfinishedStreamTopic"
            or name.startswith("UnfinishedStreamTopic.")
            or name == "StreamBarrierFailure"
            or name.startswith("StreamBarrierFailure.")
        ):
            return decision(
                "generated",
                name,
                "11.4 Node, Client and options; 11.2, stream failure details",
                "XmtpError.streamFailure carries the typed record.",
            )
        if sdk == "Browser" and name == "fetchServerConfiguration":
            return decision(
                "generated",
                "Client.fetchServerConfiguration",
                "11.1, Client static methods",
            )
    return None


# Old Node and browser binding re-exports that the facade package roots do not
# export under the same name. Each is a rename to the listed public name or a
# removal with its replacement. check-manifest-roots.py checks that every
# other re-export row names a real root export.
BINDING_REEXPORT_RENAMES: dict[str, tuple[str, str]] = {
    "BackupElementSelectionOption": ("ArchiveElement", ""),
    "Consent": ("ConsentRecord", ""),
    "ConsentEntityType": ("ConsentEntityKind", ""),
    "ConversationType": ("ConversationKind", ""),
    "GroupMember": ("Member", ""),
    "GroupMembershipState": ("MembershipState", ""),
    "GroupMessageKind": ("MessageKind", ""),
    "GroupPermissionsOptions": ("GroupPermissionMode", ""),
    "Lifetime": ("KeyPackageLifetime", ""),
    "ListConversationsOrderBy": ("ConversationOrder", ""),
    "LogOptions": ("LoggingOptions", ""),
    "MessageDisappearingSettings": ("DisappearingSettings", ""),
    "MetadataField": ("MetadataFieldKind", ""),
    "PermissionUpdateType": ("PermissionUpdateKind", ""),
    "SignatureRequestHandle": ("SignatureRequest", ""),
    "SortDirection": ("MessageOrder", ""),
    "WorkerConfigOptions": ("WorkerOptions", ""),
    "WorkerIntervalOverride": ("WorkerInterval", ""),
    "DeliveryCursor": (
        "DeliveryCursor",
        "It is now an opaque string alias, not a record.",
    ),
    "WorkerJitterOverride": (
        "WorkerInterval",
        "The jitter is `WorkerInterval.jitterNs`.",
    ),
    **{
        f"contentType{kind}": (
            "standardContentType",
            f"Pass the standard content kind (`{kind[0].lower() + kind[1:]}`).",
        )
        for kind in (
            "Actions Attachment GroupUpdated Intent LeaveRequest Markdown "
            "MultiRemoteAttachment Reaction ReadReceipt RemoteAttachment Reply "
            "Text TransactionReference WalletSendCalls"
        ).split()
    },
    **{
        f"encode{kind}": (
            "encodeStandard",
            f"Pass a `{kind[0].lower() + kind[1:]}` StandardContent value, or use `{kind}Codec`.",
        )
        for kind in (
            "Actions Attachment Intent Markdown MultiRemoteAttachment Reaction "
            "ReadReceipt RemoteAttachment TransactionReference WalletSendCalls"
        ).split()
    },
}
BINDING_REEXPORT_REMOVALS: dict[str, str] = {
    "AppDataChange": "A received `GroupUpdated` message lists `metadataFieldChanges`; the entry whose `fieldName` is `app_data` has `oldValue` and `newValue`. The `conversationMetadataChanged` `ClientEvent` names the conversation and its changed fields.",
    "BackendBuilder": "The `BackendOptions` record replaces the builder; `Backend.connect(options)` opens a shared backend.",
    "ContentType": "Message filters take `ContentTypeId` lists (`ListMessagesOptions.contentTypes`); `StandardContentKind` names the standard kinds.",
    "ConversationListItem": "`Conversations.list` returns `Conversation` values (`Group | Dm`).",
    "Cursor": "The debug cursor is `ConversationDebugInfo.cursor`; delivery positions are delivery cursor strings (`Message.deliveryCursor`).",
    "EncryptedAttachment": "`encryptBytes` returns `EncryptedEncodedContent` (ciphertext and `EncryptionKeys`); `RemoteAttachmentCodec` encodes the reference.",
    "GroupMetadata": "`Group.creatorInboxId` and `Conversation.kind` replace the metadata object.",
    "Inbox": "`GroupUpdated` lists members as `InboxId` strings.",
    "MessageCatchUp": "Retired delivery diagnostics: streams report `ConnectionState` and `Client.catchUpToLive` returns `CatchUpSummary`.",
    "MessageCatchUpGeneration": "Retired delivery diagnostics: streams report `ConnectionState` and `Client.catchUpToLive` returns `CatchUpSummary`.",
    "MessageTopicStatus": "Retired delivery diagnostics: streams report `ConnectionState` and `Client.catchUpToLive` returns `CatchUpSummary`.",
    "MessageHistorySnapshot": "Each history `Message` carries its `deliveryCursor`; a reader opened `from` a cursor replays after it.",
    "ReadReceipt": "Read receipt content is `StandardContent` of kind `readReceipt`; `ReadReceiptCodec` encodes it.",
    "Reply": "Reply content is `StandardContent` of kind `reply`, and `Message.replyContent`; `ReplyCodec` encodes it.",
    "UserPreferenceUpdate": "`Client.events` delivers `consentChanged` and `hmacKeysUpdated` `ClientEvent` values.",
}


def binding_reexport_outcome(sdk: str, name: str) -> Decision | None:
    """The public outcome of an old binding re-export that has no same-name
    root export."""
    ref = (
        "11.4 Node, Messages, codecs, preferences, values"
        if sdk == "Node"
        else "11.4 Browser; 11.4 Node, Messages, codecs, preferences, values"
    )
    if name in BINDING_REEXPORT_RENAMES:
        final, note = BINDING_REEXPORT_RENAMES[name]
        return decision(
            "generated",
            final,
            ref,
            f"Renamed in the facade package root. {note}".strip(),
        )
    if name in BINDING_REEXPORT_REMOVALS:
        # A proposal, not an owner decision: Open items lists it.
        return Decision(
            "proposed removal",
            "—",
            ref,
            f"Proposed removal. Replacement: {BINDING_REEXPORT_REMOVALS[name]}",
            open=True,
        )
    return None


def _classify(entry: object) -> Decision:
    sdk, name, kind, source = entry.sdk, entry.name, entry.kind, entry.source
    # Task 14 and CTYPE section 6 settle these Kotlin cutover rows.
    if sdk == "Kotlin" and name in {
        "Fetcher",
        "Fetcher.fetch",
        "HTTPFetcher",
        "HTTPFetcher.fetch",
        "RemoteAttachment.fetcher",
        "RemoteAttachment.load",
    }:
        return decision(
            "approved removal",
            "—",
            "[Task 14](https://plan.ref.tools/TiFDtuzx3U19olnv), Decisions and step 2",
            "Remove the blocking download helper. Use the generated attachment transfer API.",
        )
    if sdk == "Kotlin" and name in {
        "ReactionCodec",
        "ReactionCodec.contentType",
        "ReactionCodec.decode",
        "ReactionCodec.encode",
        "ReactionCodec.fallback",
        "ReactionCodec.shouldPush",
    }:
        return decision(
            "approved removal",
            "—",
            "[CTYPE section 6](../specs/CTYPE-content-types.md#6-content-type-versions), CTYPE-016; 11.4 Kotlin",
            "The approved catalogue excludes xmtp.org/reaction:1.0. Use ReactionV2Codec for xmtp.org/reaction:2.0.",
        )
    kotlin_helpers = {
        "ByteArray.toHex": "Retain this helper in uniffi.xmtp_sdk. Keep lower-case hexadecimal output.",
        "String.hexToByteArray": "Retain this helper in uniffi.xmtp_sdk. Keep the old prefix, odd-nibble, and character conversion behavior.",
        "func validateInboxId": "Retain this helper in uniffi.xmtp_sdk. Reject the 0x prefix without a case distinction. Keep the typed invalid-argument error and old diagnostic.",
        "func validateInboxIds": "Retain this helper in uniffi.xmtp_sdk. Apply validateInboxId to each inbox ID.",
    }
    if sdk == "Kotlin" and name in kotlin_helpers:
        return decision(
            "platform helper",
            name.removeprefix("func "),
            "[Task 14](https://plan.ref.tools/TiFDtuzx3U19olnv), retained pure Kotlin helpers",
            kotlin_helpers[name],
        )
    if sdk in {"Node", "Browser"} and kind == "binding re-export":
        outcome = binding_reexport_outcome(sdk, name.removeprefix("func "))
        if outcome is not None:
            return outcome
    if sdk == "Swift" and not source.endswith("/xmtpv3.swift"):
        if re.search(r"\.(?:toFfi|fromFfi)$", name):
            return decision(
                "approved removal",
                "—",
                "11.4 Swift, Messages, codecs, preferences, values",
                "FFI converter becomes internal plumbing.",
            )
        if kind == "init" and name.split(".", 1)[0] in RECORD_ROOTS:
            source_lines = (
                (Path(__file__).resolve().parents[2] / source).read_text().splitlines()
            )
            signature = " ".join(source_lines[entry.line - 1 : entry.line + 5]).split(
                "{", 1
            )[0]
            if re.search(r"\bFfi[A-Za-z0-9_]+\b", signature):
                return decision(
                    "approved removal",
                    "—",
                    "11.4 Swift, Messages, codecs, preferences, values",
                    "FFI constructor becomes internal plumbing.",
                )
    if (
        sdk == "Kotlin"
        and kind in {"val", "var"}
        and name.split(".", 1)[0] in {"Conversation", "Group", "Dm"}
    ):
        source_lines = (
            (Path(__file__).resolve().parents[2] / source).read_text().splitlines()
        )
        before = source_lines[max(0, entry.line - 9) : entry.line - 1]
        if any("@Deprecated" in line for line in before):
            return decision(
                "approved removal",
                "—",
                "11.4 Kotlin, Conversation, Group, Dm",
                "Deprecated blocking property is removed; use state().",
            )
    if kind == "generated family":
        if "Proto/" in source:
            return decision(
                "approved removal",
                "—",
                "2, removed protobuf sources",
                "Pattern and count are in Current export.",
            )
        if "Converter" in name or "internal" in name:
            return decision(
                "approved removal",
                "—",
                "2, generated bridge replacement",
                "Internal UniFFI plumbing; pattern and count are in Current export.",
            )
        if "Callback" in name:
            return decision(
                "approved removal",
                "—",
                "11.1; 11.8, foreign traits",
                "Old callbacks are replaced by facade foreign traits.",
            )
        return decision(
            "approved removal",
            "—",
            "2, generated bridge replacement",
            "Old generated binding is replaced by facade output.",
        )
    if sdk in {"Swift", "Kotlin", "Node", "Browser"} and name in {
        "Client.canMessage",
        "Client.Companion.canMessage",
    }:
        return decision(
            "generated",
            spelling(name),
            f"11.4 {sdk}, Client and options"
            if sdk != "Browser"
            else "11.4 Browser; 11.4 Node, Client and options",
            "Behavior change: result keys use `ethereum:<core text>` or "
            "`passkey:<lowercase core hex>` so identity kinds do not collide.",
        )
    if (sdk, name) in RULES:
        rule = RULES[(sdk, name)]
        return Decision(rule.status, spelling(rule.final), rule.ref, rule.note)
    if sdk == "Kotlin" and ".Companion." in name:
        normalized = name.replace(".Companion.", ".")
        if (sdk, normalized) in RULES:
            rule = RULES[(sdk, normalized)]
            return Decision(rule.status, spelling(rule.final), rule.ref, rule.note)
    if sdk == "Kotlin" and name.startswith("Client.ffi"):
        companion = (sdk, name.replace("Client.", "Client.Companion.", 1))
        if companion in RULES:
            rule = RULES[companion]
            return Decision(
                rule.status,
                spelling(rule.final.replace("Client.Companion.", "Client.")),
                rule.ref,
                rule.note,
            )
    if name == "ClientOptions.Api":
        return decision(
            "generated", "BackendOptions", f"11.4 {sdk}, Client and options"
        )
    if name == "ClientOptions":
        return decision(
            "static runtime",
            "ClientOptions",
            f"11.4 {sdk}, Client and options",
            "Host options wrapper holds codecs and callbacks.",
        )
    if name in {"InboxId", "InboxID"}:
        return decision(
            "generated",
            "InboxID",
            f"11.4 {sdk}, Messages, codecs, preferences, values; plan Decisions",
        )
    if sdk in {"Swift", "Kotlin"} and name.startswith("Ffi") and kind == "typealias":
        return decision(
            "generated",
            spelling(name[3:]),
            f"11.4 {sdk}, Messages, codecs, preferences, values",
            "Old Ffi typealias is replaced by a facade type.",
        )
    if name.startswith("Conversations.ListConversationsOrderBy"):
        return decision(
            "generated",
            name.replace("Conversations.ListConversationsOrderBy", "ConversationOrder"),
            f"11.4 {sdk}, Conversations",
        )
    if name == "SigningKey" or name.startswith("SigningKey."):
        target = (
            name.replace("SigningKey", "Signer", 1)
            .replace("publicIdentity", "identity")
            .replace("type", "kind")
        )
        return decision(
            "generated", spelling(target), f"11.4 {sdk}, Client and options; 11.1"
        )
    if name == "SignedData" or name.startswith("SignedData."):
        return decision(
            "generated",
            spelling(name.replace("SignedData", "Signature", 1)),
            f"11.4 {sdk}, Client and options; 11.1",
        )
    if name == "SignerType" or name.startswith("SignerType."):
        return decision(
            "generated",
            spelling(name.replace("SignerType", "SignerKind", 1)),
            f"11.4 {sdk}, Client and options; 11.1",
        )
    if name == "PreEventCallback" or name.startswith("PreEventCallback."):
        return decision(
            "generated",
            name.replace(
                "PreEventCallback", "ClientOptions.handlers.preAuthenticate", 1
            ),
            f"11.4 {sdk}, Client and options",
        )
    if name == "UnstableChangeCallbacks" or name.startswith("UnstableChangeCallbacks."):
        return decision(
            "approved removal",
            "—",
            f"11.4 {sdk}, Client and options; 11.8",
            "App-data fields and events replace callbacks.",
        )
    if name.endswith((".toIdentityKind", ".toFfiPublicIdentifierKind")) and sdk in {
        "Swift",
        "Kotlin",
    }:
        return decision(
            "approved removal",
            "—",
            f"11.4 {sdk}, Messages, codecs, preferences, values",
            "FFI converter is internal plumbing.",
        )
    if (
        name in {"Error.serverConfigurationError", "Error.streamFailureDetails"}
        and sdk == "Swift"
    ):
        return decision(
            "approved removal",
            "—",
            "11.4 Swift, Client and options",
            "Typed XmtpError details replace this extension.",
        )
    if name == "ProcessType" and sdk == "Kotlin":
        return decision(
            "generated",
            "ProcessType",
            "11.4 Kotlin, Messages, codecs, preferences, values",
            "Old FfiProcessType alias is replaced by a facade type.",
        )
    if name == "AuthCallback" or name.startswith("AuthCallback."):
        return decision("generated", spelling(name), "11.1, credential foreign trait")
    if name in {"Client", "Conversations", "Conversation", "Group", "Dm"}:
        return decision("generated", name, "11.1-11.2, live objects")
    if name == "ConversationsOrderBy" or name.startswith("ConversationsOrderBy."):
        return decision(
            "generated",
            spelling(name.replace("ConversationsOrderBy", "ConversationOrder", 1)),
            f"11.4 {sdk}, Conversations",
        )
    if name == "Client.LogLevel" or name.startswith("Client.LogLevel."):
        return decision(
            "generated",
            spelling(name.replace("Client.LogLevel", "LogLevel", 1)),
            "11.1, process logging",
        )
    if name == "Client.Companion":
        return decision("generated", "Client", "11.1, client static functions")
    if name == "Client.Companion.IN_MEMORY_DB_PATH":
        return decision(
            "approved removal",
            "—",
            "11.4 Kotlin, Client and options",
            "In-memory storage uses StorageOptions.location.",
        )
    if name == "Client.Companion.codecRegistry":
        return decision(
            "approved removal",
            "—",
            "11.4 Kotlin, Client and options",
            "Global codec registry moves to ClientOptions.codecs.",
        )
    if name == "ClientOptions.Api" or name.startswith("ClientOptions.Api."):
        return decision(
            "generated",
            spelling(name.replace("ClientOptions.Api", "BackendOptions", 1)),
            f"11.4 {sdk}, Client and options",
        )
    if name.startswith("XMTPDebugInformation") or name.startswith("DebugInformation"):
        if name.endswith(".constructor"):
            return decision(
                "approved removal",
                "—",
                f"11.4 {sdk}, Client and options; 11.1, Diagnostics",
                "The client owns Diagnostics.",
            )
        if name.endswith("uploadDebugInformation"):
            return decision("approved removal", "—", f"11.4 {sdk}, Client and options")
        target = name.replace("XMTPDebugInformation", "Diagnostics", 1).replace(
            "DebugInformation", "Diagnostics", 1
        )
        target = (
            target.replace("apiIdentityStatistics", "identityStatistics")
            .replace("apiAggregateStatistics", "aggregateStatistics")
            .replace("clearAllStatistics", "clearStatistics")
        )
        return decision(
            "generated",
            spelling(target),
            f"11.4 {sdk}, Client and options; 11.1",
            "Diagnostics methods are asynchronous.",
        )
    if sdk in {"Swift", "Kotlin"} and (
        name
        in {
            "ServerConfigurationError",
            "ConfigurationUnavailableError",
            "ConfigurationInvalidError",
            "BackendMismatchError",
            "ClientVersionTooOldError",
            "AuthRequiredError",
            "ChainNotAcceptedError",
        }
        or name.startswith("ServerConfigurationError.")
    ):
        return decision(
            "approved removal",
            "—",
            f"11.4 {sdk}, Client and options",
            "Old configuration errors become XmtpError variants.",
        )
    if name == "Conversation.XMTPConversationType" or name.startswith(
        "Conversation.XMTPConversationType."
    ):
        return decision(
            "generated",
            name.replace("Conversation.XMTPConversationType", "ConversationKind", 1),
            f"11.4 {sdk}, Conversation, Group, Dm",
        )
    if name == "Conversation.Type" or name.startswith("Conversation.Type."):
        return decision(
            "generated",
            name.replace("Conversation.Type", "ConversationKind", 1),
            f"11.4 {sdk}, Conversation, Group, Dm",
        )
    if name in {
        "Conversation.group",
        "Conversation.dm",
        "Conversation.Group",
        "Conversation.Dm",
        "Conversation.Group.group",
        "Conversation.Dm.dm",
    }:
        return decision(
            "generated",
            spelling(name),
            f"11.4 {sdk}, Conversation, Group, Dm",
            "Tagged conversation variant stays.",
        )
    if name == "Conversation.type":
        return decision(
            "generated", "Conversation.kind", f"11.4 {sdk}, Conversation, Group, Dm"
        )
    if name in {
        "Conversation.==",
        "Conversation.hash",
        "Group.==",
        "Group.hash",
        "Dm.==",
        "Dm.hash",
        "Group.equals",
        "Group.hashCode",
        "Dm.equals",
        "Dm.hashCode",
    }:
        return decision("static runtime", name, "11.7, value equality")
    if name in {"Group.creatorInboxId", "Dm.creatorInboxId"}:
        return decision(
            "generated", spelling(name), "11.2, immutable conversation fields"
        )
    if name == "Group.permissions":
        return decision(
            "generated",
            "Group.state().permissions",
            f"11.4 {sdk}, Conversation, Group, Dm",
        )
    if sdk in {"Node", "Browser"} and name in {
        "NetworkOptions",
        "ContentOptions",
        "OtherOptions",
    }:
        target = {
            "NetworkOptions": "BackendOptions",
            "ContentOptions": "ClientOptions.codecs",
            "OtherOptions": "ClientOptions",
        }[name]
        return decision(
            "generated" if name == "NetworkOptions" else "approved removal",
            target if name == "NetworkOptions" else "—",
            "11.4 Node, Client and options",
            "Flat option types fold into ClientOptions.",
        )
    if sdk == "Node" and name == "OtherOptions.stdoutLoggingLevel":
        return Decision(
            "generated",
            name,
            "11.4 Node, Client and options; 11.1; open",
            "11.4 moves this to initLogging; 11.1 has no matching LoggingOptions field.",
            True,
        )
    if sdk in {"Node", "Browser"} and name in {
        "BuiltInContentTypes",
        "ExtractCodecContentTypes",
    }:
        return decision("static runtime", name, "4, typed custom codecs")
    if sdk in {"Node", "Browser"} and name == "EnrichedReply":
        return decision(
            "generated", "ReplyParent", "11.4 Node, Conversation, Group, Dm; 11.7"
        )
    if sdk in {"Node", "Browser"} and name in {
        "SignMessage",
        "GetIdentifier",
        "GetChainId",
        "GetBlockNumber",
    }:
        return decision(
            "approved removal",
            "—",
            "11.4 Node, Client and options",
            "The Signer foreign trait replaces these function types.",
        )
    if sdk in {"Node", "Browser"} and name == "Preferences":
        return decision("generated", "Preferences", "11.2, preferences")
    if (
        sdk in {"Node", "Browser"}
        and name.endswith(".constructor")
        and name.split(".", 1)[0]
        in {
            "InboxReassignError",
            "AccountAlreadyAssociatedError",
            "MissingContentTypeError",
            "SignerUnavailableError",
            "ClientNotInitializedError",
            "AuthRequiredError",
            "BackendMismatchError",
            "ChainNotAcceptedError",
            "ClientVersionTooOldError",
            "ConfigurationInvalidError",
            "ConfigurationUnavailableError",
        }
    ):
        return decision(
            "generated",
            spelling(name),
            "11.4 Node, Client and options",
            "Generated XmtpError subclass constructor.",
        )
    if name.startswith("ClientOptions.") and name.endswith(".init"):
        return decision(
            "static runtime", spelling(name), f"11.4 {sdk}, Client and options"
        )
    if source.endswith("/Libxmtp/xmtpv3.swift") and name.startswith("Ffi"):
        if name == "FfiXmtpClient.waitForRegistrationVisible":
            return Decision(
                "generated",
                "Client.waitForRegistrationVisible",
                "plan Decisions; open",
                "The standalone method keeps its behavior; design 11.4 does not name it.",
                True,
            )
        root, _, member = name.partition(".")
        if root.endswith("Callback") or root.endswith("CallbackImpl"):
            return decision(
                "approved removal",
                "—",
                "11.1; 11.8, foreign traits",
                "Old callback is replaced by a facade foreign trait.",
            )
        if not member:
            target = {
                "FfiXmtpClient": "Client",
                "FfiDecodedMessage": "Message",
                "FfiSendMessageOpts": "SendOptions",
                "FfiConversationMessageKind": "MessageKind",
            }.get(root, root[3:])
            return decision(
                "generated",
                spelling(target),
                "11.4 Swift, Messages, codecs, preferences, values",
                "Old binding type used in a public SDK signature.",
            )
        return decision(
            "approved removal",
            "—",
            "2, generated bridge replacement",
            "Old binding method is replaced by facade output; its root is listed separately.",
        )
    if source.endswith("/Libxmtp/xmtpv3.swift") and (
        name == "XmtpApiClient" or name.startswith("XmtpApiClient.")
    ):
        if name == "XmtpApiClient":
            return decision(
                "generated",
                "Backend",
                "11.4 Swift, Messages, codecs, preferences, values; 11.1",
                "Old API client used in a public SDK signature.",
            )
        return decision(
            "approved removal",
            "—",
            "2, generated bridge replacement",
            "Old API client member is replaced by Backend.",
        )
    if source.endswith("/Libxmtp/xmtpv3.swift"):
        return decision(
            "approved removal",
            "—",
            "2, generated bridge replacement",
            "Old binding helper used by a public SDK signature.",
        )
    if name in {"encryptAttachment", "decryptAttachment"} and sdk in {
        "Node",
        "Browser",
    }:
        target = "encryptBytes" if name.startswith("encrypt") else "decryptBytes"
        return decision(
            "generated",
            f"func {target}",
            f"11.4 {sdk}, Messages, codecs, preferences, values"
            if sdk == "Node"
            else "11.4 Browser; 11.4 Node, Messages, codecs, preferences, values",
        )
    if name.startswith("Client.unsafe_"):
        leaf = name.split(".", 1)[1]
        ref = "11.4 Browser" if sdk == "Browser" else f"11.4 {sdk}, Client and options"
        if sdk == "Browser" and leaf.endswith("SignatureText"):
            return decision(
                "generated",
                spelling(name[:-4] + "Request"),
                ref,
                "Signature text changes to a SignatureRequest object.",
            )
        return decision("generated", spelling(name), ref)
    if sdk == "Browser" and name == "Client.isReady":
        return decision("approved removal", "—", "11.4 Browser")
    if sdk == "Browser" and name == "Conversation.metadata":
        return decision(
            "generated",
            "Conversation.metadata",
            "11.4 Browser",
            "The immutable Conversation.creatorInboxId and Conversation.kind fields replace this method.",
        )
    if sdk == "Node" and name == "Conversation.metadata":
        return decision(
            "approved removal",
            "—",
            "11.4 Node, Conversation, Group, Dm",
            "Immutable creatorInboxID and kind fields replace metadata().",
        )
    if sdk == "Browser" and name == "Opfs":
        return decision(
            "approved removal",
            "—",
            "11.4 Browser",
            "Storage.admin() replaces the old class.",
        )
    if sdk == "Browser" and name.startswith("Opfs."):
        member = name.split(".", 1)[1]
        if member in {"constructor", "init", "close", "create", "fileCount"}:
            return decision(
                "approved removal",
                "—",
                "11.4 Browser",
                "This old class member has no StorageAdmin counterpart.",
            )
        return decision(
            "generated",
            "StorageAdmin." + ("capacity" if member == "poolCapacity" else member),
            "11.4 Browser; 11.2, StorageAdmin",
            "OPFS administration moves to the storage object.",
        )
    if sdk in {"Node", "Browser"} and name == "Conversation._client":
        return decision(
            "approved removal",
            "—",
            "11.4 Node, Conversation, Group, Dm; 11.2",
            "Internal constructor storage is not a facade field.",
        )
    if sdk in {"Node", "Browser"} and name == "IdentifierKind":
        return decision(
            "approved removal",
            "—",
            "11.4 Node, Client and options; 11.1, PublicIdentity",
            "PublicIdentity.kind replaces the separate enum.",
        )
    if sdk in {"Node", "Browser"} and name in {
        "createBackend",
        "generateInboxId",
        "getInboxIdForIdentifier",
    }:
        target = {
            "createBackend": "Backend.connect",
            "generateInboxId": "func generateInboxId",
            "getInboxIdForIdentifier": "Client.inboxID(for:)",
        }[name]
        return decision(
            "generated",
            target,
            "11.4 Node, Client and options"
            if sdk == "Node"
            else "11.4 Browser; 11.4 Node, Client and options",
        )
    if sdk in {"Node", "Browser"} and name in {"HexString", "validHex", "isHexString"}:
        return decision(
            "approved removal",
            "—",
            "11.4 Node, Client and options; 11.1, ID types",
            "Plain string IDs replace the hex helper.",
        )
    if sdk in {"Node", "Browser"} and name in {
        "DEFAULT_RETRY_DELAY",
        "DEFAULT_RETRY_ATTEMPTS",
        "createStream",
    }:
        return decision(
            "approved removal",
            "—",
            "11.4 Node, Conversations",
            "Stream retry knobs leave the public API.",
        )
    if sdk in {"Node", "Browser"} and name in {
        "StreamOptions",
        "MessageStreamOptions",
        "StreamCallback",
        "StreamFunction",
        "StreamValueMutator",
    }:
        return decision(
            "static runtime",
            spelling(name),
            "5, stream adapters; 11.4 Node, Conversations",
        )
    if sdk in {"Node", "Browser"} and name.startswith("MessageStream."):
        leaf = name.split(".", 1)[1]
        if leaf.startswith("retry") or leaf in {"onRetry", "onRestart"}:
            return decision(
                "approved removal",
                "—",
                "11.4 Node, Conversations",
                "Retry knob is removed.",
            )
        return decision(
            "static runtime",
            spelling(name),
            "5, stream adapters; 11.4 Node, unchanged list",
        )
    if sdk in {"Node", "Browser"} and name.startswith("Preferences."):
        return decision(
            "generated",
            spelling(name),
            "11.4 Node, Messages, codecs, preferences, values; 11.8",
        )
    if sdk == "Browser" and name in {
        "createEOASigner",
        "createSCWSigner",
        "toSafeSigner",
        "toSafeConversation",
    }:
        return decision(
            "approved removal",
            "—",
            "11.4 Browser",
            "LocalSigner or generated conversation replaces this helper.",
        )
    if sdk == "Browser" and name in {
        "GroupNotFoundError",
        "StreamNotFoundError",
        "OpfsNotInitializedError",
        "OpfsInitializationError",
    }:
        return decision(
            "approved removal", "—", "11.4 Browser", "Replaced by an XmtpError code."
        )
    if sdk == "Browser" and name in {"HmacKeys", "LastReadTimes"}:
        return decision(
            "approved removal",
            "—",
            "11.4 Browser",
            "Transport-only type is replaced by generated records.",
        )
    if sdk in {"Node", "Browser"} and name in {"AsyncStreamProxy", "ResolveValue"}:
        return decision("static runtime", name, "5, host stream adapter")
    if sdk in {"Node", "Browser"} and name in {
        "getStreamFailureDetails",
        "getErrorCode",
    }:
        return decision(
            "approved removal",
            "—",
            "11.4 Node, Client and options",
            "XmtpError exposes typed details.",
        )
    if sdk in {"Node", "Browser"} and (
        name.startswith("is")
        and name[2:3].isupper()
        or name.startswith(("encode", "contentType"))
    ):
        return decision(
            "static runtime", spelling(name), "11.4 Node, unchanged list; 4"
        )
    if sdk in {"Node", "Browser"} and (
        name == "NotificationError" or name.startswith("NotificationError.")
    ):
        return decision(
            "generated",
            name,
            "11.4 Node, Client and options",
            "Generated XmtpError subclass where the error still exists.",
        )
    if sdk in {"Node", "Browser"} and name in {
        "InboxReassignError",
        "AccountAlreadyAssociatedError",
        "MissingContentTypeError",
        "SignerUnavailableError",
        "ClientNotInitializedError",
        "AuthRequiredError",
        "BackendMismatchError",
        "ChainNotAcceptedError",
        "ClientVersionTooOldError",
        "ConfigurationInvalidError",
        "ConfigurationUnavailableError",
    }:
        return decision(
            "generated",
            name,
            "11.4 Node, Client and options",
            "Generated XmtpError subclass where the error still exists.",
        )
    if (
        sdk in {"Node", "Browser"}
        and name.startswith("Group.")
        and name.split(".", 1)[1]
        in {"addMembersByIdentifiers", "removeMembersByIdentifiers"}
    ):
        return decision(
            "generated",
            name.replace("ByIdentifiers", ""),
            "11.4 Node, Conversation, Group, Dm",
            "Identity forms are TypeScript unions (plan Decision 6).",
        )
    if sdk in {"Swift", "Kotlin"} and name in {
        "Group.addMembersByIdentity",
        "Group.removeMembersByIdentity",
    }:
        return decision(
            "generated",
            name.replace("ByIdentity", ""),
            f"11.4 {sdk}, Conversation, Group, Dm",
            "Identity forms are same-name overloads (plan Decision 6).",
        )
    if sdk in {"Node", "Browser"} and name in {
        "Client.constructor",
        "Client.init",
        "Conversations.constructor",
        "Conversation.constructor",
        "Group.constructor",
        "Dm.constructor",
    }:
        return decision(
            "approved removal",
            "—",
            "11.1-11.2, generated live objects",
            "Construction uses the client, conversation, and factory methods.",
        )
    if sdk in {"Node", "Browser"} and name == "Conversation.hmacKeys":
        return decision(
            "generated",
            name,
            "11.4 Node, Conversation, Group, Dm",
            "Read becomes asynchronous.",
        )
    if any(name == root or name.startswith(root + ".") for root in REMOVED_TYPES[sdk]):
        section = (
            "Conversations"
            if "ConversationFilterType" in name
            else "Conversation, Group, Dm"
            if name.startswith("UnstableGroup")
            else "Client and options"
            if name.startswith(
                (
                    "ClientError",
                    "NotificationError",
                    "XMTPException",
                    "Configuration",
                    "BackendMismatch",
                    "ClientVersion",
                    "AuthRequired",
                    "ChainNotAccepted",
                )
            )
            else "Messages, codecs, preferences, values"
        )
        return decision(
            "approved removal",
            "—",
            f"11.4 {sdk}, {section}",
            "The old type and its members leave the API.",
        )
    if name == "ContentCodec" or name.startswith("ContentCodec."):
        ref = f"11.4 {sdk}, Messages, codecs, preferences, values; 4"
        member = name.split(".", 1)[1] if "." in name else ""
        # The Host codecs protocol in the cutover plan (Ref vRG5sTDlgoKQ911m)
        # has only type, encode, decode, fallback, and shouldPush.
        if member in {"==", "hash", "id", "description"}:
            return Decision(
                "proposed removal",
                "—",
                ref,
                "Proposed removal: the typed host protocol has no equality, "
                "hash, id, or description. Replacement: compare `ContentCodec.type`.",
                open=True,
            )
        if member:
            return codec_member(sdk, name, ref, "")
        return decision(
            "static runtime",
            "ContentCodec",
            ref,
            "Typed host codec: Swift `associatedtype Value`, Kotlin `ContentCodec<T>`.",
        )
    if sdk in {"Swift", "Kotlin"} and name == "SignatureRequest.ffiSignatureRequest":
        return decision(
            "approved removal",
            "—",
            f"11.4 {sdk}, Messages, codecs, preferences, values",
            "The live request hides its FFI handle.",
        )
    if sdk in {"Swift", "Kotlin"} and name in {
        "SignatureRequest.addScwSignature",
        "SignatureRequest.addEcdsaSignature",
    }:
        return decision(
            "generated",
            "SignatureRequest.addSignature",
            f"11.4 {sdk}, Messages, codecs, preferences, values",
        )
    if sdk in {"Swift", "Kotlin"} and name in {
        "ConsentRecord.value",
        "ConsentRecord.entryType",
        "ConsentRecord.consentType",
    }:
        target = {
            "ConsentRecord.value": "ConsentRecord.entity.value",
            "ConsentRecord.entryType": "ConsentRecord.entity.kind",
            "ConsentRecord.consentType": "ConsentRecord.state",
        }[name]
        return decision(
            "generated",
            target,
            f"11.4 {sdk}, Messages, codecs, preferences, values",
            "ConsentRecord uses an entity and a state.",
        )
    if (
        sdk in {"Swift", "Kotlin"}
        and name.startswith("ConsentRecord.")
        and (name.endswith(".key") or ".Companion" in name)
    ):
        return Decision("generated", name, "open", "Not covered by the design.", True)
    if sdk in {"Swift", "Kotlin"} and (
        name == "DeleteMessageRequest" or name.startswith("DeleteMessageRequest.")
    ):
        return decision(
            "generated",
            name.replace("DeleteMessageRequest", "DeleteMessageContent", 1),
            f"11.4 {sdk}, Messages, codecs, preferences, values; 4, Host codecs",
            "The typed delete codec uses DeleteMessageContent.",
        )
    if sdk in {"Swift", "Kotlin"} and (name == "Reply" or name.startswith("Reply.")):
        member = name.split(".", 1)[1] if "." in name else ""
        if member in {"contentType", "init", "Companion", "Companion.create"}:
            return decision(
                "approved removal",
                "—",
                f"11.4 {sdk}, Messages, codecs, preferences, values; 11.2, MessageContent.reply",
                "The old Any/contentType constructor and FFI factory are replaced by ReplyContent with an EncodedContent body.",
            )
        targets = {
            "": "ReplyContent",
            "reference": "ReplyContent.reference",
            "referenceId": "ReplyContent.reference",
            "content": "ReplyContent.content",
            "inReplyTo": "Message.inReplyTo",
        }
        if member in targets:
            return decision(
                "generated",
                targets[member],
                f"11.4 {sdk}, Messages, codecs, preferences, values",
            )
        return Decision(
            "generated", spelling(name), "open", "Not covered by the design.", True
        )
    if sdk == "Kotlin" and name == "EncodedContent.compress":
        return decision(
            "approved removal",
            "—",
            "11.4 Kotlin, Messages, codecs, preferences, values",
            "Compression moves to Rust.",
        )
    if sdk == "Kotlin" and re.search(r"\.(?:toFfi\w*|fromFfi\w*)$", name):
        return decision(
            "approved removal",
            "—",
            "11.4 Kotlin, Messages, codecs, preferences, values",
            "FFI converter is internal plumbing.",
        )
    if sdk == "Swift" and name == "RemoteAttachment.content":
        return decision(
            "platform helper",
            "RemoteAttachmentDownload.content",
            "2, RemoteAttachmentDownload.swift",
        )
    if sdk == "Kotlin" and name == "RemoteAttachment.load":
        return decision(
            "platform helper",
            "RemoteAttachmentDownload.load",
            "2, RemoteAttachmentDownload.kt",
        )
    if sdk == "Kotlin" and (name == "Fetcher" or name.startswith("Fetcher.")):
        return decision(
            "platform helper",
            name.replace("Fetcher", "RemoteAttachmentDownload.Fetcher", 1),
            "2, RemoteAttachmentDownload.kt",
        )
    if sdk == "Kotlin" and name == "RemoteAttachment.fetcher":
        return Decision(
            "platform helper",
            "RemoteAttachmentDownload.fetcher",
            "open",
            "The design lists the download file but does not name this injected fetcher.",
            True,
        )
    if (
        sdk in {"Swift", "Kotlin"}
        and name.startswith("ContentType")
        and name not in {"ContentTypeIdBuilder", "ContentTypeID", "ContentTypeId"}
        and "." not in name
    ):
        return decision(
            "static runtime",
            spelling(name),
            f"11.4 {sdk}, Messages, codecs, preferences, values; 4",
            "Standard content type constant.",
        )
    if sdk in {"Swift", "Kotlin"} and name.startswith(
        ("let ContentType", "val ContentType")
    ):
        return decision(
            "static runtime",
            spelling(name),
            f"11.4 {sdk}, Messages, codecs, preferences, values; 4",
            "Standard content type constant.",
        )
    covered = covered_open_export(sdk, name)
    if covered is not None:
        return covered
    root = name.split(".", 1)[0]
    if root in RECORD_ROOTS:
        target = name
        if name.endswith("AtNs"):
            target = name[:-2] + ".ns"
        elif name.endswith("At"):
            target += ".date"
        return decision(
            "generated",
            spelling(target),
            f"11.4 {sdk}, Messages, codecs, preferences, values; 11.1-11.2",
            "Generated value or record field.",
        )
    if sdk in {"Swift", "Kotlin"} and name.startswith("CodecRegistry"):
        return decision(
            "static runtime", spelling(name), "4, per-client codec registry"
        )
    if sdk == "Kotlin" and name.startswith("DecodedMessage."):
        nested = name.removeprefix("DecodedMessage.")
        for old, target in {
            "MessageDeliveryStatus": "DeliveryStatus",
            "SortDirection": "ListMessagesOptions.direction",
            "SortBy": "ListMessagesOptions.sortBy",
        }.items():
            if nested == old or nested.startswith(old + "."):
                leaf = nested[len(old) :].lstrip(".")
                if old == "MessageDeliveryStatus" and leaf == "ALL":
                    return decision(
                        "approved removal",
                        "—",
                        "11.2, ListMessagesOptions",
                        "No filter selects every delivery status.",
                    )
                value = {"SENT_TIME": "sentAt", "INSERTED_TIME": "insertedAt"}.get(
                    leaf, enum_value(leaf)
                )
                suffix = "." + value if leaf else ""
                return decision(
                    "generated",
                    target + suffix,
                    "11.2, MessageData and ListMessagesOptions",
                )
    if name.startswith("DecodedMessageV2.") or name == "DecodedMessageV2":
        if name == "DecodedMessageV2":
            return decision(
                "approved removal",
                "—",
                f"11.4 {sdk}, Messages, codecs, preferences, values; 19, decision 5",
                "The V2 type leaves the API; its value fields move to Message.",
            )
        if name.endswith((".create", ".init", ".Companion.create", ".Companion")):
            return decision(
                "approved removal",
                "—",
                f"11.4 {sdk}, Messages, codecs, preferences, values",
                "Factories become internal.",
            )
        field = name.rsplit(".", 1)[-1]
        field = {
            "contentTypeId": "contentType",
            "fallbackText": "fallback",
            "body": "content",
        }.get(field, field)
        if field in {"hasReactions", "reactionCount"}:
            return decision(
                "static runtime",
                "Message.reactions",
                "11.2, MessageData; 11.7",
                "The host derives this value from Message.reactions; it is not a Message field.",
            )
        if field.endswith("Ns"):
            field = field[:-2] + ".ns"
        elif field in {"sentAt", "insertedAt", "expiresAt"}:
            field += ".date"
        return decision(
            "static runtime",
            spelling("Message" + ("." + field if name != "DecodedMessageV2" else "")),
            f"11.4 {sdk}, Messages, codecs, preferences, values; 11.7",
            "The received type is optional. It is absent when the type field cannot be read. Original serialization is available as `Message.rawBytes`."
            if field == "contentType"
            else "",
        )
    if name.startswith("DecodedMessage.") or name == "DecodedMessage":
        if name == "DecodedMessage":
            return decision(
                "generated",
                "Message",
                f"11.4 {sdk}, Messages, codecs, preferences, values; 11.5",
                "The Message host class replaces this type.",
            )
        if name.endswith(
            (".create", ".Companion.create", ".init", ".Companion", ".constructor")
        ):
            return decision(
                "approved removal",
                "—",
                f"11.4 {sdk}, Messages, codecs, preferences, values",
                "Factories become internal.",
            )
        field = name.rsplit(".", 1)[-1]
        if field == "childMessages":
            return decision(
                "approved removal",
                "—",
                f"11.4 {sdk}, Messages, codecs, preferences, values",
            )
        if field == "body":
            field = "content"
        if field.endswith("Ns"):
            field = field[:-2] + ".ns"
        elif field in {"sentAt", "insertedAt", "expiresAt"}:
            field += ".date"
        return decision(
            "static runtime",
            spelling("Message." + field),
            f"11.4 {sdk}, Messages, codecs, preferences, values; 11.7",
        )
    if name == "DeliveryCursor" or name.startswith("DeliveryCursor."):
        return decision(
            "generated",
            spelling(name),
            "11.2, readers",
            "Cursor is a generated record.",
        )
    if name == "CatchUpSummary" or name.startswith("CatchUpSummary."):
        return decision(
            "generated",
            spelling(name),
            "11.2, readers",
            "Catch-up summary is a generated record.",
        )
    if name == "MessageReader":
        return decision(
            "generated",
            name,
            "11.2, readers",
            "The reader object is generated; stream is a host adapter.",
        )
    if sdk == "Kotlin" and re.search(
        r"\.(?:toFfi\w*|fromFfi\w*|encodedContentFromFfi|toHex|hexToByteArray)$", name
    ):
        if name in {
            "ByteArray.toHex",
            "String.hexToByteArray",
            "func encodedContentFromFfi",
        }:
            return Decision(
                "static runtime",
                spelling(name),
                "open",
                "Not covered by the design.",
                True,
            )
        return decision(
            "approved removal",
            "—",
            "11.4 Kotlin, Messages, codecs, preferences, values",
            "FFI converter is internal plumbing.",
        )
    if (
        name.endswith(".compress")
        and sdk == "Kotlin"
        and name.startswith("EncodedContent")
    ):
        return decision(
            "approved removal",
            "—",
            "11.4 Kotlin, Messages, codecs, preferences, values",
            "Compression moves to Rust.",
        )
    if (
        name.startswith("UnstableGroup")
        or name.endswith(".unstable")
        or name.endswith(".proposalsEnabled")
    ):
        return decision(
            "approved removal",
            "—",
            f"11.4 {sdk}, Conversation, Group, Dm",
            "Proposals API is removed before the facade.",
        )
    if name in {
        "Client.manageStreamLifecycle",
        "Client.Companion.manageStreamLifecycle",
    }:
        return decision(
            "platform helper", "Client.manageStreamLifecycle", "2, platform files"
        )
    if sdk == "Swift" and source.endswith(
        ("StreamLifecycle.swift", "XMTPLogger.swift", "Extensions/URL.swift")
    ):
        return decision("platform helper", spelling(name), "2, platform files")
    if sdk == "Kotlin" and source.endswith("StreamLifecycle.kt"):
        return decision("platform helper", spelling(name), "2, platform files")
    if sdk == "Kotlin" and (name == "HTTPFetcher" or name.startswith("HTTPFetcher.")):
        return decision(
            "platform helper",
            spelling(name.replace("HTTPFetcher", "RemoteAttachmentDownload", 1)),
            "2, RemoteAttachmentDownload.kt",
        )
    if sdk == "Swift" and name in {
        "RemoteAttachment.content",
        "RemoteAttachmentCodec.content",
    }:
        return decision(
            "platform helper",
            "RemoteAttachmentDownload.content",
            "2, RemoteAttachmentDownload.swift",
        )
    if (
        sdk == "Kotlin"
        and name.startswith("RemoteAttachment.")
        and name.endswith("load")
    ):
        return decision(
            "platform helper",
            "RemoteAttachmentDownload.load",
            "2, RemoteAttachmentDownload.kt",
        )
    if name.startswith("func ") and name.split()[-1] in {
        "getXMTPLogFilePaths",
        "clearXMTPLogs",
    }:
        return decision("platform helper", name, "2, Logging.kt")
    if sdk in {"Node", "Browser"} and kind == "binding re-export":
        ref = (
            "11.4 Node, Messages, codecs, preferences, values"
            if sdk == "Node"
            else "11.4 Browser; 11.4 Node, Messages, codecs, preferences, values"
        )
        return decision(
            "generated",
            spelling(name),
            ref,
            "The facade package root exports this name.",
        )
    if (name.endswith(".sentAt") or name.endswith(".sentAtNs")) and name.split(".", 1)[
        0
    ] in {
        "Message",
        "DecodedMessage",
        "DecodedMessageV2",
        "ReplyParent",
        "MessageMetadata",
    }:
        return decision(
            "generated",
            spelling(name[:-2] + ".ns" if name.endswith("Ns") else name + ".date"),
            "11.2, Timestamp; plan Decisions",
        )
    if (
        name == "Conversations.streamAllMessages"
        and sdk in {"Swift", "Kotlin", "Node", "Browser"}
    ) or (
        name
        in {
            "Conversations.streamAllGroupMessages",
            "Conversations.streamAllDmMessages",
        }
        and sdk in {"Node", "Browser"}
    ):
        ref = (
            f"11.4 {sdk}, Conversations"
            if sdk != "Browser"
            else "11.4 Browser; 11.4 Node, Conversations"
        )
        return decision(
            "generated",
            spelling(name),
            ref,
            "Behavior change: no consent list now selects allowed and unknown conversations.",
        )
    # These are explicit unchanged lists in the four Section 11.4 tables.
    owner, _, leaf = name.rpartition(".")
    if owner in {"Client", "Client.Companion"} and leaf in {
        "enableNotifications",
        "disableNotifications",
        "notificationState",
        "revokeInstallations",
        "revokeAllOtherInstallations",
        "removeAccount",
        "canMessage",
        "inboxState",
        "catchUpToLive",
        "syncAllDeviceSyncGroups",
        "serverConfiguration",
        "refreshServerConfiguration",
        "fetchServerConfiguration",
        "signWithInstallationKey",
        "verifySignedWithInstallationKey",
        "verifySignedWithPublicKey",
        "register",
        "changeRecoveryIdentifier",
        "isAddressAuthorized",
        "isInstallationAuthorized",
        "inboxID",
        "installationID",
        "inboxId",
        "installationId",
        "preferences",
        "conversations",
        "setNotifications",
    }:
        return decision(
            "generated",
            spelling(name),
            f"11.4 {sdk}, Client and options"
            if sdk != "Browser"
            else "11.4 Browser; 11.4 Node, Client and options",
            "Unchanged member or spelling rule.",
        )
    if owner == "Conversations" and leaf in {
        "sync",
        "syncAll",
        "list",
        "listGroups",
        "listDms",
        "stream",
        "streamGroups",
        "streamDms",
        "streamAllMessages",
        "streamAllGroupMessages",
        "streamAllDmMessages",
        "messageReader",
        "messageHistorySnapshot",
        "beginningDeliveryCursor",
        "deleteMessageLocally",
        "hmacKeys",
        "newGroupOptimistic",
    }:
        return decision(
            "generated",
            spelling(name),
            f"11.4 {sdk}, Conversations"
            if sdk != "Browser"
            else "11.4 Browser; 11.4 Node, Conversations",
            "Unchanged member or spelling rule.",
        )
    if owner in {"Conversation", "Group", "Dm"} and leaf in {
        "id",
        "topic",
        "sync",
        "send",
        "sendText",
        "sendMarkdown",
        "sendReaction",
        "sendReadReceipt",
        "sendReply",
        "sendTransactionReference",
        "sendWalletSendCalls",
        "sendActions",
        "sendIntent",
        "sendAttachment",
        "sendMultiRemoteAttachment",
        "sendRemoteAttachment",
        "prepareMessage",
        "publishMessages",
        "publishMessage",
        "deleteMessage",
        "stream",
        "messageReader",
        "messageHistorySnapshot",
        "beginningDeliveryCursor",
        "lastMessage",
        "members",
        "countMessages",
        "messages",
        "setNotifications",
        "updateConsentState",
        "processStreamedMessage",
        "lastReadTimes",
        "debugInfo",
        "isCreator",
        "membershipCapabilities",
        "addMembers",
        "removeMembers",
        "addAdmin",
        "removeAdmin",
        "addSuperAdmin",
        "removeSuperAdmin",
        "isAdmin",
        "isSuperAdmin",
        "listAdmins",
        "listSuperAdmins",
        "updateName",
        "updateImageUrl",
        "updateDescription",
        "updateAppData",
        "updatePermission",
        "requestRemoval",
        "duplicateDms",
    }:
        return decision(
            "generated",
            spelling(name),
            f"11.4 {sdk}, Conversation, Group, Dm"
            if sdk != "Browser"
            else "11.4 Browser; 11.4 Node, Conversation, Group, Dm",
            "Unchanged member or spelling rule.",
        )
    if sdk in {"Node", "Browser"} and name in {
        "MessageStream",
        "MessageStream[Symbol.asyncIterator]",
    }:
        return decision(
            "static runtime", name, "5, durable streams; 11.4 Node, unchanged list"
        )
    if sdk in {"Node", "Browser"} and (
        name == "CodecRegistry" or name.startswith("CodecRegistry.")
    ):
        return decision(
            "static runtime", name, "4, custom codecs; 11.4 Node, unchanged"
        )
    if name in {
        "ClientOptions",
        "Signer",
        "SigningKey",
        "SignedData",
        "SignerType",
        "PublicIdentity",
        "IdentityKind",
        "ConsentRecord",
        "InboxState",
        "Installation",
        "Member",
        "PermissionPolicySet",
        "GroupMembershipResult",
        "GroupMembershipCapabilities",
        "DisappearingMessageSettings",
        "ArchiveOptions",
        "ArchiveElement",
        "ArchiveMetadata",
        "ConversationDebugInfo",
        "ApiStats",
        "IdentityStats",
        "EncodedContent",
        "ContentTypeID",
        "ContentTypeId",
        "GroupUpdated",
    }:
        target = {
            "SigningKey": "Signer",
            "SignedData": "Signature",
            "SignerType": "SignerKind",
        }.get(name, name)
        return decision(
            "generated",
            spelling(target),
            f"11.4 {sdk}, Messages, codecs, preferences, values; 11.1-11.2",
        )
    if name == "ReactionCodec" or name.startswith("ReactionCodec."):
        # CTYPE section 6 puts legacy reaction v1 outside the catalogue and
        # publishes only v2, so the new SDK ships no v1 codec.
        return Decision(
            "proposed removal",
            "—",
            f"11.4 {sdk}, Messages, codecs, preferences, values; 4",
            "Proposed removal: the SDK has no legacy reaction v1 codec (CTYPE section 6). "
            "Replacement: `ReactionV2Codec`.",
            open=True,
        )
    if name.endswith("Codec") or (
        "Codec." in name and name.split(".")[0].endswith("Codec")
    ):
        return codec_member(
            sdk,
            name,
            f"11.4 {sdk}, Messages, codecs, preferences, values; 4",
            "Standard codec host class or codec protocol.",
        )
    if name == "PrivatePreferences" or name.startswith("PrivatePreferences."):
        if name == "PrivatePreferences.client":
            return decision(
                "approved removal",
                "—",
                "11.2, Preferences",
                "The wrapper's client field is internal to the facade.",
            )
        target = name.replace("PrivatePreferences", "Preferences", 1)
        target = (
            target.replace("setConsentState", "setConsentStates")
            .replace("conversationState", "consentState")
            .replace("inboxIdState", "consentState")
        )
        if name.endswith(".syncConsent"):
            return decision(
                "approved removal",
                "—",
                "11.4 Kotlin, Messages, codecs, preferences, values",
                "Use Preferences.sync().",
            )
        if name.endswith((".streamConsent", ".streamPreferenceUpdates")):
            return decision("approved removal", "—", "11.8, live events")
        return decision(
            "generated",
            spelling(target),
            f"11.4 {sdk}, Messages, codecs, preferences, values",
        )
    if name.endswith(".waitForRegistrationVisible") and owner in {
        "Client",
        "ClientOptions",
        "Client.Companion",
    }:
        return Decision(
            "generated",
            spelling(name),
            "plan Decisions; open",
            "Standalone method is kept by the plan; its facade owner is not covered in design 11.4.",
            True,
        )
    proposed = (
        "static runtime"
        if kind in {"func", "function", "const"} and owner == ""
        else "generated"
    )
    return Decision(
        proposed, spelling(name), "open", "Not covered by the design.", True
    )


# Client members that the new SDK has only as static members, and members that
# it has in both placements. Every other Client destination is an instance
# member. The public-member check reads the placement from the final name.
CLIENT_STATIC_ONLY = {
    "create",
    "build",
    "fetchServerConfiguration",
    "newestMessageMetadata",
    "isAddressAuthorized",
    "isInstallationAuthorized",
    "verifySignedWithPublicKey",
}
CLIENT_BOTH = {
    "canMessage",
    "inboxId(for:)",
    "inboxStates",
    "keyPackageStatuses",
    "revokeInstallations",
}


def client_placement(final: str, current_static: bool) -> tuple[str, str]:
    """Return a Client destination with its placement and a note for a move.

    A static destination has the `static` prefix. A member that changes
    placement gets a note, so the migration guide can list it.
    """
    if not final.startswith("Client."):
        return final, ""
    member = final.removeprefix("Client.").split(".")[0]
    key = member if member.endswith("(for:)") else member.split("(")[0]
    static = key in CLIENT_STATIC_ONLY or (key in CLIENT_BOTH and current_static)
    if static and not current_static:
        note = "Moves from an instance member to a static member."
    elif current_static and not static:
        note = "Moves from a static member to an instance member."
    else:
        note = ""
    return (f"static {final}" if static else final), note


def classify(entry: object) -> Decision:
    result = _classify(entry)
    result = Decision(
        result.status,
        stock_final(result.final),
        result.ref,
        result.note,
        result.open,
    )
    if entry.sdk == "Kotlin" and result.final.startswith("Client.Companion."):
        result = Decision(
            result.status,
            result.final.replace("Client.Companion.", "Client.", 1),
            result.ref,
            result.note,
            result.open,
        )
    if entry.sdk == "Browser" and result.ref.startswith("11.4 Browser,"):
        result = Decision(
            result.status,
            result.final,
            result.ref.replace("11.4 Browser,", "11.4 Browser; 11.4 Node,", 1),
            result.note,
            result.open,
        )
    return result
