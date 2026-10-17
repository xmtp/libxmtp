// Owns one route runtime; rendering lives in the ChatContentView extensions.
import ExampleCore
import PhotosUI
import SwiftUI
import UniformTypeIdentifiers

struct ChatFeatureView: View {
	let graph: AppGraph
	let id: String
	@State private var runtime: ChatRuntime?
	@State private var failure: String?
	@State private var observer = UUID()
	private var destination: ChatDestination? {
		ChatDestination(
			conversationID: id,
			session: graph.active?.key,
			routes: graph.routes,
		)
	}

	var body: some View {
		Group {
			if let runtime {
				ChatContentView(graph: graph, id: id, runtime: runtime).id(runtime.cleanupKey)
			} else if let failure {
				ContentUnavailableView(
					"Cannot open chat",
					systemImage: "exclamationmark.bubble",
					description: Text(String(localized: String.LocalizationValue(failure), table: "Chat")),
				)
			} else {
				ProgressView()
			}
		}.task(id: destination) {
			let expected = destination
			PreviewImages.shared.clear()
			graph.messageObservers.removeValue(forKey: observer)
			graph.messageDeletionObservers.removeValue(forKey: observer)
			let previous = runtime
			runtime = nil
			if let previous {
				await graph.session.scope.runCleanup(for: previous.cleanupKey)
			}
			guard !Task.isCancelled, let expected, destination == expected else { return }
			do {
				let value = try ChatRuntime(graph: graph, id: id)
				guard !Task.isCancelled, destination == expected else { return }
				failure = nil
				runtime = value
				graph.session.scope.setCleanup(for: value.cleanupKey) {
					await value.imports.close()
					await value.timeline.close(); await value.composer.close(); await value.actions.close(); await value.attachments
						.close()
				}
				graph.messageDeletionObservers[observer] = { changed, messageID, session in
					value.timeline.send(.messageDeleted(conversationID: changed, messageID: messageID, session: session))
				}
				graph.messageObservers[observer] = { changed in
					if changed == id {
						value.composer.send(.reload)
						await value.timeline.refresh()
					}
				}
				value.timeline.send(.load); value.attachments.send(.load)
			} catch { failure = "Try opening this conversation again." }
		}
		.onChange(of: graph.consentRevision) { _, _ in
			PreviewImages.shared.clear(); runtime?.attachments.invalidateDownloads(); runtime?.timeline.send(.refresh)
		}
		.onDisappear {
			PreviewImages.shared.clear()
			graph.messageDeletionObservers.removeValue(forKey: observer)
			graph.messageObservers
				.removeValue(forKey: observer); let previous = runtime; Task {
					if let previous {
						await graph.session.scope.runCleanup(for: previous.cleanupKey)
					}
				}
		}
	}
}
