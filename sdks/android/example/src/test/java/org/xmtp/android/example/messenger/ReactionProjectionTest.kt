package org.xmtp.android.example.messenger
import org.junit.Assert.*
import org.junit.Test
import uniffi.xmtp_sdk.*

class ReactionProjectionTest {
    @Test fun equalTimeActionsKeepSdkOrderWhenIdsRunInTheOppositeDirection() {
        fun row(
            id: String,
            sender: String,
            action: ReactionAction,
            time: Long = 100,
        ) = ReactionMessage(
            id.repeat(64),
            sender,
            Timestamp(time),
            DeliveryStatus.PUBLISHED,
            Reaction("👍", action, ReactionSchema.UNICODE),
        )
        val removed =
            listOf(
                row("f", "me", ReactionAction.ADDED),
                row("1", "me", ReactionAction.REMOVED),
            )
        assertTrue("A tied removal must not be reversed by its ID", reactionRows(removed, "me").isEmpty())
        val other = removed + row("2", "other", ReactionAction.ADDED)
        assertEquals(
            listOf(
                org.xmtp.android.example.shared
                    .ReactionUi("👍", 1, false),
            ),
            reactionRows(other, "me"),
        )
        val restored = other + row("0", "me", ReactionAction.ADDED)
        assertEquals(
            listOf(
                org.xmtp.android.example.shared
                    .ReactionUi("👍", 2, true),
            ),
            reactionRows(restored, "me"),
        )
        val otherRemoved = restored + row("e", "other", ReactionAction.REMOVED)
        assertEquals(
            listOf(
                org.xmtp.android.example.shared
                    .ReactionUi("👍", 1, true),
            ),
            reactionRows(otherRemoved, "me"),
        )
        val oldRemoval = otherRemoved + row("8", "me", ReactionAction.REMOVED, time = 90)
        assertEquals(
            "Sent time still orders records from different times",
            listOf(
                org.xmtp.android.example.shared
                    .ReactionUi("👍", 1, true),
            ),
            reactionRows(oldRemoval, "me"),
        )
    }

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
