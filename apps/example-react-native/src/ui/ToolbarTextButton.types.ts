export interface ToolbarTextItemProps {
  readonly title: string;
  /** The prominent toolbar style: a tinted capsule (Next, Create). */
  readonly prominent?: boolean;
  /**
   * `ToolbarItem(placement: .confirmationAction)` (Save, Add): SwiftUI draws
   * the title semibold.
   */
  readonly confirmation?: boolean;
  readonly disabled?: boolean;
  readonly onPress: () => void;
}

export interface ToolbarTextButtonProps extends ToolbarTextItemProps {
  readonly placement: "left" | "right";
}
