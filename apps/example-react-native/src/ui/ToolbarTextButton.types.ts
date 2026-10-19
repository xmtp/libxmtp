export interface ToolbarTextButtonProps {
  readonly title: string;
  readonly placement: "left" | "right";
  /** The prominent toolbar style: a tinted capsule (Next, Create). */
  readonly prominent?: boolean;
  readonly disabled?: boolean;
  readonly onPress: () => void;
}
