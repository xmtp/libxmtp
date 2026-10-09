package org.xmtp.android.example.messenger

import org.junit.Assert.*
import org.junit.Test

class ScreenDisplayContractTest {
    @Test fun physicalDefaultsCountOnlyWhenTheyEqualTheExactTarget() {
        var scale = "1.0"
        var physicalSize = "320x640"

        fun shell(command: String): String {
            when (command) {
                "settings get system font_scale" -> return scale
                "wm size" -> return "Physical size: $physicalSize"
                "wm density" -> return "Physical density: 160"
                "settings put system font_scale 2.0" -> scale = "2.0"
                "settings put system font_scale 1.0" -> scale = "1.0"
            }
            return ""
        }
        var entered = false
        withScreenDisplay(::shell) { entered = true }
        assertTrue("Exact physical defaults were rejected without overrides", entered)
        assertEquals("1.0", scale)
        assertEquals(ScreenDisplay("1.0", null, null), readScreenDisplay(::shell))
        physicalSize = "321x640"
        val error =
            assertThrows(IllegalStateException::class.java) {
                withScreenDisplay(::shell) { fail("Wrong physical size reached body") }
            }
        assertEquals("Display setup readback did not match", error.message)
    }

    @Test fun actualShellExitIsRequired() {
        fun execute(command: String): String {
            val process = ProcessBuilder("sh").start()
            process.outputStream.use { it.write(screenShellScript(command, "PROOF_EXIT").toByteArray()) }
            val output = process.inputStream.bufferedReader().use { it.readText() }
            assertEquals(0, process.waitFor())
            return screenShellOutput(output, "PROOF_EXIT")
        }
        assertEquals("result", execute("printf result"))
        assertThrows(IllegalStateException::class.java) { execute("false") }
        assertThrows(IllegalStateException::class.java) { screenShellOutput("result", "PROOF_EXIT") }
        assertThrows(IllegalStateException::class.java) { screenShellOutput("result\nPROOF_EXIT=bad", "PROOF_EXIT") }
    }

    @Test fun readbackAndEveryRestorePreserveTheOriginalFailure() {
        val original = ScreenDisplay("1.0", null, null)
        var state = original
        var failRestore = false
        val attempts = mutableListOf<String>()
        val restore = screenDisplayRestoreCommands(original.scale, original.size, original.density)

        fun shell(command: String): String {
            if (failRestore && command in restore) {
                attempts += command
                if (command == restore.first()) error("restore failed")
            }
            when (command) {
                "settings get system font_scale" -> return state.scale
                "wm size" -> return state.size?.let { "Override size: $it" } ?: "Physical size: 800x1200"
                "wm density" -> return state.density?.let { "Override density: $it" } ?: "Physical density: 320"
                "wm size 320x640" -> state = state.copy(size = "320x640")
                "wm density 160" -> state = state.copy(density = "160")
                "settings put system font_scale 2.0" -> state = state.copy(scale = "2.0")
                restore[0] -> state = state.copy(size = original.size)
                restore[1] -> state = state.copy(density = original.density)
                restore[2] -> state = state.copy(scale = original.scale)
            }
            return ""
        }
        withScreenDisplay(::shell) { assertEquals(ScreenDisplay("2.0", "320x640", "160"), state) }
        assertEquals(original, state)
        assertThrows(IllegalStateException::class.java) {
            withScreenDisplay({ command ->
                if (command ==
                    "wm size 320x640"
                ) {
                    ""
                } else {
                    shell(command)
                }
            }) { fail("Wrong readback reached body") }
        }
        assertEquals(original, state)
        val primary = AssertionError("body failed")
        val caught =
            assertThrows(AssertionError::class.java) {
                withScreenDisplay(::shell) {
                    failRestore = true
                    throw primary
                }
            }
        assertSame(primary, caught)
        assertEquals(restore, attempts)
        assertTrue(caught.suppressed.any { it.message == "restore failed" })
        assertTrue(
            caught.suppressed.any { error ->
                error.suppressed.any {
                    it.message ==
                        "Display restore readback did not match"
                }
            },
        )
    }

    @Test fun unsetFontKeyIsRestoredAfterDisplayNormalization() {
        val original = ScreenDisplay("null", null, null)
        var state = original

        fun shell(command: String): String {
            when (command) {
                "settings get system font_scale" -> return state.scale
                "wm size" -> return state.size?.let { "Override size: $it" } ?: "Physical size: 320x640"
                "wm density" -> return state.density?.let { "Override density: $it" } ?: "Physical density: 160"
                "wm size 320x640" -> state = state.copy(size = "320x640")
                "wm density 160" -> state = state.copy(density = "160")
                "settings put system font_scale 2.0" -> state = state.copy(scale = "2.0")
                "wm size reset" -> state = state.copy(size = null, scale = "1.0")
                "wm density reset" -> state = state.copy(density = null, scale = "1.0")
                "settings delete system font_scale" -> state = state.copy(scale = "null")
            }
            return ""
        }
        withScreenDisplay(::shell) {}
        assertEquals("Display normalization replaced the original unset font key", original, state)
    }

    @Test fun lateFontWritesRequireExactQuietStateAndHaveABound() {
        val original = ScreenDisplay("null", null, null)
        var state = original
        var time = 0L
        var fontWrites = 0
        var alwaysNormalize = false

        fun shell(command: String): String {
            when (command) {
                "settings get system font_scale" -> {
                    return state.scale
                }

                "wm size" -> {
                    return "Physical size: 320x640"
                }

                "wm density" -> {
                    return "Physical density: 160"
                }

                "settings delete system font_scale" -> {
                    state = state.copy(scale = "null")
                    fontWrites++
                }
            }
            return ""
        }
        val pause = {
            time += 50
            check(time <= 5050) { "Display restore deadline omitted" }
            if (time == 50L || time == 100L || alwaysNormalize) state = state.copy(scale = "1.0")
        }
        restoreScreenDisplay(::shell, original, pause, { time })
        assertEquals(original, state)
        assertEquals(3, fontWrites)
        assertTrue("Late writes passed without a quiet interval", time >= 350)
        alwaysNormalize = true
        time = 0
        val error =
            assertThrows(IllegalStateException::class.java) {
                restoreScreenDisplay(::shell, original, pause, { time })
            }
        assertEquals("Display restore did not settle to the exact saved state", error.message)
        assertEquals(5000L, time)
    }
}
