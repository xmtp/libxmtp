import ExpoModulesCore
import Foundation

/// Platform services that no published Expo module provides.
public final class MessengerPlatformModule: Module {
    /// The launch variables the app reads. `xcrun simctl launch` passes them as
    /// `SIMCTL_CHILD_<NAME>`, so the start recipes set them per launch.
    private static let launchKeys = [
        "XMTP_BACKEND_URL",
        "XMTP_ALLOW_PRIVATE_ATTACHMENTS",
        "XMTP_UI_TEST_ATTACHMENT",
    ]

    public func definition() -> ModuleDefinition {
        Name("MessengerPlatform")

        Function("launchEnvironment") { () -> [String: String] in
            let environment = ProcessInfo.processInfo.environment
            return Self.launchKeys.reduce(into: [:]) { result, key in
                if let value = environment[key] {
                    result[key] = value
                }
            }
        }

        Function("applicationSupportDirectory") { () throws -> String in
            try FileManager.default.url(
                for: .applicationSupportDirectory,
                in: .userDomainMask,
                appropriateFor: nil,
                create: true
            ).path
        }

        Function("excludeFromBackup") { (path: String) throws in
            var url = URL(fileURLWithPath: path)
            var values = URLResourceValues()
            values.isExcludedFromBackup = true
            try url.setResourceValues(values)
        }

        Function("protectUntilFirstUserAuthentication") { (path: String) throws in
            try FileManager.default.setAttributes(
                [.protectionKey: FileProtectionType.completeUntilFirstUserAuthentication],
                ofItemAtPath: path
            )
        }

        View(NavigationSubtitleView.self) {
            Prop("subtitle") { (view, subtitle: String?) in
                view.subtitle = subtitle
            }
            Prop("largeSubtitle") { (view, largeSubtitle: String?) in
                view.largeSubtitle = largeSubtitle
            }
        }

        View(ContextMenuView.self) {
            Events("onSelect")
            Prop("items") { (view, items: [ContextMenuItem]) in
                view.items = items
            }
            Prop("elevated") { (view, elevated: Bool) in
                view.elevated = elevated
            }
        }
    }
}
