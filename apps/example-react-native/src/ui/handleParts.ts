// Pure helpers for `HandleText` and `ConversationTitle`
// (`DesignSystem/Identity.swift`, `ConversationTitle.swift`).

/** Strips one leading `@` (`HandleText.normalized`). */
export function normalizeHandle(handle: string): string {
  return handle.startsWith("@") ? handle.slice(1) : handle;
}

/**
 * Splits a handle once at the first `#`: the name and the `#code` suffix
 * (empty when there is none). One leading `@` is dropped first.
 */
export function splitHandle(handle: string): { name: string; code: string } {
  const normalized = normalizeHandle(handle);
  const index = normalized.indexOf("#");
  return index < 0
    ? { name: normalized, code: "" }
    : { name: normalized.slice(0, index), code: normalized.slice(index) };
}
