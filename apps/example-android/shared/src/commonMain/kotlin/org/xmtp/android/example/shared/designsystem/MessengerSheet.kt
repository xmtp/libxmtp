package org.xmtp.android.example.shared.designsystem

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties

/** Reduced motion uses an immediate surface with the same content and Back action. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun MessengerSheet(
    onDismiss: () -> Unit,
    visible: Boolean = true,
    onHidden: () -> Unit = {},
    content: @Composable ColumnScope.() -> Unit,
) {
    if (LocalReducedMotion.current) {
        if (!visible) {
            LaunchedEffect(Unit) { onHidden() }
            return
        }
        Dialog(onDismissRequest = onDismiss, properties = DialogProperties(usePlatformDefaultWidth = false)) {
            Box(Modifier.fillMaxSize().safeDrawingPadding()) {
                Surface(
                    Modifier.align(Alignment.BottomCenter).fillMaxWidth(),
                    shape = RoundedCornerShape(topStart = 28.dp, topEnd = 28.dp),
                    color = MaterialTheme.colorScheme.surface,
                ) {
                    Column(Modifier.padding(top = 16.dp), content = content)
                }
            }
        }
    } else {
        val sheet = rememberModalBottomSheetState(skipPartiallyExpanded = true)
        LaunchedEffect(visible) {
            if (visible) {
                sheet.show()
            } else {
                sheet.hide()
                onHidden()
            }
        }
        ModalBottomSheet(
            onDismissRequest = onDismiss,
            sheetState = sheet,
            containerColor = MaterialTheme.colorScheme.surface,
            content = content,
        )
    }
}
