package org.xmtp.benchmark

import kotlinx.coroutines.delay
import org.json.JSONArray
import org.json.JSONObject
import uniffi.xmtp_sdk.*
import java.io.File

typealias BenchClient = SDKClient
typealias BenchGroup = Group

class BenchSigner(
    val config: HostConfig,
    val key: String,
    val address: String,
    val wait: Long,
    val clock: CallbackClock,
) : Signer {
    override suspend fun identity() = PublicIdentity(address, PublicIdentityKind.ETHEREUM)

    override suspend fun kind() = SignerKind.Eoa

    override suspend fun sign(request: SigningRequest): Signature {
        clock.mark()
        if (wait > 0) delay(wait)
        return Signature.Ecdsa(unhex(config.signer(obj("key" to key, "text" to request.text)).getString("signature")))
    }
}

fun options(
    config: HostConfig,
    path: String,
): ClientOptions {
    File(path).mkdirs()
    return ClientOptions(
        backend = BackendSource.Options(BackendOptions(config.backend)),
        storage =
            StorageOptions(
                location = StorageLocation.Explicit("$path/client.db", "$path/attachments"),
                encryptionKey = ByteArray(32) { 7 },
            ),
        deviceSync = false,
    )
}

suspend fun benchCreate(
    config: HostConfig,
    key: String,
    address: String,
    path: String,
    wait: Long,
    clock: CallbackClock,
) = SDKClient.create(BenchSigner(config, key, address, wait, clock), options(config, path))

suspend fun benchOpen(
    config: HostConfig,
    address: String,
    path: String,
    inbox: String,
) = SDKClient.build(PublicIdentity(address, PublicIdentityKind.ETHEREUM), options(config, path), inbox)

suspend fun benchClose(client: BenchClient) = client.end()

fun benchInbox(client: BenchClient) = client.inboxId()

suspend fun benchSync(client: BenchClient) = client.conversations().sync()

fun benchGroupID(group: BenchGroup) = group.id()

suspend fun benchGroup(
    client: BenchClient,
    id: String,
) = (client.conversations().getById(id) as Conversation.Group).group

suspend fun benchNewGroup(
    client: BenchClient,
    members: List<String>,
) = client.conversations().createGroup(members, null)

suspend fun benchPrepare(
    group: BenchGroup,
    row: JSONObject,
    ids: List<String>,
    inbox: String,
): String {
    val value =
        when {
            !row.isNull("reply_to") -> {
                StandardContent.Reply(ids[row.getString("reply_to").toInt()], inbox, encodeText(row.getString("text")))
            }

            !row.isNull("attachment") -> {
                row.getJSONObject("attachment").let {
                    StandardContent.Attachment(
                        Attachment(
                            it.getString("filename"),
                            it.getString("mime_type"),
                            unhex(it.getString("bytes_hex")),
                        ),
                    )
                }
            }

            else -> {
                StandardContent.Text(row.getString("text"))
            }
        }
    return group.prepareMessage(encodeStandard(value))
}

suspend fun benchReact(
    group: BenchGroup,
    id: String,
    inbox: String,
    reaction: JSONObject,
) = group.prepareMessage(
    encodeStandard(
        StandardContent.Reaction(
            id,
            inbox,
            Reaction(reaction.getString("content"), ReactionAction.ADDED, ReactionSchema.UNICODE),
        ),
    ),
)

suspend fun benchPublish(group: BenchGroup) = group.publishMessages()

suspend fun benchGroupSync(group: BenchGroup) = group.sync()

fun benchStream(
    client: BenchClient,
    group: BenchGroup,
) = client.messages(group)

suspend fun rows(
    group: BenchGroup,
    count: Int,
) = group.messages(
    ListMessagesOptions(
        limit = count.toUInt(),
        direction = MessageOrder.ASCENDING,
        contentTypes =
            listOf("text", "reply", "attachment").map {
                ContentTypeId("xmtp.org", it, 1u, 0u)
            },
    ),
)

suspend fun benchPage(
    group: BenchGroup,
    count: Int,
    keys: Map<String, String>,
): List<JSONObject> =
    rows(group, count).map { message ->
        val key = checkNotNull(keys[message.id])
        var text: String? = null
        var parent: String? = null
        var parentText: String? = null
        var attachment: JSONObject? = null
        when (val content = (message.content as SDKMessageContent.Standard).value) {
            is MessageContent.Text -> {
                text = content.v1
            }

            is MessageContent.Attachment -> {
                attachment =
                    obj(
                        "filename" to checkNotNull(content.v1.filename),
                        "mime_type" to content.v1.mimeType,
                        "bytes_hex" to hex(content.v1.content),
                    )
            }

            is MessageContent.Reply -> {
                parent = checkNotNull(keys[content.referenceId])
                text = (content.body as MessageBody.Text).v1
                parentText = ((message.inReplyToContent as SDKReplyContent.Standard).value as MessageBody.Text).v1
            }

            else -> {
                error("Unexpected public content")
            }
        }
        val reactions =
            message.reactions.map {
                obj(
                    "content" to it.reaction.content,
                    "schema" to if (it.reaction.schema == ReactionSchema.UNICODE) "unicode" else "unexpected",
                    "action" to if (it.reaction.action == ReactionAction.ADDED) "added" else "unexpected",
                )
            }
        obj(
            "key" to key,
            "text" to text,
            "reply_to" to parent,
            "parent_text" to parentText,
            "attachment" to attachment,
            "reactions" to JSONArray(reactions),
        )
    }
