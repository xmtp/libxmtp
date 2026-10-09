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
    ) = ImageVector
        .Builder(name, 24.dp, 24.dp, 24f, 24f)
        .addPath(
            pathData = addPathNodes(path),
            stroke = SolidColor(Color.Black),
            strokeLineWidth = 2f,
            strokeLineCap = StrokeCap.Round,
            strokeLineJoin = StrokeJoin.Round,
        ).build()

    val Back = outline("Back", "M15,5 L8,12 L15,19")
    val Add = outline("Add", "M12,5 L12,19 M5,12 L19,12")
    val Close = outline("Close", "M6,6 L18,18 M18,6 L6,18")
    val More = outline("More", "M5,11 L5,13 M12,11 L12,13 M19,11 L19,13")
    val Settings =
        outline(
            "Settings",
            "M9,3 L15,3 L16,6 L19,7 L21,12 L19,17 L16,18 L15,21 L9,21 L8,18 L5,17 L3,12 L5,7 L8,6 Z M15,12 A3,3 0,1 1,9,12 A3,3 0,1 1,15,12",
        )
}
