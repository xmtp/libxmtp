package org.xmtp.android.example.shared.app

import androidx.compose.runtime.compositionLocalOf

enum class BackEdge { LEFT, RIGHT }

/** A gesture preview changes display only. The host applies Back after commit. */
data class BackPreview(
    val progress: Float,
    val edge: BackEdge,
)

val LocalBackPreview = compositionLocalOf<BackPreview?> { null }
