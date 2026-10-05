package org.xmtp.android.example

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test

class LogWriterControllerTest {
    @Test(timeout = 5000)
    fun deactivationWaitsForActivationAfterTheOldActivityStops() =
        runBlocking {
            val processScope = CoroutineScope(SupervisorJob() + Dispatchers.Unconfined)
            val releaseActivation = CompletableDeferred<Unit>()
            val events = mutableListOf<String>()
            var activated = true
            val controller =
                LogWriterController(
                    scope = processScope,
                    activate = {
                        events.add("activation started")
                        releaseActivation.await()
                        events.add("enter")
                    },
                    deactivate = { events.add("exit") },
                    isActivated = { activated },
                    saveActivated = {
                        activated = it
                        events.add("saved $it")
                    },
                    dispatcher = Dispatchers.Unconfined,
                )
            try {
                val activation = controller.setActivated(true)
                val oldActivity = launch(start = CoroutineStart.UNDISPATCHED) { activation.await() }
                oldActivity.cancelAndJoin()
                assertFalse(activation.isCancelled)

                // A new Activity uses the same owner while activation is blocked.
                val deactivation = controller.setActivated(false)
                val secondActivity = launch(start = CoroutineStart.UNDISPATCHED) { deactivation.await() }
                secondActivity.cancelAndJoin()
                assertFalse(deactivation.isCancelled)
                val restoration = controller.setActivated(true, restoreOnly = true)
                assertFalse("Deactivation must wait for the native activation", deactivation.isCompleted)
                assertEquals(listOf("activation started"), events)

                releaseActivation.complete(Unit)
                deactivation.await()
                restoration.await()
                assertFalse(activated)
                assertEquals(listOf("activation started", "enter", "saved true", "exit", "saved false"), events)
            } finally {
                releaseActivation.complete(Unit)
                processScope.cancel()
            }
        }
}
