package uniffi.xmtp_sdk

import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.first
import org.junit.After
import org.junit.Assert.*
import org.junit.Before
import org.junit.Test
import org.xmtp.android.library.BuildConfig
import java.util.UUID

class AndroidContractWalletTest {
    private var previousLifecycle = true

    @Before fun setup() =
        runBlocking {
            previousLifecycle = AndroidStreamLifecycle.enabled
            AndroidStreamLifecycle.enabled = false
            resumeStreams()
        }

    @After fun teardown() {
        AndroidStreamLifecycle.enabled = previousLifecycle
    }

    @Test fun contractWalletBuildMembershipMessagingConsentAndAccounts() =
        runBlocking {
            withTimeout(120_000) {
                val context = InstrumentationRegistry.getInstrumentation().targetContext
                val clients = mutableListOf<SDKClient>()
                val wallets = mutableListOf<FakeSCWWallet>()

                fun options() =
                    ClientOptions(
                        backend = BackendSource.Options(BackendOptions(url = BuildConfig.XMTP_BACKEND_URL)),
                        storage =
                            StorageOptions(
                                location = StorageLocation.Default,
                                label = "scw-${UUID.randomUUID()}",
                            ),
                        deviceSync = false,
                    )

                suspend fun create(
                    signer: Signer,
                    configuration: ClientOptions = options(),
                ) = SDKClient.create(context, signer, configuration).also { clients.add(it) }
                try {
                    val scw =
                        FakeSCWWallet
                            .generate(
                                "ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80",
                            ).also {
                                wallets.add(it)
                            }
                    val second =
                        FakeSCWWallet
                            .generate(
                                "59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d",
                            ).also {
                                wallets.add(it)
                            }
                    val eoa = generateLocalSigner()
                    val configuration = options()
                    val client = create(scw, configuration)
                    val peer = create(second)
                    val eoaClient = create(eoa)
                    val inbox = client.inboxId()
                    val storagePath = client.storage().path()
                    client.end()
                    clients.remove(client)
                    val reopened =
                        SDKClient
                            .build(
                                context,
                                scw.identity(),
                                configuration,
                                inbox,
                            ).also { clients.add(it) }
                    assertEquals(inbox, reopened.inboxId())
                    assertEquals(storagePath, reopened.storage().path())
                    assertEquals(inbox, reopened.inboxIdFor(scw.identity()))
                    val unknown = generateLocalSigner().identity()
                    val unknownKey = "ethereum:${unknown.identifier.lowercase()}"
                    assertEquals(
                        mapOf("ethereum:${eoa.identity().identifier.lowercase()}" to true, unknownKey to false),
                        reopened.canMessage(listOf(eoa.identity(), unknown)),
                    )
                    assertEquals(
                        mapOf("ethereum:${scw.identity().identifier.lowercase()}" to true, unknownKey to false),
                        eoaClient.canMessage(listOf(scw.identity(), unknown)),
                    )
                    val group = reopened.conversations().createGroup(listOf(peer.inboxId(), eoaClient.inboxId()))
                    assertEquals(
                        listOf(inbox, peer.inboxId(), eoaClient.inboxId()).sorted(),
                        group.members().map { it.inboxId }.sorted(),
                    )
                    group.sendText("SCW public callback")
                    peer.conversations().syncAll(null)
                    val peerGroup = (checkNotNull(peer.conversations().getById(group.id())) as Conversation.Group).group
                    assertTrue(
                        peerGroup.messages().any {
                            (it.content as? SDKMessageContent.Standard)?.value ==
                                MessageContent.Text("SCW public callback")
                        },
                    )
                    val receipt =
                        async {
                            reopened.messages(group).first {
                                (it.content as? SDKMessageContent.Standard)?.value ==
                                    MessageContent.Text("SCW stream")
                            }
                        }
                    val messageId = peerGroup.sendText("SCW stream")
                    assertEquals(messageId, receipt.await().id)
                    val ready = CompletableDeferred<Unit>()
                    val nextGroupId = CompletableDeferred<String>()
                    val nextGroup =
                        async {
                            reopened
                                .conversationStream(onConnectionStateChange = { previous, _ ->
                                    if (previous == null) ready.complete(Unit)
                                })
                                .first { it.id() == nextGroupId.await() }
                        }
                    ready.await()
                    val streamed = peer.conversations().createGroup(listOf(inbox))
                    nextGroupId.complete(streamed.id())
                    assertEquals(streamed.id(), nextGroup.await().id())
                    reopened.preferences().setConsentStates(
                        listOf(ConsentRecord(ConsentEntity.Conversation(group.id()), ConsentState.DENIED)),
                    )
                    assertEquals(ConsentState.DENIED, group.state().common.consentState)
                    group.updateConsentState(ConsentState.ALLOWED)
                    for (state in listOf(ConsentState.ALLOWED, ConsentState.DENIED)) {
                        reopened.preferences().setConsentStates(
                            listOf(ConsentRecord(ConsentEntity.Inbox(eoaClient.inboxId()), state)),
                        )
                        assertEquals(state, group.members().first { it.inboxId == eoaClient.inboxId() }.consentState)
                    }
                    val added = generateLocalSigner()
                    reopened.unsafeAddAccount(added, false)
                    assertTrue(reopened.inboxState(true).identities.contains(added.identity()))
                    reopened.removeAccount(scw, added.identity())
                    assertFalse(reopened.inboxState(true).identities.contains(added.identity()))
                    assertEquals(scw.identity(), reopened.inboxState(true).recoveryIdentity)
                    try {
                        reopened.removeAccount(added, scw.identity())
                        fail("Recovery identity removal must fail")
                    } catch (
                        error: XmtpException,
                    ) {
                        assertTrue(reopened.inboxState(true).identities.contains(scw.identity()))
                    }
                } finally {
                    try {
                        withContext(NonCancellable) {
                            var failure: Throwable? = null
                            clients.forEach {
                                try {
                                    it.storage().delete()
                                } catch (error: Throwable) {
                                    if (failure == null) failure = error else failure!!.addSuppressed(error)
                                }
                            }
                            failure?.let { throw it }
                        }
                    } finally {
                        wallets.forEach { it.close() }
                    }
                }
            }
        }
}
