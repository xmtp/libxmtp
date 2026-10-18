// Message request count and list paging rules
// (ExampleCore/Conversations/ConversationModels.swift `ListRules` and
// ConversationsStore.swift).

/** Chats reads at most this many requests for its badge. */
export const REQUEST_COUNT_LIMIT = 501;
/** Conversation list pages. */
export const CONVERSATION_PAGE_SIZE = 50;

/**
 * The "Message requests · N" badge count, or undefined (no badge number) when
 * the read reached the 501 limit. Dismissed requests do not count.
 */
export function requestBadgeCount(
  requestIds: readonly string[],
  dismissed: ReadonlySet<string>,
): number | undefined {
  if (requestIds.length >= REQUEST_COUNT_LIMIT) return undefined;
  return requestIds.filter((id) => !dismissed.has(id)).length;
}

/** Load the next page when the last visible row is within 3 of the end and no search runs. */
export function shouldLoadNextPage(
  lastVisible: number,
  loaded: number,
  search: string,
): boolean {
  return search === "" && lastVisible >= loaded - 3;
}

/**
 * The rows whose details to read: the visible rows plus 50 below them,
 * clamped to the loaded rows. Returns `[start, end)`.
 */
export function detailWindow(
  start: number,
  count: number,
  total: number,
): readonly [number, number] {
  const lower = Math.min(Math.max(0, start), total);
  return [lower, Math.min(total, lower + Math.max(0, count) + 50)];
}
