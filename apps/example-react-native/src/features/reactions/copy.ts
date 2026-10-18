/** Reactions sheet strings (`Features/Chat/Chat.xcstrings`). */
export const reactionsCopy = {
  title: "Reactions",
  close: "Close",
  /** "All 3". */
  all: (total: number) => `All ${String(total)}`,
  removeMine: "Remove my reaction",
  cannotLoad: "Cannot load reactions. Try again.",
} as const;
