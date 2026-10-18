import ExpoModulesCore
import UIKit

/// Sets the hosting screen's `navigationItem.subtitle` and `largeSubtitle`
/// (iOS 26). react-native-screens does not set these properties, so the values
/// stay until this view leaves the screen. The view draws nothing.
public final class NavigationSubtitleView: ExpoView {
    var subtitle: String? {
        didSet { apply() }
    }

    var largeSubtitle: String? {
        didSet { apply() }
    }

    private weak var appliedItem: UINavigationItem?

    public required init(appContext: AppContext? = nil) {
        super.init(appContext: appContext)
        isUserInteractionEnabled = false
    }

    override public func didMoveToWindow() {
        super.didMoveToWindow()
        apply()
    }

    override public func willMove(toSuperview newSuperview: UIView?) {
        super.willMove(toSuperview: newSuperview)
        if newSuperview == nil {
            clear()
        }
    }

    private func apply() {
        guard window != nil, let controller = hostingViewController() else { return }
        let item = controller.navigationItem
        if let previous = appliedItem, previous !== item {
            previous.subtitle = nil
            previous.largeSubtitle = nil
        }
        item.subtitle = subtitle
        // A nil large subtitle shows `subtitle` under the large title.
        item.largeSubtitle = largeSubtitle
        appliedItem = item
    }

    private func clear() {
        appliedItem?.subtitle = nil
        appliedItem?.largeSubtitle = nil
        appliedItem = nil
    }

    /// The first view controller up the responder chain: the screen's controller.
    private func hostingViewController() -> UIViewController? {
        var responder: UIResponder? = next
        while let current = responder {
            if let controller = current as? UIViewController {
                return controller
            }
            responder = current.next
        }
        return nil
    }
}
