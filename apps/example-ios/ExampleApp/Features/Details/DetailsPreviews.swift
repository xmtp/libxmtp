import ExampleCore
import SwiftUI

@MainActor private final class PreviewGroupQueries: GroupQueries {
	func load() async throws -> ConversationDetails {
		DetailsFixtures.group
	}

	func resolve(_ input: String) async throws -> GroupMember {
		.init(id: input)
	}

	func mutate(_: DetailsMutation) async throws {}
	func permission(_: PermissionWrite) async throws {}
}

@MainActor func previewDetailsStore() -> DetailsStore {
	DetailsStore(
		queries: PreviewGroupQueries(),
		ownInboxID: "you",
		origin: DestinationOrigin(session: SessionKey(profileID: UUID(), generation: 1), token: 0),
		admit: { _ in true },
		run: { job in Task { await job() } },
	)
}

struct DetailsPreviewHost<Content: View>: View {
	let content: (AppGraph) -> Content
	@State private var graph: AppGraph?
	init(@ViewBuilder content: @escaping (AppGraph) -> Content) {
		self.content = content
	}

	var body: some View {
		Group {
			if let graph {
				NavigationStack { content(graph) }
			} else {
				ProgressView()
			}
		}
		.task { do { graph = try AppGraph() } catch {} }
	}
}
