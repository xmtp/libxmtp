import React
import React_RCTAppDelegate
import ReactAppDependencyProvider
import UIKit

class SceneDelegate: RCTDefaultReactNativeFactoryDelegate, UIWindowSceneDelegate {
    var window: UIWindow?
    var reactNativeFactory: RCTReactNativeFactory?

    func scene(
        _ scene: UIScene,
        willConnectTo _: UISceneSession,
        options connectionOptions: UIScene.ConnectionOptions
    ) {
        guard let windowScene = scene as? UIWindowScene else {
            return
        }

        if let app = UIApplication.shared.delegate as? AppDelegate, app.requiresBackgroundInitialization {
            // The app delegate starts React only after UIKit really enters Background.
            // The original scene must not initialize a second runtime while suspended.
            return
        }

        if let app = UIApplication.shared.delegate as? AppDelegate, let root = app.backgroundRootView {
            reactNativeFactory = app.backgroundFactory
            let controller = UIViewController()
            controller.view = root
            window = UIWindow(windowScene: windowScene)
            window?.rootViewController = controller
            window?.makeKeyAndVisible()
            app.finishBackgroundTask()
            return
        }
        dependencyProvider = RCTAppDependencyProvider()
        reactNativeFactory = RCTReactNativeFactory(delegate: self)
        window = UIWindow(windowScene: windowScene)

        #if DEBUG
            let devMenuConfiguration = RCTDevMenuConfiguration(
                devMenuEnabled: true,
                shakeGestureEnabled: true,
                keyboardShortcutsEnabled: true
            )
            reactNativeFactory?.devMenuConfiguration = devMenuConfiguration
        #endif

        reactNativeFactory?.startReactNative(
            withModuleName: "XmtpConsumerBare",
            in: window,
            connectionOptions: connectionOptions
        )
    }

    func sceneDidBecomeActive(_ scene: UIScene) {
        guard let windowScene = scene as? UIWindowScene,
              let app = UIApplication.shared.delegate as? AppDelegate,
              app.requiresBackgroundInitialization,
              let root = app.backgroundRootView,
              window == nil
        else { return }
        reactNativeFactory = app.backgroundFactory
        let controller = UIViewController()
        controller.view = root
        window = UIWindow(windowScene: windowScene)
        window?.rootViewController = controller
        window?.makeKeyAndVisible()
        app.finishBackgroundTask()
    }

    func scene(_ scene: UIScene, openURLContexts URLContexts: Set<UIOpenURLContext>) {
        RCTLinkingManager.scene(scene, openURLContexts: URLContexts)
    }

    func scene(_ scene: UIScene, continue userActivity: NSUserActivity) {
        RCTLinkingManager.scene(scene, continue: userActivity)
    }

    override func bundleURL() -> URL? {
        #if DEBUG
            RCTBundleURLProvider.sharedSettings().jsBundleURL(forBundleRoot: "index")
        #else
            Bundle.main.url(forResource: "main", withExtension: "jsbundle")
        #endif
    }
}
