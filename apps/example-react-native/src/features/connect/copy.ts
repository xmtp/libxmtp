import { commonCopy } from "@/copy/common";
import { SessionCopy } from "@/core/sessionError.ts";

/** Connect strings (`Features/Connect/Connect.xcstrings`, `ConnectView.swift`). */
export const connectCopy = {
  heroTitle: "A place to connect.",
  heroSubtitle: "Join a server to start a conversation.",
  yourServer: "Your server",
  serverAddress: "Server address",
  serverAddressPlaceholder: "https://server.example",
  serverName: "Server name (optional)",
  serverNamePlaceholder: "Server name",
  enterAddress: "Enter a server address",
  footnote: "Use a server address from someone you trust.",
  join: "Join a server",
  joining: "Joining server…",
  unreachable: SessionCopy.unreachable,
  enterToken: SessionCopy.enterToken,
  resetIncomplete: SessionCopy.resetIncomplete,
  continueReset: "Continue local data removal",
  restoreFailed: SessionCopy.missingSecret,
  retry: commonCopy.retry,
  deleteLocalData: "Delete local data",
  deleteLocalDataTitle: "Delete local data?",
  deleteLocalDataMessage:
    "This removes this server’s local identity, messages and files. It does not delete messages on other devices.",
} as const;
