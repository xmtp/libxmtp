import { commonCopy } from "@/copy/common";
import { SessionCopy } from "@/core/sessionError.ts";

/** Connect strings. */
export const connectCopy = {
  heroTitle: "A place to connect.",
  heroSubtitle: "Join a server to start a conversation.",
  yourServer: "Your server",
  serverAddress: "Server address",
  serverAddressPlaceholder: "https://server.example",
  serverName: "Server name (optional)",
  serverNamePlaceholder: "Server name",
  checking: "Checking server",
  ready: "Server ready",
  enterAddress: "Enter a server address",
  needsToken: "Needs a token",
  accessToken: "Access token",
  tokenFootnote: "Ask the server owner for an access token.",
  advanced: "Advanced",
  advancedSettings: "Advanced settings",
  hideAdvancedSettings: "Hide advanced settings",
  expanded: "Expanded",
  collapsed: "Collapsed",
  connectTo: "Connect to",
  footnote: "Use a server address from someone you trust.",
  advancedFootnote:
    "Use the full server address. Keep access tokens in the separate token field.",
  join: "Join a server",
  joining: "Joining server…",
  resetIncomplete: SessionCopy.resetIncomplete,
  continueReset: "Continue local data removal",
  restoreFailed: SessionCopy.missingSecret,
  retry: commonCopy.retry,
  deleteLocalData: "Delete local data",
  deleteLocalDataTitle: "Delete local data?",
  deleteLocalDataMessage:
    "This removes this server’s local identity, messages and files. It does not delete messages on other devices.",
} as const;
