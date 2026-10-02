package uniffi.xmtp_sdk

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class AndroidStreamLifecycleTest {
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
    fun failedTransitionRetriesWithoutReportingAppliedState() =
        runTest {
            val applied = mutableListOf<Boolean>()
            val failures = mutableListOf<Throwable>()
            val expected = LinkageError("native transition failed")
            var fail = true
            val controller =
                StreamLifecycleController(this, apply = { live ->
                    if (fail) throw expected
                    applied.add(live)
                }, report = failures::add)
            controller.setLive(false)
            runCurrent()
            assertEquals(listOf(expected), failures)
            assertEquals(emptyList<Boolean>(), applied)
            fail = false
            controller.setLive(false)
            runCurrent()
            assertEquals(listOf(false), applied)
        }
}
