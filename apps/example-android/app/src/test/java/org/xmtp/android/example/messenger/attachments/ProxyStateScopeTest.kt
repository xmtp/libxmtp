package org.xmtp.android.example.messenger.attachments

import kotlinx.coroutines.*
import org.junit.Assert.*
import org.junit.Test

class ProxyStateScopeTest {
    private class StubProxy(
        var enabled: Boolean,
    ) {
        val toxics = mutableSetOf("another-task-hold")
        val calls = mutableListOf<String>()
        var closed = false

        fun scope() =
            ProxyStateScope(
                readEnabled = {
                    calls += "read"
                    enabled
                },
                writeEnabled = {
                    delay(1)
                    calls += "restore:$it"
                    enabled = it
                },
                releaseHold = {
                    delay(1)
                    calls += "release-owned"
                    toxics.remove("messenger-attachment-hold")
                    Unit
                },
            )

        suspend fun close() {
            delay(1)
            calls += "close"
            closed = true
        }
    }

    @Test fun restoresEachOriginalStateAfterNormalAndFailedBodies() =
        runBlocking<Unit> {
            for (original in listOf(false, true)) {
                for (fail in listOf(false, true)) {
                    val proxy = StubProxy(original)
                    val result =
                        runCatching {
                            proxy.scope().run(proxy::close) {
                                assertEquals(listOf("read"), proxy.calls)
                                proxy.enabled = true
                                proxy.toxics += "messenger-attachment-hold"
                                if (fail) error("Actual test body failed")
                            }
                        }
                    assertEquals(fail, result.isFailure)
                    assertEquals("Restore the exact prior proxy enabled state", original, proxy.enabled)
                    assertEquals(setOf("another-task-hold"), proxy.toxics)
                    assertTrue(proxy.closed)
                    assertEquals(listOf("read", "release-owned", "close", "restore:$original"), proxy.calls)
                }
            }
        }

    @Test fun cancellationStillReleasesOnlyTheOwnedHoldAndRestoresState() =
        runBlocking<Unit> {
            val proxy = StubProxy(true)
            val entered = CompletableDeferred<Unit>()
            val work =
                launch {
                    proxy.scope().run(proxy::close) {
                        proxy.enabled = false
                        proxy.toxics += "messenger-attachment-hold"
                        entered.complete(Unit)
                        awaitCancellation()
                    }
                }
            entered.await()
            work.cancelAndJoin()
            assertTrue(proxy.enabled)
            assertEquals(setOf("another-task-hold"), proxy.toxics)
            assertTrue(proxy.closed)
            assertEquals(listOf("read", "release-owned", "close", "restore:true"), proxy.calls)
        }

    @Test fun failedReleaseStillClosesTheFixtureAndRestoresThePriorState() =
        runBlocking<Unit> {
            val proxy = StubProxy(false)
            val scope =
                ProxyStateScope(
                    readEnabled = { proxy.enabled },
                    writeEnabled = { proxy.enabled = it },
                    releaseHold = { error("Owned toxic API failed") },
                )
            val result = runCatching { scope.run(proxy::close) { proxy.enabled = true } }
            assertEquals("Owned toxic API failed", result.exceptionOrNull()?.message)
            assertFalse("Restore disabled state even when the named release fails", proxy.enabled)
            assertTrue(proxy.closed)
            assertEquals(setOf("another-task-hold"), proxy.toxics)
        }
}
