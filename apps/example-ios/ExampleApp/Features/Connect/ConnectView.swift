import ExampleCore
import SwiftUI

struct ConnectView: View {
	let store: ConnectStore
	@ScaledMetric(relativeTo: .largeTitle) private var heroTitleSize = 32
	private enum Field: Hashable { case address, name, token }
	@FocusState private var focusedField: Field?
	@State private var advanced = false
	@State private var confirmRecoveryReset = false
	@Environment(\.motionPolicy) private var motion
	var body: some View {
		ScrollView {
			VStack(spacing: 28) {
				VStack(spacing: 12) {
					Image("Logo").resizable().scaledToFit().foregroundStyle(Theme.accentText).frame(width: 68, height: 68)
						.accessibilityHidden(true).padding(.bottom, 6)
					Text("A place to connect.", tableName: "Connect")
						.font(.system(size: heroTitleSize, weight: .bold))
					Text("Join a server to start a conversation.", tableName: "Connect").font(.body)
						.foregroundStyle(Theme.secondaryLabel)
				}.frame(maxWidth: .infinity).padding(.top, 50)
				if store.session.state == .resetIncomplete {
					recovery("Local deletion is incomplete", action: "Continue local data removal", send: .continueReset)
				} else if store.session.state == .restoreFailed {
					VStack(spacing: 16) {
						recovery("Cannot restore this account. Try again.", action: "Retry", send: .retryRestore)
						Button(localized("Delete local data"), role: .destructive) { confirmRecoveryReset = true }.buttonStyle(.glass)
					}
				} else {
					VStack(alignment: .leading, spacing: 12) {
						Text("Your server", tableName: "Connect").font(.footnote.weight(.semibold)).foregroundStyle(Theme.secondaryLabel)
							.padding(
								.horizontal,
								16,
							)
						VStack(spacing: 0) {
							field("Server address") { TextField(
								localized("https://server.example"),
								text: Binding(get: { store.state.address }, set: { store.send(.addressChanged($0)) }),
							).focused($focusedField, equals: .address).keyboardType(.URL).accessibilityIdentifier("server-address") }
							Divider().padding(.leading, 16)
							field("Server name (optional)") { TextField(
								localized("Server name"),
								text: Binding(get: { store.state.serverName }, set: { store.send(.nameChanged($0)) }),
							).focused($focusedField, equals: .name) }
							if store.state.requiresToken {
								Divider().padding(.leading, 16); field("Access token") { SecureField(
									localized("Access token"),
									text: Binding(get: { store.state.token }, set: { store.send(.tokenChanged($0)) }),
								).focused($focusedField, equals: .token) }.transition(motion.transition)
							}
							Divider().padding(.leading, 16)
							HStack(spacing: 10) {
								if store.state.checking {
									ProgressView().controlSize(.small); Text("Checking server", tableName: "Connect")
								} else if let error = store.state.error {
									Image(systemName: "exclamationmark.circle"); Text(localized(error))
								} else {
									Image(systemName: store.state
										.ready ? "checkmark.seal" :
										"network"); Text(localized(store.state.ready ? "Server ready" : "Enter a server address"))
								}
								Spacer()
							}.font(.subheadline).foregroundStyle(store.state.error == nil ? .green : .red).padding(16)
						}.background(Color(.secondarySystemGroupedBackground), in: .rect(cornerRadius: 24))
						VStack(alignment: .leading, spacing: 8) {
							Button { withAnimation(motion.animation) { advanced.toggle() } } label: {
								HStack {
									Text(localized("Advanced settings")); Spacer(); Image(systemName: "chevron.down")
										.rotationEffect(.degrees(advanced ? 180 : 0))
								}
								.padding(.horizontal, 4).frame(minHeight: 38)
								.foregroundStyle(Theme.accentText)
							}.buttonStyle(.glass).frame(minHeight: Theme.minimumHitTarget)
							if advanced {
								Text("Use HTTPS for a remote backend.", tableName: "Connect").font(.footnote)
									.foregroundStyle(Theme.secondaryLabel)
									.padding(
										.horizontal,
										16,
									).transition(motion.transition)
							}
						}
						Text("Use a server address from someone you trust.", tableName: "Connect").font(.footnote)
							.foregroundStyle(Theme.secondaryLabel).padding(
								.horizontal,
								16,
							)
					}
				}
			}.padding(.horizontal, 16).padding(.bottom, 24)
				.frame(maxWidth: 560).frame(maxWidth: .infinity)
		}.background(Color(.systemGroupedBackground))
			.scrollDismissesKeyboard(.interactively)
			.animation(motion.animation, value: store.state.requiresToken)
			.animation(motion.animation, value: store.state.checking)
			.animation(motion.animation, value: store.state.joining)
			.toolbar(.hidden, for: .navigationBar).onAppear { store.send(.appear) }
			.confirmationDialog(localized("Delete local data?"), isPresented: $confirmRecoveryReset) {
				Button(localized("Delete local data"), role: .destructive) { store.send(.resetFailedProfile) }
				Button(localized("Cancel"), role: .cancel) {}
			} message: {
				Text(
					localized(
						"This removes this server’s local identity, messages and files. It does not delete messages on other devices.",
					),
				)
			}
			.safeAreaInset(edge: .bottom, spacing: 0) {
				if store.session.state != .resetIncomplete, store.session.state != .restoreFailed {
					ActionFooter {
						Button { focusedField = nil; store.send(.join) } label: {
							HStack {
								Spacer()
								if store.state.joining {
									ProgressView()
								}
								Text(store.state.joining ? localized("Joining server…") : localized("Join a server"))
									.font(.headline).contentTransition(.opacity)
								Spacer()
							}
						}
						.buttonStyle(.glassProminent)
						.controlSize(.large)
						.disabled(!store.state.canJoin)
						.accessibilityIdentifier("join-server")
					}
				}
			}
	}

	private func field(_ title: String, @ViewBuilder content: () -> some View) -> some View {
		VStack(alignment: .leading, spacing: 4) {
			Text(localized(title)).font(.caption).foregroundStyle(Theme.secondaryLabel); content().font(.body)
				.textInputAutocapitalization(.never).autocorrectionDisabled()
		}.frame(
			maxWidth: .infinity,
			alignment: .leading,
		).padding(.horizontal, 16).padding(.vertical, 10)
	}

	private func recovery(_ text: String, action: String, send: ConnectAction) -> some View {
		VStack(spacing: 20) {
			Text(localized(text)); Button(localized(action)) { store.send(send) }.buttonStyle(.glassProminent)
		}.padding(24)
	}

	private func localized(_ key: String) -> String {
		String(localized: String.LocalizationValue(key), table: "Connect")
	}
}
