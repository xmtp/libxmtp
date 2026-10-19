/**
 * Layout tokens, measured on iOS 26 screenshots (402 x 874 pt, iPhone 17
 * Pro). Values are points.
 */
export const spacing = {
  xxs: 2,
  xs: 4,
  s: 6,
  sm: 8,
  m: 10,
  md: 12,
  lg: 16,
  xl: 20,
  xxl: 24,
} as const;

export const radii = {
  /** Inset-grouped section (`List`/`Form`, iOS 26). */
  section: 26,
  /** Connect cards. */
  card: 24,
  /** Request bar card. */
  requestBar: 24,
  /** Composer text field glass. */
  composer: 22,
  /** Details shortcut tiles, context-menu preview glass. */
  tile: 20,
  /** Staged attachment card glass. */
  stagedCard: 16,
  /** Attachment previews. */
  preview: 12,
  /** Emoji search field, staged icon tile. */
  field: 10,
  /** Reply quote. */
  quote: 8,
  /** Settings navigation-row icon tile. */
  iconTile: 7,
  /** Use with `borderCurve: "continuous"` or a capsule. */
  capsule: 999,
} as const;

export const avatarSizes = {
  chatSender: 24,
  chatHeader: 30,
  reactions: 36,
  row: 44,
  memberSheet: 52,
  settings: 64,
  newGroup: 72,
  details: 96,
} as const;

/** iOS 26 inset-grouped list metrics (measured against SwiftUI `Form`). */
export const form = {
  /** Screen edge to section. */
  margin: 16,
  /** Section edge to row content. */
  rowInset: 16,
  /** Single-line row. */
  rowMinHeight: 52,
  /** Vertical padding of a row whose content is taller than one line. */
  rowPaddingVertical: 15,
  sectionRadius: radii.section,
  /** Settings icon tiles and Details symbols. */
  iconTileSize: 28,
  iconColumn: 24,
  iconGap: 12,
} as const;

export const sizes = {
  /** `Theme.minimumHitTarget`. */
  minimumHitTarget: 44,
  /** Glass bar buttons and icon buttons. */
  glassButton: 44,
  /** `ActionFooter` content width limit. */
  readableWidth: 560,
} as const;
