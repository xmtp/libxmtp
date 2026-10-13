package org.xmtp.android.example.messenger.feature.handles

import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test
import org.xmtp.android.example.messenger.SessionKey
import org.xmtp.android.example.shared.AppRoute
import org.xmtp.android.example.shared.feature.handles.HandleMode
import java.io.IOException

/** Choose your handle opens once for each account, until a claim or Skip for now. */
class HandleStepTest {
    private class FakeStore : HandleStepStore {
        val done = mutableSetOf<String>()
        val writes = mutableListOf<SessionKey>()

        override fun handleStepDone(profile: String) = profile in done

        override fun finishHandleStep(key: SessionKey): Boolean {
            writes += key
            done += key.profileId
            return true
        }
    }

    private val key = SessionKey("profile-a", 1)

    @Test fun skipForNowEndsTheStepForEveryLaterStart() =
        runBlocking {
            val store = FakeStore()
            val step = HandleStep(store)
            var reads = 0
            val noHandle: suspend () -> String? = {
                reads++
                null
            }
            assertEquals(AppRoute.ChooseHandle, step.start(key, noHandle))
            // Until the user claims or skips, each start of the account opens the step again.
            assertEquals(AppRoute.ChooseHandle, step.start(key.copy(generation = 2), noHandle))
            assertEquals(emptyList<SessionKey>(), store.writes)
            // Skip for now and a claim in onboarding both finish the step.
            step.finish(key, HandleMode.CHOOSE)
            assertEquals(listOf(key), store.writes)
            assertEquals(AppRoute.Chats, step.start(key.copy(generation = 3), noHandle))
            assertEquals(2, reads)
            // The flag belongs to one account. Another account on this device still gets the step.
            assertEquals(AppRoute.ChooseHandle, step.start(SessionKey("profile-b", 4), noHandle))
        }

    @Test fun changeHandleInSettingsDoesNotFinishTheStep() {
        val store = FakeStore()
        HandleStep(store).finish(key, HandleMode.CHANGE)
        assertEquals(emptyList<SessionKey>(), store.writes)
    }

    @Test fun anInboxWithAHandleNeverGetsTheStep() =
        runBlocking {
            val store = FakeStore()
            val step = HandleStep(store)
            assertEquals(AppRoute.Chats, step.start(key) { "nick" })
            // The flag is written, so a later start needs no read of the inbox state.
            assertEquals(listOf(key), store.writes)
            var reads = 0
            val route =
                step.start(key.copy(generation = 2)) {
                    reads++
                    null
                }
            assertEquals(AppRoute.Chats, route)
            assertEquals(0, reads)
        }

    @Test fun aFailedReadOpensChatsAndKeepsTheStep() =
        runBlocking {
            val store = FakeStore()
            val step = HandleStep(store)
            assertEquals(AppRoute.Chats, step.start(key) { throw IOException("offline") })
            assertEquals(emptyList<SessionKey>(), store.writes)
            assertEquals(AppRoute.ChooseHandle, step.start(key.copy(generation = 2)) { null })
        }

    @Test fun testsThatStartFromChatsTurnTheStepOff() =
        runBlocking {
            val store = FakeStore()
            var reads = 0
            val route =
                HandleStep(store, offer = false).start(key) {
                    reads++
                    null
                }
            assertEquals(AppRoute.Chats, route)
            assertEquals(0, reads)
            assertEquals(emptyList<SessionKey>(), store.writes)
        }
}
