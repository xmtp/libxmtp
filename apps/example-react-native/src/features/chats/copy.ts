/** Chats strings. */
export const chatsCopy = {
  title: "Chats",
  requests: "Message requests",
  requestsDetail: "Choose who can contact you",
  block: "Block",
  retry: "Retry",
  settings: "Settings",
  newMessage: "New message",
  blockConfirm: "Block conversation?",
  loading: "Loading conversations…",
  emptyTitle: "Your chats start here",
  emptyDetail:
    "Send a message to someone you know. Messages from new people appear in requests.",
  emptyAction: "Start a conversation",
  noMatches: "No matches",
  noMatchesDetail:
    "Try another name or message. Search covers loaded conversations.",
  /** The system search field's default prompt (`.searchable`). */
  search: "Search",
} as const;

/** "· N" after "Message requests". */
export function requestCountText(count: number): string {
  return `· ${String(count)}`;
}
