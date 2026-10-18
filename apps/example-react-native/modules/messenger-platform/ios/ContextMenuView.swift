import ExpoModulesCore
import UIKit

/// One context menu item.
struct ContextMenuItem: Record {
    @Field var id: String = ""
    @Field var title: String = ""
    @Field var destructive: Bool = false
}

/// A container with a UIKit context menu, like SwiftUI `.contextMenu` on a
/// list row: a long press lifts the view and shows the items. Children keep
/// their own layout, touches and accessibility.
public final class ContextMenuView: ExpoView, UIContextMenuInteractionDelegate {
    var items: [ContextMenuItem] = []
    let onSelect = EventDispatcher()
    /// A lifted preview is elevated, which turns `systemBackground` gray in
    /// dark mode. That matches a SwiftUI row with the default row background
    /// (`true`); a row with a clear background keeps the page color (`false`).
    var elevated = false {
        didSet { updateInterfaceLevel() }
    }

    public required init(appContext: AppContext? = nil) {
        super.init(appContext: appContext)
        updateInterfaceLevel()
        addInteraction(UIContextMenuInteraction(delegate: self))
    }

    private func updateInterfaceLevel() {
        if elevated {
            traitOverrides.remove(UITraitUserInterfaceLevel.self)
        } else {
            traitOverrides.userInterfaceLevel = .base
        }
    }

    public func contextMenuInteraction(
        _: UIContextMenuInteraction,
        configurationForMenuAtLocation _: CGPoint
    ) -> UIContextMenuConfiguration? {
        guard !items.isEmpty else { return nil }
        cancelReactNativeTouches()
        let items = items
        return UIContextMenuConfiguration(identifier: nil, previewProvider: nil) { [weak self] _ in
            UIMenu(children: items.map { item in
                UIAction(title: item.title, attributes: item.destructive ? .destructive : []) { _ in
                    self?.onSelect(["id": item.id])
                }
            })
        }
    }

    /// Lifts the row on `systemBackground`: the page color, or the elevated
    /// color when `elevated` (the default preview background is a lighter
    /// gray in dark mode).
    public func contextMenuInteraction(
        _: UIContextMenuInteraction,
        configuration _: UIContextMenuConfiguration,
        highlightPreviewForItemWithIdentifier _: any NSCopying
    ) -> UITargetedPreview? {
        let parameters = UIPreviewParameters()
        parameters.backgroundColor = .systemBackground
        return UITargetedPreview(view: self, parameters: parameters)
    }

    /// Ends the React Native touch that started the long press, so the row
    /// neither stays highlighted nor fires its tap.
    private func cancelReactNativeTouches() {
        guard let handlerClass = NSClassFromString("RCTSurfaceTouchHandler") else { return }
        var view: UIView? = self
        while let current = view {
            if let recognizer = current.gestureRecognizers?.first(where: { $0.isKind(of: handlerClass) }) {
                if recognizer.isEnabled {
                    recognizer.isEnabled = false
                    recognizer.isEnabled = true
                }
                return
            }
            view = current.superview
        }
    }
}
