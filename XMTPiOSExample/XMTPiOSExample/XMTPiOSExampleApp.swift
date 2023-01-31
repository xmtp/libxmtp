//
//  XMTPiOSExampleApp.swift
//  XMTPiOSExample
//
//  Created by Pat Nakajima on 11/22/22.
//

import SwiftUI
import XMTP

class AppDelegate: NSObject, UIApplicationDelegate {
	func application(_: UIApplication, didFinishLaunchingWithOptions _: [UIApplication.LaunchOptionsKey: Any]? = nil) -> Bool {
		// Change this to the URL of where your https://github.com/xmtp/example-notification-server-go is hosted
		XMTPPush.shared.setPushServer("YOUR PUSH SERVER HERE")

		return true
	}

	func application(_: UIApplication, didRegisterForRemoteNotificationsWithDeviceToken deviceToken: Data) {
		Task {
			do {
				let deviceTokenString = deviceToken.map { String(format: "%02.2hhx", $0) }.joined()
				try await XMTPPush.shared.register(token: deviceTokenString)
			} catch {
				print("Error registering: \(error)")
			}
		}
	}

	func application(_: UIApplication, didFailToRegisterForRemoteNotificationsWithError error: Error) {
		print("Could not register for remote notifications:")
		print(error.localizedDescription)
	}
}

@main
struct XMTPiOSExampleApp: App {
	@UIApplicationDelegateAdaptor private var appDelegate: AppDelegate

	var body: some Scene {
		WindowGroup {
			ContentView()
		}
	}
}
