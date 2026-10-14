package org.xmtp.android.example.shared

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.addPathNodes
import androidx.compose.ui.unit.dp

/** All actions use the same 24-unit outline grid. */
object AppIcons {
    private fun outline(
        name: String,
        path: String,
        autoMirror: Boolean = false,
    ) = ImageVector
        .Builder(name, 24.dp, 24.dp, 24f, 24f, autoMirror = autoMirror)
        .addPath(
            pathData = addPathNodes(path),
            stroke = SolidColor(Color.Black),
            strokeLineWidth = 2f,
            strokeLineCap = StrokeCap.Round,
            strokeLineJoin = StrokeJoin.Round,
        ).build()

    val Back = outline("Back", "M15,5 L8,12 L15,19", autoMirror = true)
    val Add = outline("Add", "M12,5 L12,19 M5,12 L19,12")
    val Close = outline("Close", "M6,6 L18,18 M18,6 L6,18")
    val More = outline("More", "M5,11 L5,13 M12,11 L12,13 M19,11 L19,13")
    val Send = outline("Send", "M5,12 L19,12 M13,6 L19,12 L13,18", autoMirror = true)
    val Down = outline("Down", "M6,9 L12,15 L18,9")
    val Search = outline("Search", "M16,10 A6,6 0,1 1,4,10 A6,6 0,1 1,16,10 M15,15 L21,21")
    val Reply = outline("Reply", "M9,5 L3,11 L9,17 M3,11 L14,11 Q20,11 20,19", autoMirror = true)
    val File = outline("File", "M6,3 L14,3 L19,8 L19,21 L6,21 Z M14,3 L14,8 L19,8")
    val Check = outline("Check", "M5,12 L10,17 L19,7")
    val Settings =
        outline(
            "Settings",
            "M9,3 L15,3 L16,6 L19,7 L21,12 L19,17 L16,18 L15,21 " +
                "L9,21 L8,18 L5,17 L3,12 L5,7 L8,6 Z M15,12 A3,3 0,1 1,9,12 A3,3 0,1 1,15,12",
        )
}
