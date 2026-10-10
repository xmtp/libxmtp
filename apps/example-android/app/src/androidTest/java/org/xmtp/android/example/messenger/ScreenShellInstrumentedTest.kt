package org.xmtp.android.example.messenger

import org.junit.Assert.*
import org.junit.Test

/** Check only shell/display state. This test starts no Activity or SDK client. */
class ScreenShellInstrumentedTest {
    @Test fun shellExitReadbackAndRestorationAreChecked() {
        val original = readScreenDisplay(::checkedScreenShell)
        println("SCREEN_SHELL_PROOF original=$original")
        val restore = screenDisplayRestoreCommands(original.scale, original.size, original.density)
        var failure: Throwable? = null
        try {
            assertEquals("result", checkedScreenShell("printf result"))
            assertThrows(IllegalStateException::class.java) { checkedScreenShell("false") }
            val wrongReadback =
                assertThrows(IllegalStateException::class.java) {
                    withScreenDisplay({ command ->
                        checkedScreenShell(
                            if (command ==
                                "wm size 320x640"
                            ) {
                                "wm size 321x641"
                            } else {
                                command
                            },
                        )
                    }) { fail("Wrong readback reached body") }
                }
            assertEquals("Display setup readback did not match", wrongReadback.message)
            assertEquals(original, readScreenDisplay(::checkedScreenShell))
            val primary = AssertionError("body proof")
            var restoring = false
            val attempted = mutableListOf<String>()
            val caught =
                assertThrows(AssertionError::class.java) {
                    withScreenDisplay({ command ->
                        if (restoring && command in restore) {
                            attempted += command
                            checkedScreenShell(if (command == restore.first()) "false" else command)
                        } else {
                            checkedScreenShell(command)
                        }
                    }) {
                        restoring = true
                        throw primary
                    }
                }
            assertSame(primary, caught)
            assertEquals(restore, attempted)
            assertTrue(caught.suppressed.any { it.message?.contains("exit 1") == true })
            println(
                "SCREEN_SHELL_PROOF nonzeroRejected=true wrongReadbackRejected=true restoreAttempts=${attempted.size} primaryPreserved=${caught === primary}",
            )
        } catch (error: Throwable) {
            failure = error
        } finally {
            try {
                restoreScreenDisplay(::checkedScreenShell, original)
            } catch (error: Throwable) {
                if (failure == null) {
                    failure = error
                } else if (failure !== error) {
                    failure?.addSuppressed(error)
                }
            }
            try {
                assertEquals(original, readScreenDisplay(::checkedScreenShell))
            } catch (error: Throwable) {
                if (failure == null) {
                    failure = error
                } else if (failure !== error) {
                    failure?.addSuppressed(error)
                }
            }
        }
        failure?.let { throw it }
        println("SCREEN_SHELL_PROOF restored=${readScreenDisplay(::checkedScreenShell)}")
    }
}
