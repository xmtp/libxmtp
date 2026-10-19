internal import Expo
import React
import ReactAppDependencyProvider

@main
class AppDelegate: ExpoAppDelegate, ExpoReactNativeFactoryProvider {
    var window: UIWindow?

    var reactNativeDelegate: ExpoReactNativeFactoryDelegate?
    var reactNativeFactory: RCTReactNativeFactory?

    override func application(
        _ application: UIApplication,
        didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]? = nil
    ) -> Bool {
        let delegate = ReactNativeDelegate()
        let factory = ExpoReactNativeFactory(delegate: delegate)
        delegate.dependencyProvider = RCTAppDependencyProvider()

        reactNativeDelegate = delegate
        reactNativeFactory = factory

        // The window is created and React Native is started by `SceneDelegate` under the
        // scene-based life cycle (required by the iOS 27 SDK).
        return super.application(application, didFinishLaunchingWithOptions: launchOptions)
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
            let settings = RCTBundleURLProvider.sharedSettings()
            // Each checkout runs its own Metro. The build stamps its port (RCT_METRO_PORT)
            // into Info.plist. Build the URL directly: the shared settings fall back to
            // port 8081, which can be another checkout's Metro.
            guard let port = Bundle.main.object(forInfoDictionaryKey: "XMTPMetroPort") as? String, !port.isEmpty else {
                return settings.jsBundleURL(forBundleRoot: ".expo/.virtual-metro-entry")
            }
            let location = "localhost:\(port)"
            // The packager connection (reload, fast refresh) reads the same location.
            settings.jsLocation = location
            return RCTBundleURLProvider.jsBundleURL(
                forBundleRoot: ".expo/.virtual-metro-entry",
                packagerHost: location,
                enableDev: settings.enableDev,
                enableMinification: settings.enableMinification,
                inlineSourceMap: settings.inlineSourceMap
            )
        #else
            return Bundle.main.url(forResource: "main", withExtension: "jsbundle")
        #endif
    }
}
