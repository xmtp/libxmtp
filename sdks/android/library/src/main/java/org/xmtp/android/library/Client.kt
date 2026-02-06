package org.xmtp.android.library

import android.content.Context
import android.util.Log
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import org.xmtp.android.library.codecs.ContentCodec
import org.xmtp.android.library.codecs.TextCodec
import org.xmtp.android.library.libxmtp.ArchiveMetadata
import org.xmtp.android.library.libxmtp.ArchiveOptions
import org.xmtp.android.library.libxmtp.IdentityKind
import org.xmtp.android.library.libxmtp.InboxState
import org.xmtp.android.library.libxmtp.PublicIdentity
import org.xmtp.android.library.libxmtp.SignatureRequest
import org.xmtp.android.library.libxmtp.toFfi
import uniffi.xmtpv3.DbOptions
import uniffi.xmtpv3.FfiClientMode
import uniffi.xmtpv3.FfiForkRecoveryOpts
import uniffi.xmtpv3.FfiForkRecoveryPolicy
import uniffi.xmtpv3.FfiKeyPackageStatus
import uniffi.xmtpv3.FfiLogLevel
import uniffi.xmtpv3.FfiLogRotation
import uniffi.xmtpv3.FfiMessageMetadata
import uniffi.xmtpv3.FfiProcessType
import uniffi.xmtpv3.FfiSyncWorkerMode
import uniffi.xmtpv3.FfiXmtpClient
import uniffi.xmtpv3.XmtpApiClient
import uniffi.xmtpv3.applySignatureRequest
import uniffi.xmtpv3.connectToBackend
import uniffi.xmtpv3.createClient
import uniffi.xmtpv3.enterDebugWriter
import uniffi.xmtpv3.exitDebugWriter
import uniffi.xmtpv3.generateInboxId
import uniffi.xmtpv3.getInboxIdForIdentifier
import uniffi.xmtpv3.getNewestMessageMetadata
import uniffi.xmtpv3.getVersionInfo
import uniffi.xmtpv3.inboxStateFromInboxIds
import uniffi.xmtpv3.isConnected
import uniffi.xmtpv3.revokeInstallations
import java.io.File

typealias PreEventCallback = suspend () -> Unit
typealias ProcessType = FfiProcessType
typealias MessageMetadata = FfiMessageMetadata

data class ClientOptions(
    val api: Api = Api(),
    val preAuthenticateToInboxCallback: PreEventCallback? = null,
    val appContext: Context,
    val dbEncryptionKey: ByteArray,
    val historySyncUrl: String? = api.env.getHistorySyncUrl(),
    val dbDirectory: String? = null,
    val deviceSyncEnabled: Boolean = true,
    val forkRecoveryOptions: ForkRecoveryOptions? = null,
) {
    data class Api(
        val env: XMTPEnvironment = XMTPEnvironment.DEV,
        val isSecure: Boolean = true,
        val appVersion: String? = null,
        val gatewayHost: String? = null,
    )
}

enum class ForkRecoveryPolicy {
    None,
    AllowlistedGroups,
    All,
    ;

    fun toFfi(): FfiForkRecoveryPolicy =
        when (this) {
            None -> FfiForkRecoveryPolicy.NONE
            AllowlistedGroups -> FfiForkRecoveryPolicy.ALLOWLISTED_GROUPS
            All -> FfiForkRecoveryPolicy.ALL
        }
}

data class ForkRecoveryOptions(
    val enableRecoveryRequests: ForkRecoveryPolicy,
    val groupsToRequestRecovery: List<String>,
    val disableRecoveryResponses: Boolean? = null,
    val workerIntervalNs: ULong? = null,
) {
    fun toFfi(): FfiForkRecoveryOpts =
        FfiForkRecoveryOpts(
            enableRecoveryRequests = this.enableRecoveryRequests.toFfi(),
            groupsToRequestRecovery = this.groupsToRequestRecovery,
            disableRecoveryResponses = this.disableRecoveryResponses,
            workerIntervalNs = this.workerIntervalNs,
        )
}

