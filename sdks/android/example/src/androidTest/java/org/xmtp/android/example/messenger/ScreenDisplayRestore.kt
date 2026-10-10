package org.xmtp.android.example.messenger

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
        if (fontScale == null) "settings delete system font_scale" else "settings put system font_scale $fontScale",
        if (dpi == null) "wm density reset" else "wm density $dpi",
        if (dimensions == null) "wm size reset" else "wm size $dimensions",
    )
}
