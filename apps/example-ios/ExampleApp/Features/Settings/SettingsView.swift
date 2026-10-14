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
				Avatar(
					seed: graph.active?.profile.inboxID ?? "own",
					label: DisplayInitials.person(graph.currentHandle ?? "You"),
					size: 64,
				)
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

	private func actionRow(_ title: String, symbol: String, navigates: Bool = false,
	                       color: Color = Theme.tint) -> some View
	{
		HStack(spacing: 12) {
			Image(systemName: symbol).frame(width: 20).accessibilityHidden(true)
			Text(localized(title))
			Spacer()
			if navigates {
				Image(systemName: "chevron.right").font(.caption.weight(.semibold)).foregroundStyle(.tertiary)
					.accessibilityHidden(true)
			}
		}.foregroundStyle(color).frame(maxWidth: .infinity, alignment: .leading).contentShape(Rectangle())
	}

	private func navigationRow(_ title: String, symbol: String, iconColor: Color) -> some View {
		HStack(spacing: 12) {
			Image(systemName: symbol).font(.system(size: 14))
				.foregroundStyle(.white).frame(width: 28, height: 28)
				.background(iconColor, in: RoundedRectangle(cornerRadius: 7))
				.accessibilityHidden(true)
			Text(localized(title)).foregroundStyle(.primary)
			Spacer()
			Image(systemName: "chevron.right").font(.caption.weight(.semibold)).foregroundStyle(.tertiary)
				.accessibilityHidden(true)
		}.frame(maxWidth: .infinity, alignment: .leading).contentShape(Rectangle())
	}

	private var appearanceSection: some View {
		Section {
			Picker(localized("Appearance"), selection: Binding(
				get: { store.state.appearance }, set: { store.send(.appearance($0)) },
			)) {
				Text("Automatic", tableName: "Settings").tag("system")
				Text("Light", tableName: "Settings").tag("light")
				Text("Dark", tableName: "Settings").tag("dark")
			}.pickerStyle(.segmented).labelsHidden()
				.listRowInsets(EdgeInsets(top: 8, leading: 8, bottom: 8, trailing: 8))
		} header: {
			Text("Appearance", tableName: "Settings")
		} footer: {
			Text("settings.appearance.detail", tableName: "Settings")
		}
	}

	private var privacySection: some View {
		Section {
			Button { graph.navigate(.requests) } label: {
				navigationRow("Message requests", symbol: "tray", iconColor: Theme.tint)
			}.buttonStyle(.plain)
		} header: {
			Text("Privacy", tableName: "Settings")
		} footer: {
			Text("settings.privacy.detail", tableName: "Settings")
		}
	}

	private var developerSection: some View {
		Section {
			Button { graph.navigate(.attachmentRecovery) } label: {
				navigationRow("Attachment recovery", symbol: "doc", iconColor: .gray)
			}.buttonStyle(.plain)
		} header: {
			Text("Developer tools", tableName: "Settings")
		} footer: {
			Text("settings.developer.detail", tableName: "Settings")
		}
	}

	private var deviceSection: some View {
		Section {
			Button { store.send(.signOut) } label: {
				actionRow("Sign out", symbol: "rectangle.portrait.and.arrow.right")
			}.buttonStyle(.plain)
			Button(role: .destructive) { store.send(.reset) } label: {
				actionRow("Delete local data", symbol: "trash", color: .red)
			}.buttonStyle(.plain)
			if graph.session.state == .resetIncomplete {
				Text("Local deletion is incomplete", tableName: "Settings")
				Button {
					Task {
						do { try await graph.session.continueReset() } catch { graph.error = localized("Local deletion is incomplete") }
					}
				} label: {
					actionRow("Continue deletion", symbol: "arrow.clockwise")
				}.buttonStyle(.plain)
			}
		} header: {
			Text("This device", tableName: "Settings")
		} footer: {
			Text("settings.device.detail", tableName: "Settings")
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
