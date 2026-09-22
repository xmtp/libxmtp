import type { Command } from "@oclif/core";
import AddressAuthorized from "@/commands/address-authorized";
import CanMessage from "@/commands/can-message";
import ClientAddAccount from "@/commands/client/add-account";
import ClientChangeRecoveryIdentifier from "@/commands/client/change-recovery-identifier";
import ClientInboxId from "@/commands/client/inbox-id";
import ClientInfo from "@/commands/client/info";
import ClientKeyPackageStatus from "@/commands/client/key-package-status";
import ClientRemoveAccount from "@/commands/client/remove-account";
import ClientRevokeAllOtherInstallations from "@/commands/client/revoke-all-other-installations";
import ClientRevokeInstallations from "@/commands/client/revoke-installations";
import ClientSign from "@/commands/client/sign";
import ClientVerifySignature from "@/commands/client/verify-signature";
import ConversationAddAdmin from "@/commands/conversation/add-admin";
import ConversationAddMembers from "@/commands/conversation/add-members";
import ConversationAddSuperAdmin from "@/commands/conversation/add-super-admin";
import ConversationConsentState from "@/commands/conversation/consent-state";
import ConversationCountMessages from "@/commands/conversation/count-messages";
import ConversationDebugInfo from "@/commands/conversation/debug-info";
import ConversationListAdmins from "@/commands/conversation/list-admins";
import ConversationListSuperAdmins from "@/commands/conversation/list-super-admins";
import ConversationMembers from "@/commands/conversation/members";
import ConversationMessages from "@/commands/conversation/messages";
import ConversationPermissions from "@/commands/conversation/permissions";
import ConversationPublishMessages from "@/commands/conversation/publish-messages";
import ConversationRemoveAdmin from "@/commands/conversation/remove-admin";
import ConversationRemoveMembers from "@/commands/conversation/remove-members";
import ConversationRemoveSuperAdmin from "@/commands/conversation/remove-super-admin";
import ConversationRequestRemoval from "@/commands/conversation/request-removal";
import ConversationSendMarkdown from "@/commands/conversation/send-markdown";
import ConversationSendReaction from "@/commands/conversation/send-reaction";
import ConversationSendReadReceipt from "@/commands/conversation/send-read-receipt";
import ConversationSendReply from "@/commands/conversation/send-reply";
import ConversationSendText from "@/commands/conversation/send-text";
import ConversationStream from "@/commands/conversation/stream";
import ConversationSync from "@/commands/conversation/sync";
import ConversationUpdateConsent from "@/commands/conversation/update-consent";
import ConversationUpdateDescription from "@/commands/conversation/update-description";
import ConversationUpdateImageUrl from "@/commands/conversation/update-image-url";
import ConversationUpdateName from "@/commands/conversation/update-name";
import ConversationUpdatePermission from "@/commands/conversation/update-permission";
import ConversationsCreateDm from "@/commands/conversations/create-dm";
import ConversationsCreateGroup from "@/commands/conversations/create-group";
import ConversationsGet from "@/commands/conversations/get";
import ConversationsGetDm from "@/commands/conversations/get-dm";
import ConversationsGetMessage from "@/commands/conversations/get-message";
import ConversationsHmacKeys from "@/commands/conversations/hmac-keys";
import ConversationsList from "@/commands/conversations/list";
import ConversationsStream from "@/commands/conversations/stream";
import ConversationsStreamAllMessages from "@/commands/conversations/stream-all-messages";
import ConversationsSync from "@/commands/conversations/sync";
import ConversationsSyncAll from "@/commands/conversations/sync-all";
import InboxStates from "@/commands/inbox-states";
import Init from "@/commands/init";
import InstallationAuthorized from "@/commands/installation-authorized";
import PreferencesGetConsent from "@/commands/preferences/get-consent";
import PreferencesInboxState from "@/commands/preferences/inbox-state";
import PreferencesInboxStates from "@/commands/preferences/inbox-states";
import PreferencesSetConsent from "@/commands/preferences/set-consent";
import PreferencesStream from "@/commands/preferences/stream";
import PreferencesSync from "@/commands/preferences/sync";
import RevokeInstallations from "@/commands/revoke-installations";

// Keep command IDs independent of the bundled output path.
export const COMMANDS: Record<string, Command.Class> = {
  "address-authorized": AddressAuthorized,
  "can-message": CanMessage,
  "client:add-account": ClientAddAccount,
  "client:change-recovery-identifier": ClientChangeRecoveryIdentifier,
  "client:inbox-id": ClientInboxId,
  "client:info": ClientInfo,
  "client:key-package-status": ClientKeyPackageStatus,
  "client:remove-account": ClientRemoveAccount,
  "client:revoke-all-other-installations": ClientRevokeAllOtherInstallations,
  "client:revoke-installations": ClientRevokeInstallations,
  "client:sign": ClientSign,
  "client:verify-signature": ClientVerifySignature,
  "conversation:add-admin": ConversationAddAdmin,
  "conversation:add-members": ConversationAddMembers,
  "conversation:add-super-admin": ConversationAddSuperAdmin,
  "conversation:consent-state": ConversationConsentState,
  "conversation:count-messages": ConversationCountMessages,
  "conversation:debug-info": ConversationDebugInfo,
  "conversation:list-admins": ConversationListAdmins,
  "conversation:list-super-admins": ConversationListSuperAdmins,
  "conversation:members": ConversationMembers,
  "conversation:messages": ConversationMessages,
  "conversation:permissions": ConversationPermissions,
  "conversation:publish-messages": ConversationPublishMessages,
  "conversation:remove-admin": ConversationRemoveAdmin,
  "conversation:remove-members": ConversationRemoveMembers,
  "conversation:remove-super-admin": ConversationRemoveSuperAdmin,
  "conversation:request-removal": ConversationRequestRemoval,
  "conversation:send-markdown": ConversationSendMarkdown,
  "conversation:send-reaction": ConversationSendReaction,
  "conversation:send-read-receipt": ConversationSendReadReceipt,
  "conversation:send-reply": ConversationSendReply,
  "conversation:send-text": ConversationSendText,
  "conversation:stream": ConversationStream,
  "conversation:sync": ConversationSync,
  "conversation:update-consent": ConversationUpdateConsent,
  "conversation:update-description": ConversationUpdateDescription,
  "conversation:update-image-url": ConversationUpdateImageUrl,
  "conversation:update-name": ConversationUpdateName,
  "conversation:update-permission": ConversationUpdatePermission,
  "conversations:create-dm": ConversationsCreateDm,
  "conversations:create-group": ConversationsCreateGroup,
  "conversations:get": ConversationsGet,
  "conversations:get-dm": ConversationsGetDm,
  "conversations:get-message": ConversationsGetMessage,
  "conversations:hmac-keys": ConversationsHmacKeys,
  "conversations:list": ConversationsList,
  "conversations:stream": ConversationsStream,
  "conversations:stream-all-messages": ConversationsStreamAllMessages,
  "conversations:sync": ConversationsSync,
  "conversations:sync-all": ConversationsSyncAll,
  "inbox-states": InboxStates,
  init: Init,
  "installation-authorized": InstallationAuthorized,
  "preferences:get-consent": PreferencesGetConsent,
  "preferences:inbox-state": PreferencesInboxState,
  "preferences:inbox-states": PreferencesInboxStates,
  "preferences:set-consent": PreferencesSetConsent,
  "preferences:stream": PreferencesStream,
  "preferences:sync": PreferencesSync,
  "revoke-installations": RevokeInstallations,
};
