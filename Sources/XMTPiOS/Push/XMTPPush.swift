//
//  XMTPPush.swift
//
//
//  Created by Pat Nakajima on 1/20/23.
//

import Connect

import UserNotifications

public typealias NotificationSubscription = Notifications_V1_Subscription
public typealias NotificationSubscriptionHmacKey = Notifications_V1_Subscription.HmacKey

enum XMTPPushError: Error {
	case noPushServer
}

#if canImport(UIKit)
import UIKit

public struct XMTPPush {
	public static var shared = XMTPPush()

	var installationID: String
	var installationIDKey: String = "installationID"

	var pushServer: String = ""

	private init() {
		if let id = UserDefaults.standard.string(forKey: installationIDKey) {
			installationID = id
		} else {
			installationID = UUID().uuidString
			UserDefaults.standard.set(installationID, forKey: installationIDKey)
		}
	}

	public mutating func setPushServer(_ server: String) {
		pushServer = server
	}

	public func request() async throws -> Bool {
		if pushServer == "" {
			throw XMTPPushError.noPushServer
		}

		if try await UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .badge]) {
//				await UIApplication.shared.registerForRemoteNotifications()

			return true
		}

		return false
	}

	public func register(token: String) async throws {
		if pushServer == "" {
			throw XMTPPushError.noPushServer
		}

		let request = Notifications_V1_RegisterInstallationRequest.with { request in
			request.installationID = installationID
			request.deliveryMechanism = Notifications_V1_DeliveryMechanism.with { delivery in
				delivery.apnsDeviceToken = token
				delivery.deliveryMechanismType = .apnsDeviceToken(token)
			}
		}

		_ = await client.registerInstallation(request: request)
	}

	public func subscribe(topics: [String]) async throws {
		if pushServer == "" {
			throw XMTPPushError.noPushServer
		}

		let request = Notifications_V1_SubscribeRequest.with { request in
			request.installationID = installationID
			request.topics = topics
		}

		_ = await client.subscribe(request: request)
	}
	
	public func subscribeWithMetadata(subscriptions: [NotificationSubscription]) async throws {
		if pushServer == "" {
			throw XMTPPushError.noPushServer
		}

		let request = Notifications_V1_SubscribeWithMetadataRequest.with { request in
			request.installationID = installationID
			request.subscriptions = subscriptions
		}

		_ = await client.subscribeWithMetadata(request: request)
	}
	
	public func unsubscribe(topics: [String]) async throws {
		if pushServer == "" {
			throw XMTPPushError.noPushServer
		}

		let request = Notifications_V1_UnsubscribeRequest.with { request in
			request.installationID = installationID
			request.topics = topics
		}

		_ = await client.unsubscribe(request: request)
	}

	var client: Notifications_V1_NotificationsClient {
		let protocolClient = ProtocolClient(
			httpClient: URLSessionHTTPClient(),
			config: ProtocolClientConfig(
				host: pushServer,
				networkProtocol: .connect,
				codec: ProtoCodec()
			)
		)

		return Notifications_V1_NotificationsClient(client: protocolClient)
	}
}
#else
public struct XMTPPush {
	public static var shared = XMTPPush()
	private init() {
		fatalError("XMTPPush not available")
	}

	public mutating func setPushServer(_: String) {
		fatalError("XMTPPush not available")
	}

	public func request() async throws -> Bool {
		fatalError("XMTPPush not available")
	}

	public func register(token _: String) async throws {
		fatalError("XMTPPush not available")
	}

	public func subscribe(topics _: [String]) async throws {
		fatalError("XMTPPush not available")
	}
	
	public func subscribeWithMetadata(subscriptions _: [NotificationSubscription]) async throws {
		fatalError("XMTPPush not available")
	}
	
	public func unsubscribe(topics _: [String]) async throws {
		fatalError("XMTPPush not available")
	}

	var client: Notifications_V1_NotificationsClient {
		fatalError("XMTPPush not available")
	}
}
#endif
