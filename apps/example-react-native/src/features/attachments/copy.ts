// English strings of the attachment screens.

import { commonCopy } from "@/copy/common";
import { SharedMediaCopy } from "@/core/sharedMedia.ts";

export { AttachmentCopy as attachmentCopy } from "@/core/attachments.ts";

export const attachmentPresentationCopy = {
  details: "Details",
  done: "Done",
  actions: "Attachment actions",
  image: "Image attachment",
  tapHint: "Tap to show sender and time. Long press for actions.",
  fileHint: "Tap for details. Long press for actions.",
  you: "You",
};

export const sharedMediaCopy = {
  ...SharedMediaCopy,
  retry: commonCopy.retry,
} as const;

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
  retry: commonCopy.retry,
  empty: "No files need review",
  loadFailed: "Cannot load saved files. Try again.",
} as const;
