// Generated from the MessageData record. Do not edit this output.
import Foundation

/// The fields of a received message. `Message` adds its decoded content and actions.
public extension Message {
    var `id`: MessageId {
        data.`id`
    }

    var `deliveryCursor`: String? {
        data.`deliveryCursor`
    }

    var `conversationId`: ConversationId {
        data.`conversationId`
    }

    var `topic`: String {
        data.`topic`
    }

    var `senderInboxId`: InboxId {
        data.`senderInboxId`
    }

    var `sentAt`: Timestamp {
        data.`sentAt`
    }

    var `insertedAt`: Timestamp {
        data.`insertedAt`
    }

    var `expiresAt`: Timestamp? {
        data.`expiresAt`
    }

    var `kind`: MessageKind {
        data.`kind`
    }

    var `deliveryStatus`: DeliveryStatus {
        data.`deliveryStatus`
    }

    var `rawBytes`: Data {
        data.`rawBytes`
    }

    var `contentType`: ContentTypeId? {
        data.`contentType`
    }

    var `fallback`: String? {
        data.`fallback`
    }

    var `encoded`: EncodedContent? {
        data.`encoded`
    }

    var `replyCount`: UInt64 {
        data.`replyCount`
    }

    var `reactions`: [ReactionMessage] {
        data.`reactions`
    }

    var `inReplyTo`: ReplyParent? {
        data.`inReplyTo`
    }
}
