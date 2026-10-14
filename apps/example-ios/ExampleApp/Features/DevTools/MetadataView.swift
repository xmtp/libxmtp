import ExampleCore
import ExampleSDK
import SwiftUI

@MainActor func makeMetadataStore(_ graph: AppGraph, id: String) -> MetadataStore? {
	guard let active = graph.active, let client = graph.factory.client(for: active.key) else { return nil }
	let origin = DestinationOrigin(session: active.key, token: graph.routes.token)
	let valid = { @MainActor [weak graph] in guard let graph else { return false }
		return graph.routes.admit(origin, fence: graph.fence)
	}
	return MetadataStore(
		queries: SDKMetadataQueries(client: client, id: id, reads: graph.reads, valid: valid),
		origin: origin,
		admit: { _ in valid() },
		run: { [weak graph] job in graph?.session.scope.add(job) },
	)
}

struct MetadataView: View {
	var preview = false
	let graph: AppGraph
	let id: String
	let own: Bool
	@State private var store: MetadataStore?
	@State private var draft = OwnFieldDraft()
	var body: some View {
		List {
			Section {
				Text(metadataCopy(own ? "These values apply to this conversation. Only your entry can change." :
						"Fields come from this conversation's committed registry.")).foregroundStyle(.secondary)
			}
			if let store {
				ForEach(store.state.snapshot.fields.filter { own == $0.userField }) { field in
					Section {
						if own {
							ownEditor(field)
						} else {
							GroupFieldEditor(field: field) { store.send(.edit($0)) }
						}
						Text(field.policy).font(.caption).foregroundStyle(.secondary)
					} header: { Text("\(field.name) · \(String(format: "0x%04X", field.id)) · \(field.shape.rawValue)") }
				}
				if own, !store.state.snapshot.members.isEmpty {
					Section(metadataCopy("Other members")) { ForEach(store.state.snapshot.members, id: \.inboxID) { member in VStack(
						alignment: .leading,
						spacing: 5,
					) { IdentityText(inboxID: member.inboxID, handle: graph.identityDirectory?.identities[member.inboxID]?.handle)
						ForEach(member.fields) { field in
							Text("\(field.name): \(field.present ? field.scalar : metadataCopy("Absent"))").font(.subheadline)
								.foregroundStyle(.secondary)
						}
					}.padding(.vertical, 4) } }
				}
				if !store.state.snapshot.missing.isEmpty {
					Section(metadataCopy("Offered, not registered")) { ForEach(store.state.snapshot.missing, id: \.self) { Text($0) }
						Text(metadataCopy("These fields are offered by the server but are not registered in this conversation."))
							.foregroundStyle(.secondary)
					}
				}
				DetailsErrorView(error: store.state.error, table: "DevTools")
				if store.state.busy {
					ProgressView()
				}
			}
		}.listStyle(.insetGrouped).navigationTitle(metadataCopy(own ? "My fields" : "Group fields"))
			.toolbar {
				if own {
					Button(metadataCopy("Save changed fields"), action: save).buttonStyle(.glassProminent)
						.disabled(store?.state.busy != false || draft.changes.isEmpty)
				} else {
					Button { store?.send(.refresh) } label: { Label(metadataCopy("Reload fields"), systemImage: Symbol.retry) }
						.buttonStyle(.glass)
				}
			}
			.task(id: graph.routes.token) { store = preview ? previewMetadataStore() : makeMetadataStore(graph, id: id)
				store?.send(.refresh)
			}
			.onChange(of: store?.state.snapshot) { _, snapshot in
				if let snapshot {
					merge(snapshot.fields)
				}
			}
	}

