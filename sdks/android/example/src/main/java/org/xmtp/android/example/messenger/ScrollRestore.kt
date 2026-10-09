package org.xmtp.android.example.messenger
import org.xmtp.android.example.shared.MessageRow
import org.xmtp.android.example.shared.ScrollAnchor

data class RestoredPosition(
    val anchor: ScrollAnchor?,
    val changed: Boolean,
)

private fun timeDistance(
    a: Long,
    b: Long,
): ULong =
    if (a >= b) {
        a
            .toULong() -
            b
                .toULong()
    } else {
        b
            .toULong() -
            a
                .toULong()
    }

fun restoreAnchor(
    saved: ScrollAnchor,
    rows: List<MessageRow>,
): RestoredPosition {
    val matching =
        rows.firstOrNull {
            it.id ==
                saved.messageId
        }
    if (matching != null) {
        return RestoredPosition(
            saved,
            false,
        )
    }
    val closest =
        rows.minByOrNull {
            timeDistance(
                it.sentAtNs,
                saved.sentAtNs,
            )
        }
    return RestoredPosition(
        closest?.let {
            saved
                .copy(
                    messageId =
                        it.id,
                    sentAtNs =
                        it.sentAtNs,
                    offsetPx = 0,
                    wasAtNewest = false,
                )
        },
        true,
    )
}
