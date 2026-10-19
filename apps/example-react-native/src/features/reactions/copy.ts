/** Reactions sheet strings. */
export const reactionsCopy = {
  title: "Reactions",
  close: "Close",
  /** "All 3". */
  all: (total: number) => `All ${String(total)}`,
  removeMine: "Remove my reaction",
} as const;
