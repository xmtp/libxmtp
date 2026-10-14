import ExampleCore
import ExampleSDK
import SwiftUI

func detailsCopy(_ key: String) -> String {
	featureCopy(key, table: "Details")
}

@MainActor func makeDetailsStore(_ graph: AppGraph, id: String) -> DetailsStore? {
	guard let active = graph.active, let client = graph.factory.client(for: active.key) else { return nil }
	let origin = DestinationOrigin(session: active.key, token: graph.routes.token)
	return DetailsStore(
		queries: SDKGroupQueries(
			client: client,
			id: id,
			reads: graph.reads,
			valid: { [weak graph] in guard let graph else { return false }
				return graph.routes.admit(origin, fence: graph.fence)
			},
			privacyChanged: { [weak controller = graph.push?.controller] beginning in
				if beginning {
					controller?.beginPrivacyChange()
				} else {
					controller?.endPrivacyChange()
				}
			},
		),
		origin: origin,
		admit: { [weak graph] in guard let graph else { return false }
			return graph.routes.admit($0, fence: graph.fence)
		},
		run: { [weak graph] job in graph?.session.scope.add(job) },
	)
}

struct DetailsErrorView: View {
	let error: String?
	var table = "Details"
	var body: some View {
		if let error {
			Text(featureCopy(error, table: table)).foregroundStyle(.red).accessibilityIdentifier("details.error")
		}
	}
}

struct MemberIdentityRow: View {
	let member: GroupMember
	var body: some View {
		HStack(spacing: 12) { Avatar(seed: member.id, label: String(member.title.prefix(1)).uppercased())
			IdentityText(inboxID: member.id, handle: member.handle)
			Spacer()
			Text(detailsCopy(member.role.rawValue)).font(.subheadline).foregroundStyle(.secondary)
		}.padding(.vertical, 4)
	}
}
