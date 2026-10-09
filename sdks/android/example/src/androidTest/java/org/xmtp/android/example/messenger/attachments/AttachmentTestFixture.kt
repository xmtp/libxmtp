package org.xmtp.android.example.messenger.attachments

import androidx.test.platform.app.InstrumentationRegistry
import java.util.UUID
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.withContext
import org.xmtp.android.example.BuildConfig
import org.xmtp.android.example.messenger.*
import uniffi.xmtp_sdk.*

internal class AttachmentTestFixture {
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
    private val signer = generateLocalSigner()
    private lateinit var options: ClientOptions

    suspend fun start(age: ULong = 86400uL, backend: String = BuildConfig.XMTP_BACKEND_URL) {
        paths.database.parentFile!!.mkdirs()
        paths.attachments.mkdirs()
        options = ClientOptions(backend = BackendSource.Options(BackendOptions(url = backend)), storage = StorageOptions(location = StorageLocation.Explicit(paths.database.absolutePath, paths.attachments.absolutePath)), deviceSync = false, attachments = AttachmentOptions(maxPendingAgeSeconds = age, allowPrivateNetwork = true))
        client = SDKClient.create(context, signer, options)
        clientEnded = false
        group = Conversation.Group(client.conversations.createGroup(emptyList<InboxId>(), CreateGroupOptions(name = "File proof")))
    }
    fun coordinator() = AttachmentDraftCoordinator(key, client, paths, preferences, secrets, SendCoordinator(preferences) { current }, { current })
    suspend fun reopen() {
        val inbox = client.inboxId()
        endClient()
        client = SDKClient.build(context, signer.identity(), options, inbox)
        clientEnded = false
        group = checkNotNull(client.conversations.getById(group.id()))
    }
    suspend fun close() = withContext(NonCancellable) {
        current = false
        endClient()
        paths.root.deleteRecursively()
        AttachmentFiles.profileDirectory(context, profile.id).deleteRecursively()
        preferences.removeProfile(profile.id)
    }
    suspend fun endClient() {
        if (::client.isInitialized && !clientEnded) {
            client.end()
            clientEnded = true
        }
    }
    suspend fun save(remote: RemoteAttachment, phase: SendPhase = SendPhase.DRAFT, acceptedId: String? = null): SendDraftRef {
        val id = UUID.randomUUID().toString()
        val draft = SendDraftRef(id, group.id(), "attachment-$id", acceptedId, phase)
        secrets.write(profile.id, checkNotNull(draft.descriptorSecretRef), AttachmentDescriptor.encode(remote))
        preferences.saveDraft(profile.id, draft)
        return draft
    }
}
