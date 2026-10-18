import React
import React_RCTAppDelegate
import ReactAppDependencyProvider
import UIKit

@main
class AppDelegate: UIResponder, UIApplicationDelegate {
    var backgroundDelegate: SceneDelegate?
    var backgroundFactory: RCTReactNativeFactory?
    var backgroundRootView: UIView?
    var backgroundTask: UIBackgroundTaskIdentifier = .invalid
    var backgroundObserver: NSObjectProtocol?
    var requiresBackgroundInitialization: Bool {
        ProcessInfo.processInfo.arguments.contains("--xmtp-background")
    }

    func application(_ application: UIApplication, didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]? = nil) -> Bool {
        if requiresBackgroundInitialization {
            waitForBackgroundInitialization(application, launchOptions: launchOptions)
        }
        return true
    }

    private func waitForBackgroundInitialization(_ application: UIApplication, launchOptions: [UIApplication.LaunchOptionsKey: Any]?) {
        let arguments = ProcessInfo.processInfo.arguments
        guard let marker = arguments.firstIndex(of: "--xmtp-background"), marker + 1 < arguments.count,
              let components = URLComponents(string: arguments[marker + 1]),
              components.scheme == "xmtprn", components.host == "run",
              components.queryItems?.contains(where: { $0.name == "mode" && $0.value == "background" }) == true
        else {
            fatalError("Background fixture requires its authenticated launch URL")
        }
        // No React factory, runtime, native SDK module, or SDK observer exists yet.
        // Wait for a real transition; didFinishLaunching can briefly report Background
        // even when the OS is about to activate the initial scene.
        backgroundObserver = NotificationCenter.default.addObserver(forName: UIApplication.didEnterBackgroundNotification, object: application, queue: .main) { [weak self] _ in
            guard let self, application.applicationState == .background, self.backgroundRootView == nil else { return }
            if let observer = self.backgroundObserver {
                NotificationCenter.default.removeObserver(observer)
                self.backgroundObserver = nil
            }
            // Let notification delivery finish before any SDK observer can exist.
            DispatchQueue.main.async { [weak self] in
                guard let self, application.applicationState == .background, self.backgroundRootView == nil else { return }
                UserDefaults.standard.set(application.applicationState.rawValue, forKey: "XmtpFixtureFirstReactInitializationState")
                self.backgroundTask = application.beginBackgroundTask(withName: "XMTP native lifecycle fixture") { [weak self] in
                    self?.finishBackgroundTask()
                }
                let delegate = SceneDelegate()
                delegate.dependencyProvider = RCTAppDependencyProvider()
                let factory = RCTReactNativeFactory(delegate: delegate)
                self.backgroundDelegate = delegate
                self.backgroundFactory = factory
                self.backgroundRootView = factory.rootViewFactory.view(withModuleName: "XmtpConsumerBare", initialProperties: nil, launchOptions: launchOptions)
                NSLog("XMTP first React root initialized in actual UIKit Background, pid=%d", ProcessInfo.processInfo.processIdentifier)
            }
        }
    }

    func finishBackgroundTask() {
        if backgroundTask != .invalid {
            UIApplication.shared.endBackgroundTask(backgroundTask)
            backgroundTask = .invalid
        }
    }
}
