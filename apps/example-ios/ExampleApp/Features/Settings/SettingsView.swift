import ExampleCore
import SwiftUI

struct SettingsView: View {
	let graph: AppGraph
	@State private var store: SettingsStore

	init(graph: AppGraph) {
		self.graph = graph
		let token = graph.routes.token
		_store = State(initialValue: SettingsStore(
			appearance: graph.appearance,
			valid: { graph.routes.token == token },
			saveAppearance: graph.setAppearance,
			signOut: {
				try await graph.session.signOut()
				await graph.bindSession()
			},
			reset: {
				try await graph.session.reset()
				await graph.bindSession()
			},
		))
	}

	var body: some View {
		Form {
			profileSection
			serverSection
			PushSettingsSection(coordinator: graph.push)
			appearanceSection
			privacySection
			SendReviewSection(graph: graph)
			developerSection
			deviceSection
			if let error = store.state.error ?? graph.error {
				Text(error).foregroundStyle(.red)
			}
		}
		.navigationTitle(localized("Settings"))
		.alert(localized("Sign out?"), isPresented: confirmation(.signOut)) {
			Button(localized("Sign out"), role: .destructive) { store.send(.confirm) }
			Button(localized("Cancel"), role: .cancel) { store.send(.cancel) }
		} message: { Text("settings.signOut.detail", tableName: "Settings") }
		.alert(localized("Delete local data?"), isPresented: confirmation(.reset)) {
			Button(localized("Delete local data"), role: .destructive) { store.send(.confirm) }
			Button(localized("Cancel"), role: .cancel) { store.send(.cancel) }
		} message: { Text("settings.reset.detail", tableName: "Settings") }
	}

	private var profileSection: some View {
		Section {
			HStack(spacing: 12) {
				Avatar(seed: graph.active?.profile.inboxID ?? "own", label: DisplayInitials.person(graph.currentHandle ?? "You"))
				VStack(alignment: .leading, spacing: 3) {
					if let handle = graph.currentHandle {
						HandleText(handle: handle).font(.title3.bold())
					} else {
						Text("No handle", tableName: "Settings").font(.headline)
					}
					Text("Your handle on this server", tableName: "Settings")
						.font(.caption).foregroundStyle(Theme.secondaryLabel)
				}
			}
			if !graph.otherHandles.isEmpty {
				Text("Other handles", tableName: "Settings").font(.caption).foregroundStyle(Theme.secondaryLabel)
				ForEach(graph.otherHandles, id: \.self) { HandleText(handle: $0) }
				Text("settings.otherHandles.detail", tableName: "Settings")
					.font(.footnote).foregroundStyle(Theme.secondaryLabel)
			}
			Button { graph.navigate(.changeHandle) } label: {
				actionRow("Change handle", symbol: "at", navigates: true)
			}.buttonStyle(.plain)
			if let inbox = graph.active?.profile.inboxID {
				VStack(alignment: .leading, spacing: 5) {
					Text("Inbox ID", tableName: "Settings").font(.caption).foregroundStyle(Theme.secondaryLabel)
					Text(inbox).font(.system(.footnote, design: .monospaced)).lineLimit(1)
						.truncationMode(.tail)
				}.frame(maxWidth: .infinity, alignment: .leading)
					.accessibilityElement(children: .ignore)
					.accessibilityLabel(localized("Inbox ID"))
					.accessibilityValue(inbox)
					.accessibilityIdentifier("profile-inbox-id")
				Button { UIPasteboard.general.string = inbox } label: {
					actionRow("Copy inbox ID", symbol: "doc.on.doc")
				}.buttonStyle(.plain)
			}
		}
	}

	private var serverSection: some View {
		Section {
			LabeledContent(localized("Name"), value: graph.serverName)
			VStack(alignment: .leading, spacing: 5) {
				Text("Address", tableName: "Settings").font(.caption).foregroundStyle(Theme.secondaryLabel)
				Text(graph.active?.profile.url ?? "").font(.system(.body, design: .monospaced)).lineLimit(1)
					.truncationMode(.middle)
			}.frame(maxWidth: .infinity, alignment: .leading)
			Button { UIPasteboard.general.string = graph.active?.profile.url } label: {
				actionRow("Copy server address", symbol: "doc.on.doc")
			}.buttonStyle(.plain)
		} header: {
			Text("Server", tableName: "Settings")
		} footer: {
			Text("settings.server.detail", tableName: "Settings")
		}
	}

	private func actionRow(_ title: String, symbol: String, navigates: Bool = false) -> some View {
		HStack(spacing: 12) {
			Image(systemName: symbol).frame(width: 20).accessibilityHidden(true)
			Text(localized(title))
			Spacer()
			if navigates {
				Image(systemName: "chevron.right").font(.caption.weight(.semibold)).foregroundStyle(.tertiary)
					.accessibilityHidden(true)
			}
		}.foregroundStyle(Theme.tint).frame(maxWidth: .infinity, alignment: .leading).contentShape(Rectangle())
	}

	private var appearanceSection: some View {
		Section(localized("Appearance")) {
			Picker(localized("Appearance"), selection: Binding(
				get: { store.state.appearance }, set: { store.send(.appearance($0)) },
			)) {
				Text("Automatic", tableName: "Settings").tag("system")
				Text("Light", tableName: "Settings").tag("light")
				Text("Dark", tableName: "Settings").tag("dark")
			}
		}
	}

	private var privacySection: some View {
		Section(localized("Privacy")) {
			Button(localized("Message requests")) { graph.navigate(.requests) }.buttonStyle(.glass)
			Text("settings.privacy.detail", tableName: "Settings").font(.footnote).foregroundStyle(Theme.secondaryLabel)
		}
	}

	private var developerSection: some View {
		Section(localized("Developer tools")) {
			Button(localized("Attachment recovery")) { graph.navigate(.attachmentRecovery) }.buttonStyle(.glass)
			Text("settings.developer.detail", tableName: "Settings").font(.footnote).foregroundStyle(Theme.secondaryLabel)
		}
	}

	private var deviceSection: some View {
		Section(localized("This device")) {
			Button(localized("Sign out"), role: .destructive) { store.send(.signOut) }.buttonStyle(.glass)
			Button(localized("Delete local data"), role: .destructive) { store.send(.reset) }.buttonStyle(.glass)
			if graph.session.state == .resetIncomplete {
				Text("Local deletion is incomplete", tableName: "Settings")
				Button(localized("Continue deletion")) {
					Task {
						do { try await graph.session.continueReset() } catch { graph.error = localized("Local deletion is incomplete") }
					}
				}.buttonStyle(.glass)
			}
			Text("settings.device.detail", tableName: "Settings").font(.footnote).foregroundStyle(Theme.secondaryLabel)
		}
	}

	private func confirmation(_ choice: SettingsConfirmation) -> Binding<Bool> {
		Binding(get: { store.state.confirmation == choice }, set: {
			if !$0 {
				store.dismissConfirmation(choice)
			}
		})
	}

	private func localized(_ key: String) -> String {
		String(localized: String.LocalizationValue(key), table: "Settings")
	}
}
