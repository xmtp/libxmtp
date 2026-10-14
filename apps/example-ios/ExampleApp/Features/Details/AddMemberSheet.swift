import ExampleCore
import SwiftUI

struct AddMemberSheet: View {
	let graph: AppGraph
	let id: String
	let store: DetailsStore
	let finish: () -> Void
	@State private var input = ""
	@State private var submitted = false
	var body: some View {
		NavigationStack {
			Form {
				Section {
					TextField(detailsCopy("Handle, Ethereum address, or inbox ID"), text: $input).textInputAutocapitalization(.never)
						.autocorrectionDisabled().accessibilityIdentifier("member.recipient")
						.onChange(of: input) { _, value in store.resolvePreview(value) }
					if let recipient = store.state.recipient {
						HStack {
							MemberIdentityRow(member: recipient); Spacer(); Text(detailsCopy("Found")).font(.caption)
								.foregroundStyle(.secondary)
						}
					}
				} footer: {
					Text(
						detailsCopy(
							"The app finds the inbox for a handle or an address before it adds the member. A handle can start with @.",
						),
					)
				}
				Group {
					DetailsErrorView(error: store.state.error)
					if store.state.busy {
						ProgressView()
					}
					ForEach(Array(store.state.pendingMembers.values)) { member in MemberIdentityRow(member: member) }
				}
			}.navigationTitle(detailsCopy("Add member")).navigationBarTitleDisplayMode(.inline)
				.toolbar {
					ToolbarItem(placement: .cancellationAction) { Button(detailsCopy("Cancel"), action: finish) }
					ToolbarItem(placement: .confirmationAction) { Button(detailsCopy("Add")) { submitted = true
						store.send(.mutate(.add(input)))
					}.buttonStyle(.glassProminent).disabled(store.state.recipient == nil || store.state.busy) }
				}
				.onChange(of: store.state.busy) { _, busy in
					if submitted, busy == false, store.state.error == nil {
						finish()
					}
				}
		}.presentationDetents([.medium, .large])
	}
}
