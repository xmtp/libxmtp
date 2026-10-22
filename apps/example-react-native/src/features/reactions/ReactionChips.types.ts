export interface ReactionChip {
  /** "" for All. */
  readonly key: string;
  readonly title: string;
}

export interface ReactionChipsProps {
  readonly chips: readonly ReactionChip[];
  readonly selected: string;
  readonly onSelect: (key: string) => void;
}