typealias InboxId = String

class Client(
    libXMTPClient: FfiXmtpClient,
    val dbPath: String,
    val installationId: String,
    val inboxId: InboxId,
    val environment: XMTPEnvironment,
    val publicIdentity: PublicIdentity,
) {
    val preferences: PrivatePreferences =
        PrivatePreferences(client = this, ffiClient = libXMTPClient)
    val conversations: Conversations =
        Conversations(
            client = this,
            ffiConversations = libXMTPClient.conversations(),
            ffiClient = libXMTPClient,
        )
    val debugInformation: XMTPDebugInformation =
        XMTPDebugInformation(ffiClient = libXMTPClient)
    val libXMTPVersion: String = getVersionInfo()
    private val ffiClient: FfiXmtpClient = libXMTPClient

    companion object {
        private const val TAG = "Client"

        var codecRegistry =
            run {
                val registry = CodecRegistry()
                registry.register(codec = TextCodec())
                registry
            }

        private fun ClientOptions.Api.toCacheKey(): String =
            "${env.getUrl()}|$isSecure|${appVersion ?: "nil"}|${gatewayHost ?: "nil"}"

        private val apiClientCache = mutableMapOf<String, XmtpApiClient>()
        private val cacheLock = Mutex()

        private val syncApiClientCache = mutableMapOf<String, XmtpApiClient>()
        private val syncCacheLock = Mutex()

        fun activatePersistentLibXMTPLogWriter(
            appContext: Context,
            logLevel: FfiLogLevel,
            rotationSchedule: FfiLogRotation,
            maxFiles: Int,
            processType: ProcessType = FfiProcessType.MAIN,
        ) {
            val logDirectory = File(appContext.filesDir, "xmtp_logs")
            if (!logDirectory.exists()) {
                logDirectory.mkdirs()
            }
            enterDebugWriter(
                logDirectory.toString(),
                logLevel,
                rotationSchedule,
                maxFiles.toUInt(),
                processType,
            )
        }

        fun deactivatePersistentLibXMTPLogWriter() {
            exitDebugWriter()
        }

        fun getXMTPLogFilePaths(appContext: Context): List<String> {
            val logDirectory = File(appContext.filesDir, "xmtp_logs")
            if (!logDirectory.exists()) {
                return emptyList()
            }

            return logDirectory
                .listFiles()
                ?.filter { it.isFile }
                ?.map { it.absolutePath }
                ?: emptyList()
        }

        fun clearXMTPLogs(appContext: Context): Int {
            val logDirectory = File(appContext.filesDir, "xmtp_logs")
            if (!logDirectory.exists()) {
                return 0
            }

            try {
                deactivatePersistentLibXMTPLogWriter()
            } catch (e: Exception) {
                // Log writer might not be active, continue with deletion
            }

            var deletedCount = 0
            logDirectory.listFiles()?.forEach { file ->
                if (file.isFile && file.delete()) {
                    deletedCount++
                }
            }

            return deletedCount
        }

        suspend fun connectToApiBackend(api: ClientOptions.Api): XmtpApiClient {
            val cacheKey = api.toCacheKey()
            return cacheLock.withLock {
                val cached = apiClientCache[cacheKey]

                if (cached != null && isConnected(cached)) {
                    return cached
                }

                // If not cached or not connected, create a fresh client
                val newClient =
                    connectToBackend(
                        api.env.getUrl(),
                        api.gatewayHost,
                        api.isSecure,
                        FfiClientMode.DEFAULT,
                        api.appVersion,
                        null,
                        null,
                    )
                apiClientCache[cacheKey] = newClient
                return@withLock newClient
            }
        }

        suspend fun connectToSyncApiBackend(api: ClientOptions.Api): XmtpApiClient {
            val cacheKey = api.toCacheKey()
            return syncCacheLock.withLock {
                val cached = syncApiClientCache[cacheKey]

                if (cached != null && isConnected(cached)) {
                    return cached
                }

                // If not cached or not connected, create a fresh client
                val newClient =
                    connectToBackend(
                        api.env.getUrl(),
                        api.gatewayHost,
                        api.isSecure,
                        FfiClientMode.DEFAULT,
                        api.appVersion,
                        null,
                        null,
                    )
                syncApiClientCache[cacheKey] = newClient
                return@withLock newClient
            }
        }

        suspend fun getOrCreateInboxId(
            api: ClientOptions.Api,
            publicIdentity: PublicIdentity,
        ): InboxId =
            withContext(Dispatchers.IO) {
                val rootIdentity = publicIdentity.ffiPrivate
                var inboxId =
                    getInboxIdForIdentifier(
                        api = connectToApiBackend(api),
                        accountIdentifier = rootIdentity,
                    )
                if (inboxId.isNullOrBlank()) {
                    inboxId = generateInboxId(rootIdentity, 0.toULong())
                }
                inboxId
            }

        suspend fun revokeInstallations(
            api: ClientOptions.Api,
            signingKey: SigningKey,
            inboxId: InboxId,
            installationIds: List<String>,
        ) = withContext(Dispatchers.IO) {
            val apiClient = connectToApiBackend(api)
            val rootIdentity = signingKey.publicIdentity.ffiPrivate
            val ids = installationIds.map { it.hexToByteArray() }
            val signatureRequest = revokeInstallations(apiClient, rootIdentity, inboxId, ids)
            handleSignature(SignatureRequest(signatureRequest), signingKey)
            applySignatureRequest(apiClient, signatureRequest)
        }

        @DelicateApi(
            "This function is delicate and should be used with caution. Should only be used if trying to manage the signature flow independently otherwise use `revokeInstallations()` instead",
        )
        suspend fun ffiRevokeInstallations(
            api: ClientOptions.Api,
            publicIdentity: PublicIdentity,
            inboxId: InboxId,
            installationIds: List<String>,
        ): SignatureRequest =
            withContext(Dispatchers.IO) {
                val apiClient = connectToApiBackend(api)
                val rootIdentity = publicIdentity.ffiPrivate
                val ids = installationIds.map { it.hexToByteArray() }
                val signatureRequest = revokeInstallations(apiClient, rootIdentity, inboxId, ids)
                SignatureRequest(signatureRequest)
            }

        @DelicateApi(
            "This function is delicate and should be used with caution. Should only be used if trying to manage the signature flow independently otherwise use `revokeInstallations()` instead",
        )
        suspend fun ffiApplySignatureRequest(
            api: ClientOptions.Api,
            signatureRequest: SignatureRequest,
        ) = withContext(Dispatchers.IO) {
            val apiClient = connectToApiBackend(api)
            applySignatureRequest(apiClient, signatureRequest.ffiSignatureRequest)
        }

        fun register(codec: ContentCodec<*>) {
            codecRegistry.register(codec = codec)
        }

        private suspend fun <T> withFfiClient(
            api: ClientOptions.Api,
            useClient: suspend (ffiClient: FfiXmtpClient) -> T,
        ): T =
            withContext(Dispatchers.IO) {
                val publicIdentity =
                    PublicIdentity(
                        IdentityKind.ETHEREUM,
                        "0x0000000000000000000000000000000000000000",
                    )
                val inboxId = getOrCreateInboxId(api, publicIdentity)

                val ffiClient =
                    createClient(
                        api = connectToApiBackend(api),
                        syncApi = connectToApiBackend(api),
                        db =
                            DbOptions(
                                db = null,
                                encryptionKey = null,
                                maxDbPoolSize = null,
                                minDbPoolSize = null,
                            ),
                        accountIdentifier = publicIdentity.ffiPrivate,
                        inboxId = inboxId,
                        nonce = 0.toULong(),
                        legacySignedPrivateKeyProto = null,
                        deviceSyncServerUrl = null,
                        deviceSyncMode = null,
                        allowOffline = false,
                        forkRecoveryOpts = null,
                    )

                useClient(ffiClient)
            }

        suspend fun inboxStatesForInboxIds(
            inboxIds: List<InboxId>,
            api: ClientOptions.Api,
        ): List<InboxState> =
            withContext(Dispatchers.IO) {
                val apiClient = connectToApiBackend(api)
                inboxStateFromInboxIds(apiClient, inboxIds).map { InboxState(it) }
            }

        suspend fun getNewestMessageMetadata(
            groupIds: List<String>,
            api: ClientOptions.Api,
        ): Map<String, MessageMetadata> =
            withContext(Dispatchers.IO) {
                val apiClient = connectToApiBackend(api)
                val groupIdBytes = groupIds.map { it.hexToByteArray() }
                val result = getNewestMessageMetadata(apiClient, groupIdBytes)
                result.entries.associate { (byteArrayKey, metadata) ->
                    byteArrayKey.toHex() to metadata
                }
            }

        suspend fun keyPackageStatusesForInstallationIds(
            installationIds: List<String>,
            api: ClientOptions.Api,
        ): Map<String, FfiKeyPackageStatus> =
            withContext(Dispatchers.IO) {
                withFfiClient(api) { ffiClient ->
                    val byteArrays = installationIds.map { it.hexToByteArray() }
                    val result = ffiClient.getKeyPackageStatusesForInstallationIds(byteArrays)
                    result.entries.associate { (byteArrayKey, status) ->
                        byteArrayKey.toHex() to status
                    }
                }
            }

        suspend fun canMessage(
            identities: List<PublicIdentity>,
            api: ClientOptions.Api,
        ): Map<String, Boolean> =
            withContext(Dispatchers.IO) {
                withFfiClient(api) { ffiClient ->
                    val ffiIdentifiers = identities.map { it.ffiPrivate }
                    val result = ffiClient.canMessage(ffiIdentifiers)

                    result.mapKeys { (ffiIdentifier, _) ->
                        ffiIdentifier.identifier
                    }
                }
            }

        private suspend fun initializeV3Client(
            publicIdentity: PublicIdentity,
            clientOptions: ClientOptions,
            signingKey: SigningKey? = null,
            inboxId: InboxId? = null,
            buildOffline: Boolean = false,
        ): Client =
            withContext(Dispatchers.IO) {
                val recoveredInboxId =
                    inboxId ?: getOrCreateInboxId(clientOptions.api, publicIdentity)

                val (ffiClient, dbPath) =
                    createFfiClient(
                        publicIdentity,
                        recoveredInboxId,
                        clientOptions,
                        clientOptions.appContext,
                        buildOffline,
                    )
                clientOptions.preAuthenticateToInboxCallback?.let {
                    runBlocking {
                        it.invoke()
                    }
                }
                ffiClient.signatureRequest()?.let { signatureRequest ->
                    signingKey?.let {
                        handleSignature(SignatureRequest(signatureRequest), it)
                    } ?: run {
                        Log.d("XMTP", "No signer provided. Logging DB context...")
                        Log.d("XMTP", "dbPath: $dbPath")

                        if (clientOptions.dbDirectory != null) {
                            Log.d("XMTP", "dbDirectory: ${clientOptions.dbDirectory}")

                            val dbDirFile = File(clientOptions.dbDirectory)
                            val fileCount = dbDirFile.listFiles()?.size ?: 0

                            Log.d("XMTP", "Files in dbDirectory: $fileCount")
                        }
                        throw XMTPException("No signer passed but signer was required.")
                    }

                    ffiClient.registerIdentity(signatureRequest)
                }

                Client(
                    ffiClient,
                    dbPath,
                    ffiClient.installationId().toHex(),
                    ffiClient.inboxId(),
                    clientOptions.api.env,
                    publicIdentity,
                )
            }

        // Function to create a client with a signing key
        suspend fun create(
            account: SigningKey,
            options: ClientOptions,
        ): Client =
            withContext(Dispatchers.IO) {
                try {
                    initializeV3Client(account.publicIdentity, options, account)
                } catch (e: Exception) {
                    throw XMTPException("Error creating V3 client: ${e.message}", e)
                }
            }

        // Function to build a client from a address
        suspend fun build(
            publicIdentity: PublicIdentity,
            options: ClientOptions,
            inboxId: InboxId? = null,
        ): Client =
            withContext(Dispatchers.IO) {
                try {
                    initializeV3Client(
                        publicIdentity,
                        options,
                        inboxId = inboxId,
                        buildOffline = inboxId != null,
                    )
                } catch (e: Exception) {
                    throw XMTPException("Error creating V3 client: ${e.message}", e)
                }
            }

        private suspend fun createFfiClient(
            publicIdentity: PublicIdentity,
            inboxId: InboxId,
            options: ClientOptions,
            appContext: Context,
            buildOffline: Boolean = false,
        ): Pair<FfiXmtpClient, String> =
            withContext(Dispatchers.IO) {
                val alias = "xmtp-${options.api.env}-$inboxId"

                val mlsDbDirectory = options.dbDirectory
                val directoryFile =
                    if (mlsDbDirectory != null) {
                        File(mlsDbDirectory)
                    } else {
                        File(appContext.filesDir.absolutePath, "xmtp_db")
                    }

                if (!directoryFile.exists()) {
                    val created = directoryFile.mkdirs()
                    if (!created) {
                        throw XMTPException("Failed to create directory for database at ${directoryFile.absolutePath}")
                    }
                }
                val dbPath = directoryFile.absolutePath + "/$alias.db3"

                val ffiClient =
                    createClient(
                        api = connectToApiBackend(options.api),
                        syncApi = connectToSyncApiBackend(options.api),
                        db =
                            DbOptions(
                                db = dbPath,
                                encryptionKey = options.dbEncryptionKey,
                                maxDbPoolSize = null,
                                minDbPoolSize = null,
                            ),
                        accountIdentifier = publicIdentity.ffiPrivate,
                        inboxId = inboxId,
                        nonce = 0.toULong(),
                        legacySignedPrivateKeyProto = null,
                        deviceSyncServerUrl = options.historySyncUrl,
                        deviceSyncMode =
                            if (!options.deviceSyncEnabled) {
                                FfiSyncWorkerMode.DISABLED
                            } else {
                                FfiSyncWorkerMode.ENABLED
                            },
                        allowOffline = buildOffline,
                        forkRecoveryOpts = options.forkRecoveryOptions?.toFfi(),
                    )
                Pair(ffiClient, dbPath)
            }

        private suspend fun handleSignature(
            signatureRequest: SignatureRequest,
            signingKey: SigningKey,
        ) {
            val signedData = signingKey.sign(signatureRequest.signatureText())

            when (signingKey.type) {
                SignerType.SCW -> {
                    val chainId =
                        signingKey.chainId ?: throw XMTPException("ChainId is required for SCW")
                    signatureRequest.addScwSignature(
                        signedData.rawData,
                        signingKey.publicIdentity.identifier,
                        chainId.toULong(),
                        signingKey.blockNumber?.toULong(),
                    )
                }

                else -> {
                    signatureRequest.addEcdsaSignature(signedData.rawData)
                }
            }
        }

        @DelicateApi(
            "This function is delicate and should be used with caution. Creating an FfiClient without signing or registering will create a broken experience use `create()` instead",
        )
        suspend fun ffiCreateClient(
            publicIdentity: PublicIdentity,
            clientOptions: ClientOptions,
        ): Client =
            withContext(Dispatchers.IO) {
                val recoveredInboxId = getOrCreateInboxId(clientOptions.api, publicIdentity)

                val (ffiClient, dbPath) =
                    createFfiClient(
                        publicIdentity,
                        recoveredInboxId,
                        clientOptions,
                        clientOptions.appContext,
                    )
                Client(
                    ffiClient,
                    dbPath,
                    ffiClient.installationId().toHex(),
                    ffiClient.inboxId(),
                    clientOptions.api.env,
                    publicIdentity,
                )
            }
    }

    suspend fun revokeInstallations(
        signingKey: SigningKey,
        installationIds: List<String>,
    ) = withContext(Dispatchers.IO) {
        val ids = installationIds.map { it.hexToByteArray() }
        val signatureRequest = ffiRevokeInstallations(ids)
        handleSignature(signatureRequest, signingKey)
        ffiApplySignatureRequest(signatureRequest)
    }

    suspend fun revokeAllOtherInstallations(signingKey: SigningKey) =
        withContext(Dispatchers.IO) {
            ffiRevokeAllOtherInstallations()?.let {
                handleSignature(it, signingKey)
                ffiApplySignatureRequest(it)
            }
        }

    @DelicateApi(
        "This function is delicate and should be used with caution. Adding a identity already associated with an inboxId will cause the identity to lose access to that inbox. See: inboxIdFromIdentity(publicIdentity)",
    )
    suspend fun addAccount(
        newAccount: SigningKey,
        allowReassignInboxId: Boolean = false,
    ) = withContext(Dispatchers.IO) {
        val signatureRequest = ffiAddIdentity(newAccount.publicIdentity, allowReassignInboxId)
        handleSignature(signatureRequest, newAccount)
        ffiApplySignatureRequest(signatureRequest)
    }

    suspend fun removeAccount(
        recoverAccount: SigningKey,
        publicIdentityToRemove: PublicIdentity,
    ) = withContext(Dispatchers.IO) {
        val signatureRequest = ffiRevokeIdentity(publicIdentityToRemove)
        handleSignature(signatureRequest, recoverAccount)
        ffiApplySignatureRequest(signatureRequest)
    }

    fun signWithInstallationKey(message: String): ByteArray = ffiClient.signWithInstallationKey(message)

    fun verifySignature(
        message: String,
        signature: ByteArray,
    ): Boolean =
        try {
            ffiClient.verifySignedWithInstallationKey(message, signature)
            true
        } catch (e: Exception) {
            false
        }

    fun verifySignatureWithInstallationId(
        message: String,
        signature: ByteArray,
        installationId: String,
    ): Boolean =
        try {
            ffiClient.verifySignedWithPublicKey(message, signature, installationId.hexToByteArray())
            true
        } catch (e: Exception) {
            false
        }

    suspend fun canMessage(identities: List<PublicIdentity>): Map<String, Boolean> =
        withContext(Dispatchers.IO) {
            val ffiIdentifiers = identities.map { it.ffiPrivate }
            val result = ffiClient.canMessage(ffiIdentifiers)

            result.mapKeys { (ffiIdentifier, _) ->
                ffiIdentifier.identifier
            }
        }

    suspend fun inboxIdFromIdentity(publicIdentity: PublicIdentity): InboxId? =
        withContext(Dispatchers.IO) {
            ffiClient.findInboxId(publicIdentity.ffiPrivate)
        }

    suspend fun deleteLocalDatabase() =
        withContext(Dispatchers.IO) {
            dropLocalDatabaseConnection()
            File(dbPath).delete()
        }

    @DelicateApi(
        "This function is delicate and should be used with caution. App will error if database not properly reconnected. See: reconnectLocalDatabase()",
    )
    suspend fun dropLocalDatabaseConnection() =
        withContext(Dispatchers.IO) {
            ffiClient.releaseDbConnection()
        }

    suspend fun reconnectLocalDatabase() =
        withContext(Dispatchers.IO) {
            ffiClient.dbReconnect()
        }

    suspend fun inboxStatesForInboxIds(
        refreshFromNetwork: Boolean,
        inboxIds: List<InboxId>,
    ): List<InboxState> =
        withContext(Dispatchers.IO) {
            ffiClient
                .addressesFromInboxId(refreshFromNetwork, inboxIds)
                .map { InboxState(it) }
        }

    suspend fun inboxState(refreshFromNetwork: Boolean): InboxState =
        withContext(Dispatchers.IO) {
            InboxState(ffiClient.inboxState(refreshFromNetwork))
        }

    /**
     * Manually trigger a device sync request to sync records from another active device on this account.
     */
    suspend fun sendSyncRequest() =
        withContext(Dispatchers.IO) {
            ffiClient.sendSyncRequest()
        }

    suspend fun createArchive(
        path: String,
        encryptionKey: ByteArray,
        opts: ArchiveOptions = ArchiveOptions(),
    ) = withContext(Dispatchers.IO) {
        ffiClient.createArchive(path, opts.toFfi(), encryptionKey)
    }

    suspend fun importArchive(
        path: String,
        encryptionKey: ByteArray,
    ) = withContext(Dispatchers.IO) {
        ffiClient.importArchive(path, encryptionKey)
    }

    suspend fun archiveMetadata(
        path: String,
        encryptionKey: ByteArray,
    ): ArchiveMetadata =
        withContext(Dispatchers.IO) {
            ArchiveMetadata(ffiClient.archiveMetadata(path, encryptionKey))
        }

    @DelicateApi(
        "This function is delicate and should be used with caution. Should only be used if trying to manage the signature flow independently otherwise use `addAccount(), removeAccount(), or revoke()` instead",
    )
    suspend fun ffiApplySignatureRequest(signatureRequest: SignatureRequest) {
        ffiClient.applySignatureRequest(signatureRequest.ffiSignatureRequest)
    }

    @DelicateApi(
        "This function is delicate and should be used with caution. Should only be used if trying to manage the signature flow independently otherwise use `revokeInstallations()` instead",
    )
    suspend fun ffiRevokeInstallations(ids: List<ByteArray>): SignatureRequest =
        SignatureRequest(ffiClient.revokeInstallations(ids))

    @DelicateApi(
        "This function is delicate and should be used with caution. Should only be used if trying to manage the signature flow independently otherwise use `revokeAllOtherInstallations()` instead",
    )
    suspend fun ffiRevokeAllOtherInstallations(): SignatureRequest? =
        ffiClient.revokeAllOtherInstallationsSignatureRequest()?.let { SignatureRequest(it) }

    @DelicateApi(
        "This function is delicate and should be used with caution. Should only be used if trying to manage the signature flow independently otherwise use `removeAccount()` instead",
    )
    suspend fun ffiRevokeIdentity(publicIdentityToRemove: PublicIdentity): SignatureRequest =
        SignatureRequest(ffiClient.revokeIdentity(publicIdentityToRemove.ffiPrivate))

    @DelicateApi(
        "This function is delicate and should be used with caution. Should only be used if trying to manage the create and register flow independently otherwise use `addAccount()` instead",
    )
    suspend fun ffiAddIdentity(
        publicIdentityToAdd: PublicIdentity,
        allowReassignInboxId: Boolean = false,
    ): SignatureRequest {
        val inboxId: InboxId? =
            if (!allowReassignInboxId) {
                inboxIdFromIdentity(
                    PublicIdentity(
                        publicIdentityToAdd.kind,
                        publicIdentityToAdd.identifier,
                    ),
                )
            } else {
                null
            }

        if (allowReassignInboxId || inboxId.isNullOrBlank()) {
            return SignatureRequest(ffiClient.addIdentity(publicIdentityToAdd.ffiPrivate))
        } else {
            throw XMTPException("This identity is already associated with inbox $inboxId")
        }
    }

    @DelicateApi(
        "This function is delicate and should be used with caution. Should only be used if trying to manage the signature flow independently otherwise use `create()` instead",
    )
    fun ffiSignatureRequest(): SignatureRequest? = ffiClient.signatureRequest()?.let { SignatureRequest(it) }

    @DelicateApi(
        "This function is delicate and should be used with caution. Should only be used if trying to manage the create and register flow independently otherwise use `create()` instead",
    )
    suspend fun ffiRegisterIdentity(signatureRequest: SignatureRequest) {
        ffiClient.registerIdentity(signatureRequest.ffiSignatureRequest)
    }
}
