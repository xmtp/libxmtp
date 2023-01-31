//
//  NotificationService.swift
//  NotificationService
//
//  Created by Pat Nakajima on 1/20/23.
//

import UserNotifications
import XMTP

class NotificationService: UNNotificationServiceExtension {
	var contentHandler: ((UNNotificationContent) -> Void)?
	var bestAttemptContent: UNMutableNotificationContent?

	override func didReceive(_ request: UNNotificationRequest, withContentHandler contentHandler: @escaping (UNNotificationContent) -> Void) {
		self.contentHandler = contentHandler
		bestAttemptContent = (request.content.mutableCopy() as? UNMutableNotificationContent)

		do {
			guard let encryptedMessage = request.content.userInfo["encryptedMessage"] as? String,
			      let topic = request.content.userInfo["topic"] as? String,
			      let encryptedMessageData = Data(base64Encoded: Data(encryptedMessage.utf8))
			else {
				print("Did not get correct message data from push")
				return
			}

			let persistence = Persistence()

			// swiftlint:disable no_optional_try
			guard let keysData = persistence.loadKeys(),
			      let keys = try? PrivateKeyBundle(serializedData: keysData),
			      let conversationContainer = try persistence.load(conversationTopic: topic)
			else {
				print("No keys or conversation persisted")
				return
			}

			let client = try Client.from(bundle: keys)
			let conversation = conversationContainer.decode(with: client)

			let envelope = XMTP.Envelope.with { envelope in
				envelope.message = encryptedMessageData
				envelope.contentTopic = topic
			}

			if let bestAttemptContent = bestAttemptContent {
				let decodedMessage = try conversation.decode(envelope)

				bestAttemptContent.body = (try? decodedMessage.content()) ?? "no content"

				contentHandler(bestAttemptContent)
			}

			// swiftlint:enable no_optional_try
		} catch {
			print("Error receiving notification: \(error)")
		}
	}

	override func serviceExtensionTimeWillExpire() {
		// Called just before the extension will be terminated by the system.
		// Use this as an opportunity to deliver your "best attempt" at modified content, otherwise the original push payload will be used.
		if let contentHandler = contentHandler, let bestAttemptContent = bestAttemptContent {
			contentHandler(bestAttemptContent)
		}
	}
}
