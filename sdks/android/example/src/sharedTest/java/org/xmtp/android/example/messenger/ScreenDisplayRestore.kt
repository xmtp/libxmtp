package org.xmtp.android.example.messenger

internal data class ScreenDisplay(
    val scale: String,
    val size: String?,
    val density: String?,
)

internal fun screenShellScript(
    command: String,
    marker: String,
): String {
    require(Regex("[A-Za-z0-9_]+").matches(marker))
    return "$command 2>&1\nxmtp_status=\$?\nprintf '\\n$marker=%s\\n' \"\$xmtp_status\"\n"
}

internal fun screenShellOutput(
    raw: String,
    marker: String,
): String {
    val position = raw.lastIndexOf("\n$marker=")
    check(position >= 0) { "Missing screen shell exit status" }
    val status = raw.substring(position + marker.length + 2).trim().toIntOrNull()
    check(status != null && status in 0..255) { "Invalid screen shell exit status" }
    check(status == 0) { "Screen shell command failed with exit $status" }
    return raw.substring(0, position).trim()
}

internal fun readScreenDisplay(
    shell: (String) -> String,
    effective: Boolean = false,
): ScreenDisplay {
    val rawScale = shell("settings get system font_scale")

    fun overrideValue(
        command: String,
        label: String,
    ): String? {
        val lines = shell(command).lineSequence().toList()
        val selected =
            lines.firstOrNull { it.startsWith(label) }
                ?: if (effective) lines.firstOrNull { it.startsWith(label.replace("Override", "Physical")) } else null
        return selected?.substringAfter(':')?.trim()
    }
    val size = overrideValue("wm size", "Override size:")
    val density = overrideValue("wm density", "Override density:")
    screenDisplayRestoreCommands(rawScale, size, density)
    return ScreenDisplay(if (rawScale == "null") rawScale else rawScale.toFloat().toString(), size, density)
}

internal fun withScreenDisplay(
    shell: (String) -> String,
    pause: () -> Unit = { Thread.sleep(50) },
    clock: () -> Long = { System.nanoTime() / 1_000_000 },
    body: () -> Unit,
) {
    val previous = readScreenDisplay(shell)
    var failure: Throwable? = null

    fun attempt(step: () -> Unit) {
        try {
            step()
        } catch (error: Throwable) {
            if (failure == null) {
                failure = error
            } else if (failure !== error) {
                failure?.addSuppressed(error)
            }
        }
    }
    try {
        shell("wm size 320x640")
        shell("wm density 160")
        shell("settings put system font_scale 2.0")
        check(
            readScreenDisplay(shell, effective = true) == ScreenDisplay("2.0", "320x640", "160"),
        ) { "Display setup readback did not match" }
        body()
    } catch (error: Throwable) {
        failure = error
    } finally {
        attempt { restoreScreenDisplay(shell, previous, pause, clock) }
    }
    failure?.let { throw it }
}

internal fun restoreScreenDisplay(
    shell: (String) -> String,
    previous: ScreenDisplay,
    pause: () -> Unit = { Thread.sleep(50) },
    clock: () -> Long = { System.nanoTime() / 1_000_000 },
) {
    val restore = screenDisplayRestoreCommands(previous.scale, previous.size, previous.density)
    var failure: Throwable? = null
    for (command in restore) {
        try {
            shell(command)
        } catch (error: Throwable) {
            if (failure == null) {
                failure = error
            } else if (failure !== error) {
                checkNotNull(failure).addSuppressed(error)
            }
        }
    }
    if (failure != null) {
        val first = checkNotNull(failure)
        try {
            check(readScreenDisplay(shell) == previous) { "Display restore readback did not match" }
        } catch (
            error: Throwable,
        ) {
            if (first !== error) first.addSuppressed(error)
        }
        throw first
    }
    val deadline = clock() + 5_000
    var stableSince: Long? = null
    while (clock() < deadline) {
        val current = readScreenDisplay(shell)
        check(
            current.size == previous.size && current.density == previous.density,
        ) { "Display restore readback did not match" }
        if (current.scale == previous.scale) {
            if (stableSince == null) stableSince = clock()
            if (clock() - checkNotNull(stableSince) >= 250) return
        } else {
            stableSince = null
            shell(restore.last())
        }
        pause()
    }
    error("Display restore did not settle to the exact saved state")
}

/** Validate captured values before changing the display. Commands use canonical numbers only. */
internal fun screenDisplayRestoreCommands(
    scale: String,
    size: String?,
    density: String?,
): List<String> {
    val fontScale =
        if (scale == "null") {
            null
        } else {
            requireNotNull(scale.toFloatOrNull()) { "Invalid captured font scale" }.also {
                require(it.isFinite() && it > 0f) { "Invalid captured font scale" }
            }
        }

    fun positiveInteger(value: String): Int =
        requireNotNull(value.toIntOrNull()?.takeIf { it > 0 }) { "Invalid captured display integer" }
    val dpi = density?.let(::positiveInteger)
    val dimensions =
        size?.let {
            val match = requireNotNull(Regex("([0-9]+)x([0-9]+)").matchEntire(it)) { "Invalid captured display size" }
            "${positiveInteger(match.groupValues[1])}x${positiveInteger(match.groupValues[2])}"
        }
    return listOf(
        if (dimensions == null) "wm size reset" else "wm size $dimensions",
        if (dpi == null) "wm density reset" else "wm density $dpi",
        if (fontScale == null) "settings delete system font_scale" else "settings put system font_scale $fontScale",
    )
}
