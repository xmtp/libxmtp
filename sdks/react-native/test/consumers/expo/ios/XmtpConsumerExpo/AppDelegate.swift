internal import Expo
import React
import ReactAppDependencyProvider

@main
class AppDelegate: ExpoAppDelegate, ExpoReactNativeFactoryProvider {
    var window: UIWindow?
    var backgroundRootView: UIView?
    var backgroundTask: UIBackgroundTaskIdentifier = .invalid
    var backgroundObserver: NSObjectProtocol?
    var reactNativeDelegate: ExpoReactNativeFactoryDelegate?
    var reactNativeFactory: RCTReactNativeFactory?
    var requiresBackgroundInitialization: Bool {
        ProcessInfo.processInfo.arguments.contains("--xmtp-background")
    }

    override func application(_ application: UIApplication, didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]? = nil) -> Bool {
        if requiresBackgroundInitialization {
            waitForBackgroundInitialization(application, launchOptions: launchOptions)
        } else {
            let delegate = ReactNativeDelegate()
            delegate.dependencyProvider = RCTAppDependencyProvider()
            reactNativeDelegate = delegate
            reactNativeFactory = ExpoReactNativeFactory(delegate: delegate)
        }
        // Normal launches are started by SceneDelegate. The background fixture
        // leaves React entirely uninitialized until UIKit enters Background.
        return super.application(application, didFinishLaunchingWithOptions: launchOptions)
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
                let delegate = ReactNativeDelegate()
                delegate.dependencyProvider = RCTAppDependencyProvider()
                let factory = ExpoReactNativeFactory(delegate: delegate)
                self.reactNativeDelegate = delegate
                self.reactNativeFactory = factory
                self.backgroundRootView = factory.rootViewFactory.view(withModuleName: "main", initialProperties: nil, launchOptions: launchOptions)
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

class ReactNativeDelegate: ExpoReactNativeFactoryDelegate {
    // Extension point for config-plugins

    override func sourceURL(for bridge: RCTBridge) -> URL? {
        // needed to return the correct URL for expo-dev-client.
        bridge.bundleURL ?? bundleURL()
    }

    override func bundleURL() -> URL? {
        #if DEBUG
            return RCTBundleURLProvider.sharedSettings().jsBundleURL(forBundleRoot: ".expo/.virtual-metro-entry")
        #else
            return Bundle.main.url(forResource: "main", withExtension: "jsbundle")
        #endif
    }
}
