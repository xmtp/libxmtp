import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.withTimeout
import kotlinx.coroutines.yield
import uniffi.xmtp_sdk.*
import java.util.concurrent.atomic.AtomicInteger

internal suspend fun callbackLifetimeOperations(
    backend: BackendOptions,
    selected: String?,
) {
    for (family in listOf("eventStop", "eventEnd", "signatureRequest", "credential")) {
        if (selected != null && selected != family) continue
        repeat(LIFETIME_CYCLES) {
            when (family) {
                "signatureRequest" -> signatureLifetimeCycle(backend)
                "credential" -> credentialLifetimeCycle(backend)
                else -> eventLifetimeCycle(backend, family == "eventEnd")
            }
        }
        println("Kotlin callback lifetime: $family, $LIFETIME_CYCLES cycles passed")
    }
}

private suspend fun eventLifetimeCycle(
    backend: BackendOptions,
    ownEnd: Boolean,
) {
    val signer = generateLocalSigner()
    val client = Client.create(signer, lifetimeOptions(backend))
    val entered = CompletableDeferred<Unit>()
    val action = CompletableDeferred<Unit>()
    val reentered = CompletableDeferred<Unit>()
    val release = CompletableDeferred<Unit>()
    val finished = CompletableDeferred<Unit>()
    val calls = AtomicInteger()
    val active = AtomicInteger()
    var id = 0uL
    try {
        id =
            client.startListener(
                EventFilter(listOf(EventKind.HMAC_KEYS_UPDATED), null, null, false),
                object : EventListener {
                    override suspend fun onEvent(event: ClientEvent) {
                        calls.incrementAndGet()
                        active.incrementAndGet()
                        entered.complete(Unit)
                        try {
                            action.await()
                            if (ownEnd) client.end() else client.stopListener(id)
                            reentered.complete(Unit)
                            release.await()
                        } finally {
                            active.decrementAndGet()
                            finished.complete(Unit)
                        }
                    }
                },
            )
        client.sdkConformanceEmitHmacEvents(1u)
        withTimeout(LIFETIME_TIMEOUT) { entered.await() }
        client.sdkConformanceEmitHmacEvents(1030u)
        val queued = client.sdkConformanceListenerCounts(id)
        check(queued.registered && queued.queued == 1023uL && queued.inFlight == 1uL && queued.discarded == 7uL)
        check(calls.get() == 1)
        check(sdkConformanceForeignCallCounts().running >= 1uL)
        action.complete(Unit)
        withTimeout(LIFETIME_TIMEOUT) { reentered.await() }
        check(active.get() == 1 && calls.get() == 1)
        check(!client.sdkConformanceListenerCounts(id).registered)
        release.complete(Unit)
        withTimeout(LIFETIME_TIMEOUT) { finished.await() }
        yield()
        check(calls.get() == 1) { "queued event handed off after stop/end" }
    } finally {
        action.complete(Unit)
        release.complete(Unit)
        client.stopListener(id)
        client.end()
        client.destroy()
        (signer as? Disposable)?.destroy()
    }
    lifetimeDrained()
}

private suspend fun signatureLifetimeCycle(backend: BackendOptions) =
    coroutineScope {
        val entered = CompletableDeferred<Unit>()
        val release = CompletableDeferred<Unit>()
        val active = AtomicInteger()
        val total = AtomicInteger()
        val clients = mutableListOf<Client>()
        val requests = mutableListOf<SignatureRequest>()
        val signers = mutableListOf<Signer>()
        val calls = mutableListOf<kotlinx.coroutines.Deferred<Unit>>()
        try {
            repeat(LIFETIME_CALLS) {
                val base = generateLocalSigner()
                signers.add(base)
                val options = lifetimeOptions(backend).copy(registration = RegistrationOptions(auto = false))
                val client = Client.create(base, options)
                clients.add(client)
                val request = checkNotNull(client.unsafeCreateInboxSignatureRequest())
                requests.add(request)
                calls.add(
                    async {
                        request.sign(
                            object : Signer {
                                override suspend fun identity() = base.identity()

                                override suspend fun kind() = base.kind()

                                override suspend fun sign(request: SigningRequest): Signature {
                                    active.incrementAndGet()
                                    if (total.incrementAndGet() == LIFETIME_CALLS) entered.complete(Unit)
                                    try {
                                        release.await()
                                        return base.sign(request)
                                    } finally {
                                        active.decrementAndGet()
                                    }
                                }
                            },
                        )
                    },
                )
            }
            withTimeout(LIFETIME_TIMEOUT) { entered.await() }
            check(sdkConformanceForeignCallCounts().running >= LIFETIME_CALLS.toULong())
            withTimeout(LIFETIME_TIMEOUT) { clients.map { async { it.end() } }.awaitAll() }
            check(active.get() == LIFETIME_CALLS)
            release.complete(Unit)
            withTimeout(LIFETIME_TIMEOUT) { calls.awaitAll() }
            check(active.get() == 0)
            clients.zip(requests).forEach { (client, request) ->
                check(
                    runCatching {
                        client.unsafeApplySignatureRequest(
                            request,
                        )
                    }.exceptionOrNull() is XmtpException.ClientClosed,
                )
            }
        } finally {
            release.complete(Unit)
            calls.forEach { it.cancelAndJoin() }
            clients.forEach {
                it.end()
                it.destroy()
            }
            requests.forEach { it.destroy() }
            signers.forEach { (it as? Disposable)?.destroy() }
        }
        lifetimeDrained()
    }

