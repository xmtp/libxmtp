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
        // The large subtitle's font follows Dynamic Type.
        registerForTraitChanges([UITraitPreferredContentSizeCategory.self]) {
            (self: Self, _: UITraitCollection) in
            self.apply()
        }
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
        // A pushed or replaced screen lays out its large title during the
        // transition. A subtitle that changes then leaves the large title
        // collapsed, so apply it once the transition ends.
        if let coordinator = controller.transitionCoordinator {
            coordinator.animate(alongsideTransition: nil) { [weak self] _ in
                self?.apply()
            }
            return
        }
        let item = controller.navigationItem
        if let previous = appliedItem, previous !== item {
            Self.clear(previous)
        }
        item.subtitle = subtitle
        // A nil large subtitle shows `subtitle` under the large title.
        item.largeSubtitle = largeSubtitle
        item.largeAttributedSubtitle = (largeSubtitle ?? subtitle).map { largeStyle($0) }
        appliedItem = item
    }

    /// The text under the large title in UIKit's default large subtitle style
    /// (Caption 2, medium weight, secondary label). react-native-screens gives
    /// each screen its own bar appearances, and with those UIKit draws the
    /// subtitle under the large title in the inline subtitle style (Caption 1,
    /// regular) instead.
    private func largeStyle(_ text: String) -> AttributedString {
        let caption = UIFont.preferredFont(forTextStyle: .caption2, compatibleWith: traitCollection)
        let medium = caption.fontDescriptor.addingAttributes([
            .traits: [UIFontDescriptor.TraitKey.weight: UIFont.Weight.medium],
        ])
        var attributes = AttributeContainer()
        attributes.uiKit.font = UIFont(descriptor: medium, size: 0)
        attributes.uiKit.foregroundColor = UIColor.secondaryLabel
        return AttributedString(text, attributes: attributes)
    }

    private func clear() {
        if let item = appliedItem {
            Self.clear(item)
        }
        appliedItem = nil
    }

    private static func clear(_ item: UINavigationItem) {
        item.subtitle = nil
        item.largeSubtitle = nil
        item.largeAttributedSubtitle = nil
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
