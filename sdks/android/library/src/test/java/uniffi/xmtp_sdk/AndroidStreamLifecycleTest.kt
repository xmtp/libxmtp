package uniffi.xmtp_sdk

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import kotlinx.coroutines.cancel
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class AndroidStreamLifecycleTest {
    @Test
    fun failedRegistrationCanRetryWithoutChangingExistingWaiters() =
        runTest {
            val scope = CoroutineScope(SupervisorJob() + StandardTestDispatcher(testScheduler))
            try {
                val firstGate = CompletableDeferred<Unit>()
                val replacementGate = CompletableDeferred<Unit>()
                val expected = IllegalStateException("transient registration failure")
                var registrations = 0
                val applied = mutableListOf<Boolean>()
                val controller =
                    StreamLifecycleController(scope, apply = { applied.add(it) }, report = { throw it })
                val startup =
                    StreamLifecycleStartup(scope, controller) {
                        registrations += 1
                        if (registrations == 1) {
                            firstGate.await()
                            throw expected
                        }
                        replacementGate.await()
                        false
                    }
                val first = async { runCatching { startup.awaitReady() } }
                val second = async { runCatching { startup.awaitReady() } }
                runCurrent()
                assertEquals(1, registrations)
                firstGate.complete(Unit)
                runCurrent()
                val firstFailure = first.await().exceptionOrNull()
                val secondFailure = second.await().exceptionOrNull()
                assertTrue(firstFailure is IllegalStateException)
                assertTrue(secondFailure is IllegalStateException)
                assertEquals(expected.message, firstFailure?.message)
                assertEquals(expected.message, secondFailure?.message)
                val retry = async { runCatching { startup.awaitReady() } }
                val concurrentRetry = async { runCatching { startup.awaitReady() } }
                runCurrent()
                assertEquals(2, registrations)
                assertFalse(retry.isCompleted)
                assertFalse(concurrentRetry.isCompleted)
                replacementGate.complete(Unit)
                runCurrent()
                assertTrue(retry.await().isSuccess)
                assertTrue(concurrentRetry.await().isSuccess)
                assertEquals(2, registrations)
                assertEquals(listOf(false), applied)
                assertEquals(expected.message, second.await().exceptionOrNull()?.message)
            } finally {
                scope.cancel()
            }
        }

    @Test
    fun newerForegroundCallbackWinsOverBackgroundStartupSnapshot() =
        runTest {
            val applied = mutableListOf<Boolean>()
            val controller =
                StreamLifecycleController(this, apply = { live -> applied.add(live) }, report = { throw it })
            val startup =
                StreamLifecycleStartup(this, controller) { setLive ->
                    setLive(true)
                    false
                }
            startup.awaitReady()
            assertEquals(emptyList<Boolean>(), applied)
        }

    @Test
    fun newerBackgroundCallbackWinsOverForegroundStartupSnapshot() =
        runTest {
            val applied = mutableListOf<Boolean>()
            val controller =
                StreamLifecycleController(this, apply = { live -> applied.add(live) }, report = { throw it })
            val startup =
                StreamLifecycleStartup(this, controller) { setLive ->
                    setLive(false)
                    true
                }
            startup.awaitReady()
            assertEquals(listOf(false), applied)
        }

    @Test
    fun foregroundWaitsForHeldBackgroundTransition() =
        runTest {
            val release = CompletableDeferred<Unit>()
            val entered = mutableListOf<Boolean>()
            val applied = mutableListOf<Boolean>()
            val controller =
                StreamLifecycleController(this, apply = { live ->
                    entered.add(live)
                    if (!live) release.await()
                    applied.add(live)
                }, report = { throw it })
            controller.setLive(false)
            runCurrent()
            assertEquals(listOf(false), entered)
            controller.setLive(true)
            runCurrent()
            assertEquals(listOf(false), entered)
            release.complete(Unit)
            runCurrent()
            assertEquals(listOf(false, true), applied)
        }

    @Test
    fun failedResumeRetriesWithoutReportingAppliedState() =
        runTest {
            val applied = mutableListOf<Boolean>()
            val failures = mutableListOf<Throwable>()
            val expected = LinkageError("native transition failed")
            var fail = true
            val controller =
                StreamLifecycleController(this, apply = { live ->
                    if (live && fail) throw expected
                    applied.add(live)
                }, report = failures::add)
            controller.setLive(false)
            runCurrent()
            controller.setLive(true)
            runCurrent()
            assertEquals(listOf(expected), failures)
            assertEquals(listOf(false), applied)
            fail = false
            controller.setLive(true)
            runCurrent()
            assertEquals(listOf(false, true), applied)
        }

    @Test
    fun foregroundResumesAfterSuspendFailsWithTheNativeLatchParked() =
        runTest {
            val calls = mutableListOf<Boolean>()
            val failures = mutableListOf<Throwable>()
            val expected = LinkageError("suspend failed after parking")
            var parked = false
            val controller =
                StreamLifecycleController(this, apply = { live ->
                    calls.add(live)
                    parked = !live
                    if (!live) throw expected
                }, report = failures::add)
            controller.setLive(false)
            runCurrent()
            assertTrue(parked)
            assertEquals(listOf(expected), failures)
            controller.setLive(true)
            runCurrent()
            assertEquals(listOf(false, true), calls)
            assertFalse(parked)
        }

    @Test
    fun foregroundDuringFailedSuspendStillResumesTheNativeLatch() =
        runTest {
            val release = CompletableDeferred<Unit>()
            val calls = mutableListOf<Boolean>()
            val failures = mutableListOf<Throwable>()
            val expected = LinkageError("held suspend failed after parking")
            var parked = false
            val controller =
                StreamLifecycleController(this, apply = { live ->
                    calls.add(live)
                    parked = !live
                    if (!live) {
                        release.await()
                        throw expected
                    }
                }, report = failures::add)
            controller.setLive(false)
            runCurrent()
            controller.setLive(true)
            runCurrent()
            assertEquals(listOf(false), calls)
            assertTrue(parked)
            release.complete(Unit)
            runCurrent()
            assertEquals(listOf(expected), failures)
            assertEquals(listOf(false, true), calls)
            assertFalse(parked)
        }

    @Test
    fun sharedStartupWaitsForRegistrationAndInitialSuspension() =
        runTest {
            val registered = CompletableDeferred<Unit>()
            val suspended = CompletableDeferred<Unit>()
            val calls = mutableListOf<Boolean>()
            var registrations = 0
            var parked = false
            val controller =
                StreamLifecycleController(this, apply = { live ->
                    calls.add(live)
                    if (!live) suspended.await()
                    parked = !live
                }, report = { throw it })
            val startup =
                StreamLifecycleStartup(this, controller) {
                    registrations += 1
                    registered.await()
                    false
                }
            startup.enable()
            val first = async { startup.awaitReady() }
            val second = async { startup.awaitReady() }
            val cancelled = async { startup.awaitReady() }
            runCurrent()
            assertEquals(1, registrations)
            assertFalse(first.isCompleted)
            assertFalse(second.isCompleted)
            assertEquals(emptyList<Boolean>(), calls)
            cancelled.cancelAndJoin()
            registered.complete(Unit)
            runCurrent()
            assertEquals(listOf(false), calls)
            assertFalse(first.isCompleted)
            assertFalse(second.isCompleted)
            suspended.complete(Unit)
            runCurrent()
            first.await()
            second.await()
            assertTrue(parked)
            assertEquals(1, registrations)
            assertEquals(listOf(false), calls)
        }
}
