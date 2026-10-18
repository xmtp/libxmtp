import type { StyleProp, ViewStyle } from "react-native";

export interface SegmentedOption<T extends string> {
  value: T;
  label: string;
}

export interface SegmentedPickerProps<T extends string> {
  options: readonly SegmentedOption<T>[];
  selection: T;
  onSelectionChange: (value: T) => void;
  /** The control's spoken name (`Picker(label)` with `.labelsHidden()`). */
  accessibilityLabel?: string;
  disabled?: boolean;
  testID?: string;
  style?: StyleProp<ViewStyle>;
}
