package org.xmtp.android.example.shared.designsystem

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.sp
import org.xmtp.android.example.shared.Appearance

val LocalReducedMotion = staticCompositionLocalOf { false }

object MessengerMotion {
    const val PRESS = 90
    const val ROUTE = 220
    const val ROW = 160
    const val PICKER = 140
    const val REPLY = 180
}

private val Light =
    lightColorScheme(
        primary = Color(0xFF3454BC),
        onPrimary = Color.White,
        primaryContainer = Color(0xFFE9EFFF),
        onPrimaryContainer = Color(0xFF243E91),
        background = Color(0xFFFAFBFF),
        onBackground = Color(0xFF182235),
        surface = Color.White,
        onSurface = Color(0xFF182235),
        surfaceVariant = Color(0xFFEDF0F5),
        onSurfaceVariant = Color(0xFF556276),
        outline = Color(0xFF8490A2),
        outlineVariant = Color(0xFFE7EBF2),
    )
private val Dark =
    darkColorScheme(
        primary = Color(0xFFBAC8FF),
        onPrimary = Color(0xFF182956),
        primaryContainer = Color(0xFF293B68),
        onPrimaryContainer = Color(0xFFDCE4FF),
        background = Color(0xFF11151E),
        onBackground = Color(0xFFEEF1FA),
        surface = Color(0xFF1D2330),
        onSurface = Color(0xFFEEF1FA),
        surfaceVariant = Color(0xFF262E3E),
        onSurfaceVariant = Color(0xFFB5BFD1),
        outline = Color(0xFF8190AC),
        outlineVariant = Color(0xFF303A4D),
    )
private val Type =
    Typography(
        headlineLarge = TextStyle(fontSize = 32.sp, lineHeight = 38.sp, fontWeight = FontWeight.Bold),
        headlineMedium = TextStyle(fontSize = 28.sp, lineHeight = 34.sp, fontWeight = FontWeight.Bold),
        titleLarge = TextStyle(fontSize = 22.sp, lineHeight = 28.sp, fontWeight = FontWeight.SemiBold),
        titleMedium = TextStyle(fontSize = 16.sp, lineHeight = 22.sp, fontWeight = FontWeight.SemiBold),
        bodyLarge = TextStyle(fontSize = 16.sp, lineHeight = 24.sp),
        bodyMedium = TextStyle(fontSize = 14.sp, lineHeight = 21.sp),
        bodySmall = TextStyle(fontSize = 12.sp, lineHeight = 18.sp),
        labelLarge = TextStyle(fontSize = 14.sp, lineHeight = 20.sp, fontWeight = FontWeight.SemiBold),
        labelMedium = TextStyle(fontSize = 12.sp, lineHeight = 16.sp, fontWeight = FontWeight.Medium),
        labelSmall = TextStyle(fontSize = 11.sp, lineHeight = 16.sp, fontWeight = FontWeight.Medium),
    )

@Composable
fun MessengerTheme(
    appearance: Appearance,
    reducedMotion: Boolean,
    content: @Composable () -> Unit,
) {
    val dark =
        when (appearance) {
            Appearance.SYSTEM -> isSystemInDarkTheme()
            Appearance.LIGHT -> false
            Appearance.DARK -> true
        }
    CompositionLocalProvider(LocalReducedMotion provides reducedMotion) {
        MaterialTheme(colorScheme = if (dark) Dark else Light, typography = Type, content = content)
    }
}
