// English strings of the attachment screens (Attachments.xcstrings,
// AttachmentRecovery.xcstrings, SharedMedia.xcstrings, and the attachment
// keys of Chat.xcstrings).

export { AttachmentCopy as attachmentCopy } from "@/core/attachments.ts";
export { SharedMediaCopy as sharedMediaCopy } from "@/core/sharedMedia.ts";

export const recoveryCopy = {
  title: "Attachment recovery",
  header: "Files that need review",
  intro:
    "Upload drafts are retained for 24 hours by default. Review an interrupted send before you send another file.",
  unknown:
    "Review send outcome. The original message may already be in the chat. Review it before you send another file.",
  unassigned:
    "Choose a chat for this draft. Unassigned file. Discard it or select a chat.",
  viewChat: "View chat",
  assignTo: (title: string) => `Assign to ${title}`,
  discard: "Discard",
  empty: "No files need review",
  loadFailed: "Cannot load saved files. Try again.",
} as const;
