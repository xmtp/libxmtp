import ExampleCore
import UIKit
import UserNotifications

@MainActor final class AppDelegate: NSObject, UIApplicationDelegate, UNUserNotificationCenterDelegate {
	private static var graph: AppGraph?
	static func ensureGraph() throws -> AppGraph {
		if let graph {
			return graph
		}
		try UITestHooks.resetIfRequested()
		let graph = try AppGraph(); self.graph = graph; return graph
	}

	func application(_: UIApplication,
	                 didFinishLaunchingWithOptions _: [UIApplication.LaunchOptionsKey: Any]? = nil) -> Bool
	{
		#if PUSH_ENABLED
			UNUserNotificationCenter.current().delegate = self
		#endif
		do { _ = try Self.ensureGraph() } catch { /* The scene presents the storage failure; push fails closed. */ }
		return true
	}

	#if PUSH_ENABLED
		func application(_: UIApplication, didRegisterForRemoteNotificationsWithDeviceToken deviceToken: Data) {
			PushCoordinator.current?.tokenChanged(deviceToken)
		}

		func application(_: UIApplication, didFailToRegisterForRemoteNotificationsWithError _: Error) {
			PushCoordinator.current?.registrationFailed()
		}

		func application(
			_: UIApplication,
			didReceiveRemoteNotification userInfo: [AnyHashable: Any],
			fetchCompletionHandler completionHandler: @escaping (UIBackgroundFetchResult) -> Void,
		) {
			do {
				let graph = try Self.ensureGraph()
				guard let coordinator = graph.push, let topic = userInfo["topic"] as? String,
				      let sequence = userInfo["sequence_id"] as? String else { completionHandler(.noData); return }
				coordinator.receive(topic: topic, sequence: sequence, completion: completionHandler)
			} catch { completionHandler(.noData) }
		}

		nonisolated func userNotificationCenter(
			_: UNUserNotificationCenter,
			didReceive response: UNNotificationResponse,
		) async {
			let info = response.notification.request.content.userInfo
			guard let profile = info["push-profile"] as? String, let id = UUID(uuidString: profile) else { return }
			let route = PushRoute(profileID: id, conversationID: info["push-conversation"] as? String)
			let coordinator = await MainActor.run {
				do { return try Self.ensureGraph().push } catch { return nil }
			}
			await coordinator?.tap(route)
		}
	#endif
}
