import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.async
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.withTimeout
import kotlinx.coroutines.yield
import uniffi.xmtp_sdk.*
import java.nio.file.Files
import java.util.concurrent.atomic.AtomicInteger

internal const val LIFETIME_CYCLES = 20
internal const val LIFETIME_CALLS = 32
internal const val LIFETIME_TIMEOUT = 30_000L

internal suspend fun callbackLifetime(backend: BackendOptions) {
    val selected = System.getenv("SDK_CALLBACK_LIFETIME_FAMILY")
    for (family in listOf("identity", "kind", "sign", "preAuthenticate")) {
        if (selected != null && selected != family) continue
        repeat(LIFETIME_CYCLES) { creationLifetimeCycle(backend, family) }
        println("Kotlin callback lifetime: $family, $LIFETIME_CYCLES cycles passed")
    }
    callbackLifetimeOperations(backend, selected)
    if (selected == null || selected == "constructorFailure") constructorFailure(backend)
    if (selected == null || selected == "adoption") constructorLifetime(backend)
    val runtime = Runtime.getRuntime()
    println(
        "Kotlin lifetime memory: used=${runtime.totalMemory() - runtime.freeMemory()}, " +
            "threads=${Thread.getAllStackTraces().size}",
    )
}

internal fun lifetimeOptions(backend: BackendOptions) =
    ClientOptions(
        backend = BackendSource.Options(backend),
        storage = StorageOptions(location = StorageLocation.InMemory),
        deviceSync = false,
    )

internal suspend fun lifetimeDrained() =
    try {
        withTimeout(LIFETIME_TIMEOUT) {
            while (true) {
                val tasks = sdkConformanceForeignCallCounts()
                if (tasks.inFlight == 0uL && tasks.running == 0uL &&
                    sdkConformanceCallbackHandleCounts().values.all { it == 0 }
                ) {
                    check(tasks.droppedEarly == 0uL) { "foreign future dropped before completion" }
                    check(tasks.pollsOnCallerThread == 0uL) { "foreign future polled on caller thread" }
                    break
                }
                yield()
            }
        }
    } catch (error: Exception) {
        println(
            "Kotlin lifetime drain failure: tasks=${sdkConformanceForeignCallCounts()}, " +
                "handles=${sdkConformanceCallbackHandleCounts()}",
        )
        throw error
    }

private suspend fun creationLifetimeCycle(
    backend: BackendOptions,
    family: String,
) = coroutineScope {
    val width = if (family == "preAuthenticate") 1 else LIFETIME_CALLS
    val entered = AtomicInteger()
    val active = AtomicInteger()
    val ended = AtomicInteger()
    val completed = AtomicInteger()
    val allEntered = CompletableDeferred<Unit>()
    val action = CompletableDeferred<Unit>()
    val reentered = CompletableDeferred<Unit>()
    val release = CompletableDeferred<Unit>()
    val finished = CompletableDeferred<Unit>()
    val independentSigner = generateLocalSigner()
    val independent = Client.create(independentSigner, lifetimeOptions(backend))
    val signers = mutableListOf<Signer>()

    suspend fun hold() {
        active.incrementAndGet()
        if (entered.incrementAndGet() == width) allEntered.complete(Unit)
        try {
            action.await()
            independent.end()
            if (ended.incrementAndGet() == width) reentered.complete(Unit)
            release.await()
        } finally {
            active.decrementAndGet()
            if (completed.incrementAndGet() == width) finished.complete(Unit)
        }
    }
    val calls =
        (0 until width).map {
            val base = generateLocalSigner()
            signers.add(base)
            val signer =
                object : Signer {
                    override suspend fun identity(): PublicIdentity {
                        if (family == "identity") hold()
                        return base.identity()
                    }

                    override suspend fun kind(): SignerKind {
                        if (family == "kind") hold()
                        return base.kind()
                    }

                    override suspend fun sign(request: SigningRequest): Signature {
                        if (family == "sign") hold()
                        return base.sign(request)
                    }
                }
            var options = lifetimeOptions(backend)
            if (family == "preAuthenticate") {
                options =
                    options.copy(
                        handlers =
                            ClientHandlers(
                                preAuthenticate =
                                    object : PreAuthenticate {
                                        override suspend fun run() = hold()
                                    },
                            ),
                    )
            }
            async { Client.create(signer, options) }
        }
    try {
        withTimeout(LIFETIME_TIMEOUT) { allEntered.await() }
        check(active.get() == width)
        check(sdkConformanceForeignCallCounts().running >= width.toULong())
        // UniFFI starts the callback coroutine before it stores that job's handle.
        withTimeout(LIFETIME_TIMEOUT) {
            while (sdkConformanceCallbackHandleCounts().getValue("foreignFutures") < width) yield()
        }
        calls.forEach { it.cancel() }
        withTimeout(LIFETIME_TIMEOUT) { calls.forEach { it.join() } }
        calls.forEach { check(it.isCancelled) }
        check(active.get() == width) { "caller cancellation ended a host callback" }
        action.complete(Unit)
        withTimeout(LIFETIME_TIMEOUT) { reentered.await() }
        check(active.get() == width)
        release.complete(Unit)
        withTimeout(LIFETIME_TIMEOUT) { finished.await() }
    } finally {
        action.complete(Unit)
        release.complete(Unit)
        calls.forEach { it.cancelAndJoin() }
        independent.end()
        withTimeout(LIFETIME_TIMEOUT) {
            while (sdkConformanceForeignCallCounts().inFlight != 0uL) yield()
        }
        independent.destroy()
        (independentSigner as? Disposable)?.destroy()
        signers.forEach { (it as? Disposable)?.destroy() }
    }
    lifetimeDrained()
}

