import uniffi.xmtp_sdk.*

// This list comes from the retained Client surface in the approved plan, not
// from the generator. Removing a public forwarder must fail this compile.
suspend fun consumeClient(
    signer: Signer,
    options: ClientOptions,
    identity: PublicIdentity,
    credential: Credential,
    request: SignatureRequest,
    filter: EventFilter,
    config: NotificationConfig,
) {
    val client = SDKClient.create(signer, options)
    val built = SDKClient.build(identity, options, client.inboxId())
    val inboxId: InboxId = client.inboxId()
    val installationId: InstallationId = client.installationId()
    val installationBytes: ByteArray = client.installationIdBytes()
    val clientIdentity: PublicIdentity = client.identity()
    val inMemory: Boolean = client.isInMemory()
    val path: String? = client.storagePath()
    val version: String = client.libxmtpVersion()
    val appVersion: String? = client.appVersion()
    val clientOptions: ClientOptions = client.options()
    val conversations: Conversations = client.conversations()
    val preferences: Preferences = client.preferences()
    val diagnostics: Diagnostics = client.diagnostics()
    val archives: Archives = client.archives()
    val storage: Storage = client.storage()
    client.register()
    val registered: Boolean = client.isRegistered()
    val state: InboxState = client.inboxState(true)
    val states: List<InboxState> = client.inboxStates(listOf(inboxId), false)
    val found: InboxId? = client.inboxIdFor(identity)
    val reachable: Map<String, Boolean> = client.canMessage(listOf(identity))
    val latest: Map<String, ULong> = client.latestInboxUpdatesCount(listOf(inboxId), false)
    val own: ULong = client.ownInboxUpdatesCount(false)
    val packages: Map<String, KeyPackageStatus> = client.keyPackageStatuses(listOf(installationId))
    val configuration: ServerConfiguration = client.serverConfiguration()
    val refreshed: ServerConfiguration = client.refreshServerConfiguration()
    client.setCredential(credential)
    val signature: ByteArray = client.signWithInstallationKey("text")
    val verified: Boolean = client.verifySignedWithInstallationKey("text", signature)
    val caughtUp: CatchUpSummary = client.catchUpToLive(null)
    val synced: GroupSyncSummary = client.syncAllDeviceSyncGroups()
    val decoded: MessageContent = client.decodeContent(encodeText("text"))
    val createInbox: SignatureRequest? = client.unsafeCreateInboxSignatureRequest()
    val addAccount: SignatureRequest = client.unsafeAddAccountSignatureRequest(identity, false)
    val removeAccount: SignatureRequest = client.unsafeRemoveAccountSignatureRequest(identity)
    val revoke: SignatureRequest = client.unsafeRevokeInstallationsSignatureRequest(emptyList())
    val revokeOthers: SignatureRequest? = client.unsafeRevokeAllOtherInstallationsSignatureRequest()
    val recovery: SignatureRequest = client.unsafeChangeRecoveryIdentifierSignatureRequest(identity)
    client.unsafeApplySignatureRequest(request)
    client.unsafeAddAccount(signer, false)
    client.removeAccount(signer, identity)
    client.revokeInstallations(signer, emptyList())
    client.revokeAllOtherInstallations(signer)
    client.changeRecoveryIdentifier(signer, identity)
    val enabled: NotificationState = client.enableNotifications(config)
    val notificationState: NotificationState = client.notificationState()
    client.disableNotifications()
    val events = client.events(filter)
    val listener = client.startListener(filter) { }
    client.stopListener(listener)
    val stream = client.conversationStream()
    println(
        listOf(
            installationBytes,
            clientIdentity,
            inMemory,
            path,
            version,
            appVersion,
            clientOptions,
            conversations,
            preferences,
            diagnostics,
            archives,
            storage,
            registered,
            state,
            states,
            found,
            reachable,
            latest,
            own,
            packages,
            configuration,
            refreshed,
            verified,
            caughtUp,
            synced,
            decoded,
            createInbox,
            addAccount,
            removeAccount,
            revoke,
            revokeOthers,
            recovery,
            enabled,
            notificationState,
            events,
            stream,
        ),
    )
    built.end()
    client.end()
}

suspend fun consumeStaticHelpers(
    signer: Signer,
    identity: PublicIdentity,
    backend: BackendSource,
) {
    val configuration: ServerConfiguration = SDKClient.fetchServerConfiguration(backend)
    val reachable: Map<String, Boolean> = SDKClient.canMessage(listOf(identity), backend)
    val inbox: InboxId = SDKClient.inboxIdFor(identity, backend)
    val states: List<InboxState> = SDKClient.inboxStates(listOf(inbox), backend)
    val packages: Map<String, KeyPackageStatus> = SDKClient.keyPackageStatuses(emptyList(), backend)
    val newest: Map<String, MessageMetadataEntry> = SDKClient.newestMessageMetadata(emptyList(), backend)
    SDKClient.revokeInstallations(signer, inbox, emptyList(), backend)
    val address: Boolean = SDKClient.isAddressAuthorized("address", inbox, backend)
    val installation: Boolean = SDKClient.isInstallationAuthorized("installation", inbox, backend)
    val verified: Boolean = SDKClient.verifySignedWithPublicKey("text", ByteArray(0), ByteArray(0))
    println(listOf(configuration, reachable, states, packages, newest, address, installation, verified))
}
