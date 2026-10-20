export interface ToolbarTextItemProps {
  readonly title: string;
  /** The prominent toolbar style: a tinted capsule (Next, Create). */
  readonly prominent?: boolean;
  /** A confirming action (Save, Add): the title is semibold. */
  readonly confirmation?: boolean;
  readonly disabled?: boolean;
  readonly onPress: () => void;
}

export interface ToolbarTextButtonProps extends ToolbarTextItemProps {
  readonly placement: "left" | "right";
}
