package org.xmtp.android.example

import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test
import uniffi.xmtp_sdk.LogLevel

class PersistentLogSessionTest {
    @Test
    fun levelsFollowTheFileSession() =
        runBlocking {
            val events = mutableListOf<String>()
            val session = PersistentLogSession({ events.add(it.name) }, { events.add("enter") }, { events.add("exit") })
            session.activate()
            session.deactivate()
            assertEquals(listOf("INFO", "DEBUG", "enter", "exit", "INFO"), events)
        }

    @Test
    fun failedEntryResetsLevelAndPreservesBothErrors() =
        runBlocking {
            val events = mutableListOf<String>()
            val entryError = IllegalStateException("entry")
            val resetError = IllegalStateException("reset")
            var resetting = false
            val session =
                PersistentLogSession(
                    {
                        events.add(it.name)
                        if (resetting) throw resetError
                    },
                    {
                        resetting = true
                        throw entryError
                    },
                    {},
                )
            val actual = runCatching { session.activate() }.exceptionOrNull()
            assertSame(entryError, actual)
            assertArrayEquals(arrayOf(resetError), actual!!.suppressed)
            assertEquals(listOf("INFO", "DEBUG", "INFO"), events)
        }

    @Test
    fun failedExitStillResetsLevelAndPreservesBothErrors() =
        runBlocking {
            val events = mutableListOf<String>()
            val exitError = IllegalStateException("exit")
            val resetError = IllegalStateException("reset")
            val session =
                PersistentLogSession(
                    {
                        events.add(it.name)
                        throw resetError
                    },
                    {},
                    {
                        events.add("exit")
                        throw exitError
                    },
                )
            val actual = runCatching { session.deactivate() }.exceptionOrNull()
            assertSame(exitError, actual)
            assertArrayEquals(arrayOf(resetError), actual!!.suppressed)
            assertEquals(listOf("exit", "INFO"), events)
        }

    @Test
    fun resetFailureAfterSuccessfulExitIsReported() =
        runBlocking {
            val resetError = IllegalStateException("reset")
            val session = PersistentLogSession({ throw resetError }, {}, {})
            assertSame(resetError, runCatching { session.deactivate() }.exceptionOrNull())
        }
}
