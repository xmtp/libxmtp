import { Platform } from "react-native";

import { commonCopy } from "@/copy/common";
import { SessionCopy } from "@/core/sessionError.ts";

/** Settings strings (`Settings.xcstrings`, `Push.xcstrings`, `SettingsView.swift`). */
export const settingsCopy = {
  noHandle: "No handle",
  handleCaption: "Your handle on this server",
  otherHandles: "Other handles",
  otherHandlesDetail:
    "They are still yours, and people can still find you by them.",
  changeHandle: "Change handle",
  inboxId: commonCopy.inboxId,
  copyInboxId: commonCopy.copyInboxId,
  server: "Server",
  name: commonCopy.name,
  address: "Address",
  copyServerAddress: "Copy server address",
  serverFooter: "To change servers, sign out and join another server.",
  notifications: "Notifications",
  status: "Status",
  /** The push status of a build without push (`PushSettingsSection`). */
  pushStatus:
    Platform.OS === "ios"
      ? "Off: APNs is not configured"
      : "Off: Firebase is not configured",
  notificationsFooter:
    "Get alerts when new messages arrive. You can use Chats with notifications off.",
  appearance: "Appearance",
  automatic: "Automatic",
  light: "Light",
  dark: "Dark",
  appearanceFooter: "Choose how Chats looks on this device.",
  privacy: "Privacy",
  messageRequests: "Message requests",
  privacyFooter:
    "Message requests let you choose who can contact you. Files are verified before they can be opened or saved. Request files are not downloaded automatically.",
  developer: "Developer tools",
  attachmentRecovery: "Attachment recovery",
  developerFooter:
    "Unread counts use local insertion time. Equal timestamps and imported messages can change these counts. Open conversation details for raw metadata fields.",
  device: "This device",
  signOut: "Sign out",
  deleteLocalData: "Delete local data",
  resetIncomplete: SessionCopy.resetIncomplete,
  continueDeletion: "Continue deletion",
  deviceFooter:
    "Sign out ends this session and clears its access token. Your identity and saved messages stay on this device. Delete local data removes this server’s identity, saved messages and local files from this device.",
  signOutTitle: "Sign out?",
  signOutMessage: "Your messages and account keys will stay on this device.",
  deleteTitle: "Delete local data?",
  deleteMessage:
    "This removes this account and its messages from this device. This cannot be undone.",
  cancel: commonCopy.cancel,
  /** The avatar initials seed when there is no handle. */
  you: "You",
} as const;
