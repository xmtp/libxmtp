package org.xmtp.benchmark

import com.google.protobuf.ByteString
import kotlinx.coroutines.delay
import org.json.JSONArray
import org.json.JSONObject
import org.xmtp.android.library.*
import org.xmtp.android.library.codecs.*
import org.xmtp.android.library.libxmtp.DecodedMessage.SortDirection
import org.xmtp.android.library.libxmtp.IdentityKind
import org.xmtp.android.library.libxmtp.PublicIdentity
import uniffi.xmtpv3.FfiContentType
import java.io.File

typealias BenchClient = Client
typealias BenchGroup = Group
typealias BenchLiveMessage = org.xmtp.android.library.libxmtp.DecodedMessage

class BenchSigner(
    val config: HostConfig,
    val key: String,
    val address: String,
    val wait: Long,
    val clock: CallbackClock,
) : SigningKey {
    override val publicIdentity get() = PublicIdentity(IdentityKind.ETHEREUM, address)

    override suspend fun sign(message: String): SignedData {
        clock.mark()
        if (wait > 0) delay(wait)
        return SignedData(unhex(config.signer(obj("key" to key, "text" to message)).getString("signature")))
    }
}

fun options(
    config: HostConfig,
    path: String,
): ClientOptions {
    File(path).mkdirs()
    Client.register(TextCodec())
    Client.register(AttachmentCodec())
    Client.register(ReplyCodec())
    Client.register(ReactionV2Codec())
    return ClientOptions(
        api = ClientOptions.Api(env = XMTPEnvironment.LOCAL, gatewayHost = config.backend),
        appContext = config.context,
        dbEncryptionKey = ByteArray(32) { 7 },
        dbDirectory = path,
        deviceSyncEnabled = false,
    )
}

suspend fun benchCreate(
    config: HostConfig,
    key: String,
    address: String,
    path: String,
    wait: Long,
    clock: CallbackClock,
) = Client.create(BenchSigner(config, key, address, wait, clock), options(config, path))

suspend fun benchOpen(
    config: HostConfig,
    address: String,
    path: String,
    inbox: String,
) = Client.build(PublicIdentity(IdentityKind.ETHEREUM, address), options(config, path), inbox)

@OptIn(DelicateApi::class)
suspend fun benchClose(client: BenchClient) = client.dropLocalDatabaseConnection()

fun benchInbox(client: BenchClient) = client.inboxId

suspend fun benchSync(client: BenchClient) = client.conversations.sync()

fun benchGroupID(group: BenchGroup) = group.id

suspend fun benchGroup(
    client: BenchClient,
    id: String,
) = checkNotNull(client.conversations.findGroup(id))

suspend fun benchNewGroup(
    client: BenchClient,
    members: List<String>,
) = client.conversations.newGroup(members)

suspend fun benchPrepare(
    group: BenchGroup,
    row: JSONObject,
    ids: List<String>,
    inbox: String,
): String =
    when {
        !row.isNull("reply_to") -> {
            group.prepareMessage(
                Reply(ids[row.getString("reply_to").toInt()], row.getString("text"), ContentTypeText),
                noSend = false,
            )
        }

        !row.isNull("attachment") -> {
            row.getJSONObject("attachment").let {
                group.prepareMessage(
                    Attachment(
                        it.getString("filename"),
                        it.getString("mime_type"),
                        ByteString.copyFrom(unhex(it.getString("bytes_hex"))),
                    ),
                    noSend = false,
                )
            }
        }

        else -> {
            group.prepareMessage(row.getString("text"), noSend = false)
        }
    }

suspend fun benchReact(
    group: BenchGroup,
    id: String,
    inbox: String,
    reaction: JSONObject,
) = group.prepareMessage(
    Reaction(id, ReactionAction.Added, reaction.getString("content"), ReactionSchema.Unicode, inbox),
    SendOptions(contentType = ContentTypeReactionV2),
    noSend = false,
)

suspend fun benchPublish(group: BenchGroup) = group.publishMessages()

suspend fun benchGroupSync(group: BenchGroup) = group.sync()

fun benchStream(
    client: BenchClient,
    group: BenchGroup,
) = group.streamMessages()

suspend fun benchPage(
    group: BenchGroup,
    count: Int,
    keys: Map<String, String>,
): List<JSONObject> =
    group
        .enrichedMessages(
            limit = count,
            direction = SortDirection.ASCENDING,
            excludeContentTypes =
                listOf(
                    FfiContentType.REACTION,
                    FfiContentType.GROUP_UPDATED,
                    FfiContentType.GROUP_MEMBERSHIP_CHANGE,
                ),
        ).map { message ->
            val key = checkNotNull(keys[message.id])
            var text: String? = null
            var parent: String? = null
            var parentText: String? = null
            var attachment: JSONObject? = null
            when (message.contentTypeId.typeId) {
                "text" -> {
                    text = checkNotNull(message.content<String>())
                }

                "attachment" -> {
                    checkNotNull(message.content<Attachment>()).let {
                        attachment =
                            obj(
                                "filename" to it.filename,
                                "mime_type" to it.mimeType,
                                "bytes_hex" to hex(it.data.toByteArray()),
                            )
                    }
                }

                "reply" -> {
                    checkNotNull(message.content<org.xmtp.android.library.libxmtp.Reply>()).let {
                        parent = checkNotNull(keys[it.referenceId])
                        text = it.content as String
                        parentText = checkNotNull(it.inReplyTo?.content<String>())
                    }
                }

                else -> {
                    error("Unexpected public content")
                }
            }
            val reactions =
                message.reactions.map { value ->
                    val reaction = checkNotNull(value.content<Reaction>())
                    obj(
                        "content" to reaction.content,
                        "schema" to if (reaction.schema == ReactionSchema.Unicode) "unicode" else "unexpected",
                        "action" to if (reaction.action == ReactionAction.Added) "added" else "unexpected",
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

suspend fun benchLift(
    group: BenchGroup,
    pair: Int,
): JSONObject = error("The class/record check uses the new package")

fun benchLive(message: BenchLiveMessage): LiveEvent =
    when (val content = checkNotNull(message.content<Any>())) {
        is String -> {
            LiveEvent(message.id, "text", text = content)
        }

        is Attachment -> {
            LiveEvent(
                message.id,
                "attachment",
                attachment =
                    LiveAttachment(content.filename, content.mimeType, hex(content.data.toByteArray())),
            )
        }

        is Reply -> {
            LiveEvent(message.id, "reply", text = content.content as String, reference = content.reference)
        }

        is Reaction -> {
            LiveEvent(
                message.id,
                "reaction",
                reference = content.reference,
                reaction =
                    LiveReaction(
                        content.content,
                        if (content.schema == ReactionSchema.Unicode) "unicode" else "unexpected",
                        if (content.action == ReactionAction.Added) "added" else "unexpected",
                    ),
            )
        }

        else -> {
            error("Missing or unsupported live content")
        }
    }