	@ViewBuilder private func ownEditor(_ field: MetadataField) -> some View {
		if !field.canWrite {
			Text(metadataCopy(field.immutable && field
					.componentPresent ? "Immutable field: this component is already set." : "Unsupported field type or policy."))
				.foregroundStyle(.secondary)
		} else {
			TextField(
				field.name,
				text: Binding(get: { draft.value(for: field.id).text }, set: { draft.set(field.id, to: .value($0)) }),
				axis: .vertical,
			).textInputAutocapitalization(.never).autocorrectionDisabled()
			Text(metadataCopy(draft.changed(field.id) ? "Changed" : draft.value(for: field.id) == .absent ? "Absent" : "Saved"))
				.font(.caption).foregroundStyle(.secondary)
			HStack { Button(metadataCopy("Set empty")) { draft.set(field.id, to: .value("")) }.buttonStyle(.glass)
				Button(metadataCopy("Clear")) { draft.set(field.id, to: .absent) }.buttonStyle(.glass)
			}
		}
	}

	private func merge(_ fields: [MetadataField]) {
		draft.merge(fields)
	}

	private func save() {
		let changes = draft.changes
		guard !changes.isEmpty else { return }
		store?.send(.edit(.own(changes)))
	}
}

struct GroupFieldEditor: View {
	let field: MetadataField
	let send: (MetadataEdit) -> Void
	@State private var value = ""
	@State private var key = ""
	@State private var entryValue = ""
	var body: some View {
		if !field.canWrite {
			Text(metadataCopy(field.immutable && field
					.componentPresent ? "Immutable field: this component is already set." : "Unsupported field type or policy."))
				.foregroundStyle(.secondary)
		} else if field.shape == .string || field.shape == .bytes {
			LabeledContent(metadataCopy("Current value"), value: field.present ? field.scalar : metadataCopy("Absent"))
			TextField(field.name, text: $value, axis: .vertical).textInputAutocapitalization(.never).autocorrectionDisabled()
			HStack { Button(metadataCopy("Set")) { send(.scalar(field.id, value)) }.buttonStyle(.glassProminent)
				Button(metadataCopy("Clear")) { send(.scalar(field.id, nil)) }.buttonStyle(.glass)
			}
		} else {
			ForEach(field.entries, id: \.key) { entry in
				HStack { VStack(alignment: .leading) {
					Text(entry.key).font(.footnote.monospaced())
					if field.shape == .byteMap {
						Text(entry.value).font(.footnote.monospaced())
					}
				}
				Spacer()
				if field.shape == .byteMap {
					Button(metadataCopy("Edit")) { key = entry.key
						entryValue = entry.value
					}.buttonStyle(.glass)
				}
				Button(role: .destructive) { send(.entry(field.id, .delete, entry.key, "")) } label: {
					Image(systemName: Symbol.delete)
				}.accessibilityLabel(metadataCopy("Delete entry")).buttonStyle(.glass)
				}
			}
			TextField(metadataCopy(field.shape == .inboxSet ? "Inbox ID" : "Key in hex"), text: $key)
				.textInputAutocapitalization(.never).autocorrectionDisabled()
			if field.shape == .byteMap {
				TextField(metadataCopy("Value in hex"), text: $entryValue).textInputAutocapitalization(.never)
					.autocorrectionDisabled()
			}
			Button(metadataCopy(field.shape == .byteMap && field.entries.contains { $0.key == key } ? "Update" : "Add")) {
				send(.entry(
					field.id,
					field.shape == .byteMap && field.entries.contains { $0.key == key } ? .update : .insert,
					key,
					entryValue,
				))
			}.buttonStyle(.glassProminent)
				.disabled(field.shape != .byteMap && field.entries.contains { $0.key == key })
		}
	}
}

@MainActor private final class PreviewMetadataQueries: MetadataQueries {
	func load() async throws -> MetadataSnapshot {
		DetailsFixtures.fields
	}

	func edit(_: MetadataEdit, baseline _: MetadataSnapshot) async throws {}
}

@MainActor private func previewMetadataStore() -> MetadataStore {
	MetadataStore(
		queries: PreviewMetadataQueries(),
		origin: DestinationOrigin(session: SessionKey(profileID: UUID(), generation: 1), token: 0),
		admit: { _ in true },
		run: { job in Task { await job() } },
	)
}
