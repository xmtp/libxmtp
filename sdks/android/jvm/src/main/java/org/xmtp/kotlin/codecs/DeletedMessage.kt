package org.xmtp.kotlin.codecs

import org.xmtp.kotlin.InboxId

/**
 * Represents a message that has been deleted.
 */
data class DeletedMessage(
    val deletedBy: DeletedBy,
)

/**
 * Indicates who deleted the message.
 */
sealed class DeletedBy {
    /** The original sender deleted their own message */
    object Sender : DeletedBy()

    /** An admin deleted the message */
    data class Admin(
        val inboxId: InboxId,
    ) : DeletedBy()
}
