import Foundation

private let sharedCopyKeys: [String: String] = [
	"Cancel": "common.cancel",
	"Close": "common.close",
	"Retry": "common.retry",
	"Add": "common.add",
	"Edit": "common.edit",
	"Save": "common.save",
	"Selected": "common.selected",
	"Off": "common.off",
	"Custom": "common.custom",
	"Member": "common.member",
	"Admin": "common.admin",
	"Super admin": "common.superAdmin",
	"All members": "common.allMembers",
	"Admins only": "common.adminsOnly",
	"Admins": "common.admins",
	"Super admins": "common.superAdmins",
	"Denied": "common.denied",
	"Copy inbox ID": "common.copyInboxID",
	"Inbox ID": "common.inboxID",
	"Permissions": "common.permissions",
	"Disappearing messages": "common.disappearing",
	"Group fields": "common.groupFields",
	"My fields": "common.myFields",
	"Membership change pending": "common.membershipPending",
	"Description": "common.description",
	"Name": "common.name",
	"Cannot update this conversation. Try again.": "common.conversationUpdateFailed",
	"Cannot update these fields. Reload and try again.": "common.metadataUpdateFailed",
]

func featureCopy(_ key: String, table: String) -> String {
	if let sharedKey = sharedCopyKeys[key] {
		return String(localized: String.LocalizationValue(sharedKey), table: "Common")
	}
	return String(localized: String.LocalizationValue(key), table: table)
}