// The credential callback can end an independent client. It must return before
// its own client ends because that end waits for the callback's foreground call.
private suspend fun credentialLifetimeCycle(backend: BackendOptions) =
    coroutineScope {
        val entered = CompletableDeferred<Unit>()
        val action = CompletableDeferred<Unit>()
        val reentered = CompletableDeferred<Unit>()
        val release = CompletableDeferred<Unit>()
        val finished = CompletableDeferred<Unit>()
        val active = AtomicInteger()
        val entries = AtomicInteger()
        val reentries = AtomicInteger()
        val completions = AtomicInteger()
        val independentSigner = generateLocalSigner()
        val independent = Client.create(independentSigner, lifetimeOptions(backend))
        val clients = mutableListOf<Client>()
        val groups = mutableListOf<Group>()
        val signers = mutableListOf<Signer>()
        val calls = mutableListOf<kotlinx.coroutines.Deferred<Unit>>()
        try {
            repeat(LIFETIME_CALLS) {
                val armed =
                    java.util.concurrent.atomic
                        .AtomicBoolean()
                val source =
                    object : CredentialSource {
                        override suspend fun credential(): Credential {
                            if (armed.get()) {
                                active.incrementAndGet()
                                if (entries.incrementAndGet() == LIFETIME_CALLS) entered.complete(Unit)
                                try {
                                    action.await()
                                    independent.end()
                                    if (reentries.incrementAndGet() == LIFETIME_CALLS) reentered.complete(Unit)
                                    release.await()
                                } finally {
                                    active.decrementAndGet()
                                    if (completions.incrementAndGet() == LIFETIME_CALLS) finished.complete(Unit)
                                }
                            }
                            return Credential(null, "Bearer callback-lifetime", 9_000_000_000_000_000L)
                        }
                    }
                val options =
                    lifetimeOptions(
                        backend,
                    ).copy(backend = BackendSource.Options(backend.copy(credentials = source)))
                val signer = generateLocalSigner()
                signers.add(signer)
                val client = Client.create(signer, options)
                clients.add(client)
                val conversations = client.conversations()
                val group = conversations.createGroup(emptyList(), null)
                groups.add(group)
                conversations.destroy()
                client.setCredential(Credential(null, "Bearer expired", 0L))
                armed.set(true)
                calls.add(async { group.sync() })
            }
            withTimeout(LIFETIME_TIMEOUT) { entered.await() }
            check(active.get() == LIFETIME_CALLS)
            check(sdkConformanceForeignCallCounts().running >= LIFETIME_CALLS.toULong())
            calls.forEach { it.cancel() }
            withTimeout(LIFETIME_TIMEOUT) { calls.forEach { it.join() } }
            calls.forEach { check(it.isCancelled) }
            check(active.get() == LIFETIME_CALLS)
            action.complete(Unit)
            withTimeout(LIFETIME_TIMEOUT) { reentered.await() }
            val ends = clients.map { async { it.end() } }
            yield()
            check(active.get() == LIFETIME_CALLS && ends.none { it.isCompleted })
            release.complete(Unit)
            withTimeout(LIFETIME_TIMEOUT) {
                finished.await()
                ends.awaitAll()
            }
            groups.forEach { check(runCatching { it.sync() }.exceptionOrNull() is XmtpException.ClientClosed) }
        } finally {
            action.complete(Unit)
            release.complete(Unit)
            calls.forEach { it.cancelAndJoin() }
            independent.end()
            independent.destroy()
            (independentSigner as? Disposable)?.destroy()
            clients.forEach {
                it.end()
                it.destroy()
            }
            groups.forEach { it.destroy() }
            signers.forEach { (it as? Disposable)?.destroy() }
        }
        lifetimeDrained()
    }