private suspend fun constructorLifetime(backend: BackendOptions) =
    coroutineScope {
        val directory = Files.createTempDirectory("xmtp-kotlin-adoption-")
        try {
            for (build in listOf(false, true)) {
                for (cancel in listOf(false, true)) {
                    repeat(LIFETIME_CYCLES) { cycle ->
                        val path = directory.resolve("$build-$cancel-$cycle.db3")
                        val options =
                            lifetimeOptions(backend).copy(
                                storage =
                                    StorageOptions(
                                        location = StorageLocation.Explicit(path.toString(), "$path.attachments"),
                                        singleConnection = true,
                                    ),
                            )
                        val signer = generateLocalSigner()
                        val identity = signer.identity()
                        if (build) {
                            val seed = Client.create(signer, options)
                            seed.end()
                            seed.destroy()
                        }
                        val probe = SdkConformanceConstructorProbe.open()
                        val pending =
                            async { if (build) probe.build(identity, options, null) else probe.create(signer, options) }
                        try {
                            withTimeout(LIFETIME_TIMEOUT) { probe.waitForCompleted() }
                            val held = probe.state()
                            check(
                                held.clientCaptured && held.storeConnected && !held.clientClosed &&
                                    !held.workersStopped && !held.storeOpenReported,
                            )
                            if (cancel) {
                                pending.cancelAndJoin()
                                withTimeout(LIFETIME_TIMEOUT) { probe.waitForCleanup() }
                                val cleaned = probe.state()
                                check(
                                    cleaned.storeOpenReported && cleaned.clientClosed && cleaned.workersStopped &&
                                        !cleaned.storeConnected,
                                )
                            } else {
                                probe.release()
                                withTimeout(LIFETIME_TIMEOUT) { pending.await() }
                                check(probe.state().storeConnected && !probe.state().storeOpenReported)
                                probe.endAdopted()
                            }
                        } finally {
                            pending.cancelAndJoin()
                            probe.cleanup()
                            probe.destroy()
                            (signer as? Disposable)?.destroy()
                        }
                        lifetimeDrained()
                    }
                    println("Kotlin constructor lifetime: build=$build, cancel=$cancel, $LIFETIME_CYCLES cycles passed")
                }
            }
        } finally {
            directory.toFile().deleteRecursively()
        }
    }

private suspend fun constructorFailure(backend: BackendOptions) =
    coroutineScope {
        val directory = Files.createTempDirectory("xmtp-kotlin-failure-")
        try {
            for (build in listOf(false, true)) {
                repeat(LIFETIME_CYCLES) { cycle ->
                    val path = directory.resolve("$build-$cycle.db3")
                    val options =
                        lifetimeOptions(backend).copy(
                            storage =
                                StorageOptions(
                                    location = StorageLocation.Explicit(path.toString(), "$path.attachments"),
                                    singleConnection = true,
                                ),
                        )
                    val base = generateLocalSigner()
                    val probe = SdkConformanceConstructorProbe.open()
                    val badSigner =
                        object : Signer {
                            override suspend fun identity() = base.identity()

                            override suspend fun kind(): SignerKind = throw SignerException.Failed()

                            override suspend fun sign(request: SigningRequest) = base.sign(request)
                        }
                    val pending =
                        async {
                            runCatching {
                                if (build) {
                                    probe.build(
                                        base.identity(),
                                        options,
                                        null,
                                    )
                                } else {
                                    probe.create(badSigner, options)
                                }
                            }
                        }
                    try {
                        withTimeout(LIFETIME_TIMEOUT) { probe.waitForCompleted() }
                        val state = probe.state()
                        check(!state.storeOpenReported && !state.storeConnected)
                        if (!build) check(state.clientCaptured && state.clientClosed && state.workersStopped)
                        probe.release()
                        check(withTimeout(LIFETIME_TIMEOUT) { pending.await() }.isFailure)
                    } finally {
                        probe.cleanup()
                        probe.destroy()
                        (base as? Disposable)?.destroy()
                    }
                    lifetimeDrained()
                }
            }
        } finally {
            directory.toFile().deleteRecursively()
        }
        println("Kotlin constructor lifetime: clean create/build failure, $LIFETIME_CYCLES cycles passed")
    }
