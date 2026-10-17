package org.xmtp.android.example.shared
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.scale
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ColorFilter
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.addPathNodes
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import org.xmtp.android.example.shared.designsystem.LocalReducedMotion
import org.xmtp.android.example.shared.designsystem.MessengerMotion

internal val ink: Color @Composable get() = MaterialTheme.colorScheme.onSurface
internal val blue: Color @Composable get() = MaterialTheme.colorScheme.primary
internal val outgoing: Color @Composable get() = MaterialTheme.colorScheme.primaryContainer
internal val xmtpLogo =
    ImageVector
        .Builder(
            "XMTP",
            64.dp,
            64.dp,
            64f,
            64f,
        ).addPath(
            pathData =
                addPathNodes(
                    "M0,32C0,14.327 14.327,0 32,0" +
                        "C49.662,0 63.307,14.061 63.723,31.861" +
                        "C63.723,37.541 61.784,42.32 56.935,46.822" +
                        "C52.838,50.627 45.853,51.186 40.728,48.346" +
                        "C37.07,46.236 34.25,41.742 31.861,38.442" +
                        "L27.429,45.229H17.87L26.875,31.861L18.147,18.701H27.983L31.931,25.489" +
                        "L35.81,18.701H45.714L36.71,31.861" +
                        "C36.71,31.861 41.004,38.442 43.359,41.004" +
                        "C45.714,43.567 50.009,43.636 52.779,40.866" +
                        "C55.825,37.82 56.507,35.394 56.52,31.861" +
                        "C56.567,18.188 45.706,7.203 32,7.203" +
                        "C18.305,7.203 7.203,18.305 7.203,32" +
                        "C7.203,45.695 18.305,56.797 32,56.797" +
                        "C33.894,56.797 35.71,56.637 37.472,56.242" +
                        "L39.134,63.238C36.627,63.8 34.463,64 32,64" +
                        "C14.327,64 0,49.673 0,32Z",
                ),
            fill = SolidColor(Color(0xFF182235)),
        ).build()

/** The logo row at the top of the setup screens, with the space that follows it. */
@Composable internal fun XmtpBrand() {
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        Image(
            xmtpLogo,
            "XMTP",
            Modifier.size(48.dp),
            colorFilter = ColorFilter.tint(MaterialTheme.colorScheme.primary),
        )
        Text("XMTP", style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.Bold)
    }
    Spacer(Modifier.height(12.dp))
}

@Composable internal fun CircleAction(
    label: String,
    icon: ImageVector,
    click: () -> Unit,
) {
    val interactions = remember { MutableInteractionSource() }
    val pressed by interactions.collectIsPressedAsState()
    val scale by animateFloatAsState(
        if (pressed &&
            !LocalReducedMotion.current
        ) {
            0.98f
        } else {
            1f
        },
        tween(MessengerMotion.PRESS),
        label = "Press",
    )
    FilledTonalButton(
        onClick = click,
        interactionSource = interactions,
        modifier = Modifier.size(48.dp).scale(scale).semantics { contentDescription = label },
        shape = CircleShape,
        colors =
            ButtonDefaults.filledTonalButtonColors(
                containerColor = MaterialTheme.colorScheme.surfaceVariant,
                contentColor = ink,
            ),
        contentPadding = PaddingValues(12.dp),
    ) { Icon(icon, contentDescription = null, modifier = Modifier.size(24.dp)) }
}

@Composable internal fun Notice(
    text: String,
    button: String? = null,
    click: () -> Unit = {
    },
) {
    Row(
        Modifier
            .fillMaxWidth()
            .background(MaterialTheme.colorScheme.primaryContainer)
            .semantics { liveRegion = LiveRegionMode.Polite }
            .padding(
                12.dp,
            ),
        verticalAlignment =
            Alignment.CenterVertically,
    ) {
        Text(
            text,
            Modifier
                .weight(1f),
        )
        if (button != null) {
            TextButton(
                click,
                Modifier
                    .heightIn(
                        min =
                            48.dp,
                    ),
            ) {
                Text(button)
            }
        }
    }
}

@Composable internal fun Action(
    label: String,
    enabled: Boolean = true,
    click: () -> Unit,
) {
    Button(
        click,
        Modifier
            .fillMaxWidth()
            .heightIn(
                min =
                    48.dp,
            ),
        enabled = enabled,
    ) {
        Text(label)
    }
}

@Composable
internal fun MessengerAvatar(
    title: String,
    modifier: Modifier = Modifier,
    group: Boolean = false,
    size: Dp = 48.dp,
) {
    val initials =
        title
            .trim()
            .removePrefix("@")
            .split(Regex("\\s+"))
            .filter {
                it.isNotEmpty()
            }.take(2)
            .joinToString("") { it.take(1) }
            .uppercase()
            .ifEmpty { if (group) "G" else "?" }
    Surface(
        modifier.size(size).clearAndSetSemantics {
        },
        shape = CircleShape,
        color = MaterialTheme.colorScheme.primaryContainer,
    ) {
        Box(contentAlignment = Alignment.Center) {
            Text(
                initials,
                fontSize =
                    with(LocalDensity.current) {
                        (16 / fontScale).sp
                    },
                color = MaterialTheme.colorScheme.onPrimaryContainer,
                style = MaterialTheme.typography.titleMedium,
                fontWeight = FontWeight.SemiBold,
                maxLines = 1,
            )
        }
    }
}
