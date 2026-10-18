internal import Expo
import UIKit

@objc(SceneDelegate)
class SceneDelegate: ExpoAppSceneDelegate {
    override func scene(_ scene: UIScene, willConnectTo session: UISceneSession, options connectionOptions: UIScene.ConnectionOptions) {
        if let app = UIApplication.shared.delegate as? AppDelegate, app.requiresBackgroundInitialization {
            // Keep Expo from starting React before the app delegate observes Background.
            return
        }
        if let windowScene = scene as? UIWindowScene,
           let app = UIApplication.shared.delegate as? AppDelegate,
           let root = app.backgroundRootView
        {
            let controller = UIViewController()
            controller.view = root
            let window = UIWindow(windowScene: windowScene)
            self.window = window
            app.window = window
            window.rootViewController = controller
            window.makeKeyAndVisible()
            app.finishBackgroundTask()
            return
        }
        super.scene(scene, willConnectTo: session, options: connectionOptions)
    }

    override func sceneDidBecomeActive(_ scene: UIScene) {
        guard let app = UIApplication.shared.delegate as? AppDelegate, app.requiresBackgroundInitialization else {
            super.sceneDidBecomeActive(scene)
            return
        }
        guard let windowScene = scene as? UIWindowScene,
              let root = app.backgroundRootView,
              window == nil
        else { return }
        let controller = UIViewController()
        controller.view = root
        let window = UIWindow(windowScene: windowScene)
        self.window = window
        app.window = window
        window.rootViewController = controller
        window.makeKeyAndVisible()
        app.finishBackgroundTask()
    }
}
