import SwiftUI

/// A native sheet with cross-fading compose content when Reduce Motion is enabled.
struct ReducedMotionSheet<Item: Hashable>: ViewModifier {
	let item: Binding<Item?>
	let presentationChanged: (Bool) -> Void
	let sheetContent: (Item) -> AnyView
	@State private var presentedItem: Item?
	@State private var cachedContent: AnyView?
	@State private var isPresented = false
	@State private var opacity = 0.0
	@State private var closing = false
	@State private var revision = UUID()

	func body(content: Content) -> some View {
		content
			.sheet(isPresented: Binding(get: { isPresented }, set: { _ in }), onDismiss: didDismiss) {
				if let cachedContent {
					cachedContent
						.background(Color(uiColor: .systemGroupedBackground))
						.opacity(opacity)
						.presentationBackground(.clear)
						.interactiveDismissDisabled()
						.disabled(item.wrappedValue != presentedItem || closing)
						.allowsHitTesting(item.wrappedValue == presentedItem && !closing)
						.accessibilityHidden(item.wrappedValue != presentedItem || closing)
						.task(id: presentedItem) { await fadeIn() }
				}
			}
			.onChange(of: item.wrappedValue, initial: true) { _, value in synchronize(value) }
			.task(id: closing) { await finishClosingFade() }
	}

	private func synchronize(_ value: Item?) {
		guard isPresented else {
			guard cachedContent == nil else { return }
			if let value {
				present(value)
			}
			return
		}
		guard value != presentedItem, !closing else { return }
		closing = true
		withAnimation(.easeInOut(duration: 0.2)) { opacity = 0 }
	}

	private func finishClosingFade() async {
		guard closing else { return }
		let expected = revision
		do { try await Task.sleep(for: .milliseconds(200)) } catch { return }
		guard !Task.isCancelled, closing, revision == expected else { return }
		withoutPresentationAnimation { isPresented = false }
	}

	private func present(_ value: Item) {
		presentedItem = value
		cachedContent = sheetContent(value)
		closing = false
		revision = UUID()
		opacity = 0
		presentationChanged(true)
		withoutPresentationAnimation { isPresented = true }
	}

	private func fadeIn() async {
		let expected = revision
		// Commit the transparent native sheet before starting its content animation.
		do { try await Task.sleep(for: .milliseconds(16)) } catch { return }
		guard !Task.isCancelled, !closing, revision == expected, item.wrappedValue == presentedItem else { return }
		var transaction = Transaction(animation: .easeInOut(duration: 0.2))
		transaction.disablesAnimations = false
		withTransaction(transaction) { opacity = 1 }
	}

	private func didDismiss() {
		cachedContent = nil
		presentedItem = nil
		closing = false
		revision = UUID()
		presentationChanged(false)
		if let next = item.wrappedValue {
			Task { @MainActor in
				await Task.yield()
				guard !isPresented, item.wrappedValue == next else { return }
				present(next)
			}
		}
	}

	private func withoutPresentationAnimation(_ update: () -> Void) {
		var transaction = Transaction(animation: nil)
		transaction.disablesAnimations = true
		withTransaction(transaction, update)
	}
}
