export interface ToolbarTextButtonProps {
  readonly title: string;
  readonly placement: "left" | "right";
  /** `.borderedProminent` in a toolbar: a tinted capsule (Next, Create). */
  readonly prominent?: boolean;
  readonly disabled?: boolean;
  readonly onPress: () => void;
}
