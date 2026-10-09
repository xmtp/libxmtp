package org.xmtp.android.example.messenger
import org.junit.Assert.*
import org.junit.Test
import uniffi.xmtp_sdk.*

class ReactionProjectionTest {
    @Test fun removalAppliesOnlyToTheSameSendersEmojiAndLaterAddCanRestoreIt() {
        fun row(
            id: String,
            sender: String,
            time: Long,
            action: ReactionAction,
        ) = ReactionMessage(
            id,
            sender,
            Timestamp(time),
            DeliveryStatus.PUBLISHED,
            Reaction(
                "👍",
                action,
                ReactionSchema.UNICODE,
            ),
        )
        val rows =
            listOf(
                row(
                    "a",
                    "me",
                    10,
                    ReactionAction.ADDED,
                ),
                row(
                    "b",
                    "other",
                    15,
                    ReactionAction.ADDED,
                ),
                row(
                    "c",
                    "me",
                    20,
                    ReactionAction.REMOVED,
                ),
            )
        val removed =
            reactionRows(
                rows,
                "me",
            ).single()
        assertEquals(
            1,
            removed.count,
        )
        assertFalse(
            removed.mine,
        )
        val restored =
            reactionRows(
                rows +
                    row(
                        "d",
                        "me",
                        30,
                        ReactionAction.ADDED,
                    ),
                "me",
            ).single()
        assertEquals(
            2,
            restored.count,
        )
        assertTrue(
            restored.mine,
        )
    }
}
