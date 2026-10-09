package org.xmtp.android.example.messenger.attachments

import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.withContext
import org.xmtp.android.example.BuildConfig
import org.xmtp.android.example.messenger.*
import uniffi.xmtp_sdk.*
import java.util.UUID

internal class AttachmentTestFixture {
    private val previousLifecycle = AndroidStreamLifecycle.enabled

    init {
        AndroidStreamLifecycle.enabled = false
    }

    val context = InstrumentationRegistry.getInstrumentation().targetContext
    val profile = BackendProfile(UUID.randomUUID().toString(), BuildConfig.XMTP_BACKEND_URL, allowPrivateNetwork = true)
    val key = SessionKey(profile.id, 1)
    val paths = profile.paths(context.filesDir)
    val preferences = MessengerPreferences(context)
    val secrets = SecureSecretStore(context)
    var current = true
    lateinit var client: SDKClient
    private var clientEnded = true
    lateinit var group: Conversation
    private lateinit var signer: Signer
    private lateinit var options: ClientOptions

    suspend fun start(
        age: ULong = 86400uL,
        backend: String = BuildConfig.XMTP_BACKEND_URL,
    ) {
        resumeStreams()
        signer = generateLocalSigner()
        paths.database.parentFile!!.mkdirs()
        paths.attachments.mkdirs()
        val location = StorageLocation.Explicit(paths.database.absolutePath, paths.attachments.absolutePath)
        options =
            ClientOptions(
                backend = BackendSource.Options(BackendOptions(url = backend)),
                storage = StorageOptions(location = location),
                deviceSync = false,
                attachments = AttachmentOptions(maxPendingAgeSeconds = age, allowPrivateNetwork = true),
            )
        client = SDKClient.create(context, signer, options)
        clientEnded = false
        group =
            Conversation.Group(
                client.conversations.createGroup(emptyList<InboxId>(), CreateGroupOptions(name = "File proof")),
            )
    }

    fun coordinator() =
        AttachmentDraftCoordinator(
            key,
            client,
            paths,
            preferences,
            secrets,
            SendCoordinator(preferences) {
                current
            },
            { current },
        )

    suspend fun reopen() {
        val inbox = client.inboxId()
        endClient()
        client = SDKClient.build(context, signer.identity(), options, inbox)
        clientEnded = false
        group = checkNotNull(client.conversations.getById(group.id()))
    }

    suspend fun close() =
        withContext(NonCancellable) {
            try {
                current = false
                endClient()
                paths.root.deleteRecursively()
                AttachmentFiles.profileDirectory(context, profile.id).deleteRecursively()
                preferences.removeProfile(profile.id)
            } finally {
                AndroidStreamLifecycle.enabled = previousLifecycle
            }
        }

    suspend fun endClient() {
        if (::client.isInitialized && !clientEnded) {
            client.end()
            clientEnded = true
        }
    }

    suspend fun save(
        remote: RemoteAttachment,
        phase: SendPhase = SendPhase.DRAFT,
        acceptedId: String? = null,
    ): SendDraftRef {
        val id = UUID.randomUUID().toString()
        val draft = SendDraftRef(id, group.id(), "attachment-$id", acceptedId, phase)
        secrets.write(profile.id, checkNotNull(draft.descriptorSecretRef), AttachmentDescriptor.encode(remote))
        preferences.saveDraft(profile.id, draft)
        return draft
    }
}
