import { Host, Picker, Text } from "@expo/ui/swift-ui";
import {
  accessibilityLabel as accessibilityLabelModifier,
  disabled as disabledModifier,
  labelsHidden,
  pickerStyle,
  tag,
  type ViewModifier,
} from "@expo/ui/swift-ui/modifiers";
import { StyleSheet } from "react-native";

import { useColors } from "@/ui/colors";
import type { SegmentedPickerProps } from "@/ui/SegmentedPicker.types";

export type {
  SegmentedOption,
  SegmentedPickerProps,
} from "@/ui/SegmentedPicker.types";

/** The native segmented control (`Picker` with `.pickerStyle(.segmented)`). */
export function SegmentedPicker<T extends string>({
  options,
  selection,
  onSelectionChange,
  accessibilityLabel,
  disabled = false,
  testID,
  style,
}: SegmentedPickerProps<T>) {
  const palette = useColors();
  const modifiers: ViewModifier[] = [
    pickerStyle("segmented"),
    labelsHidden(),
    disabledModifier(disabled),
  ];
  if (accessibilityLabel !== undefined) {
    modifiers.push(accessibilityLabelModifier(accessibilityLabel));
  }
  return (
    <Host
      matchContents={{ vertical: true }}
      seedColor={palette.tint}
      style={[styles.host, style]}>
      <Picker<string>
        label={accessibilityLabel}
        selection={selection}
        onSelectionChange={(value) => {
          const option = options.find((candidate) => candidate.value === value);
          if (option !== undefined) onSelectionChange(option.value);
        }}
        testID={testID}
        modifiers={modifiers}>
        {options.map((option) => (
          <Text key={option.value} modifiers={[tag(option.value)]}>
            {option.label}
          </Text>
        ))}
      </Picker>
    </Host>
  );
}

const styles = StyleSheet.create({
  host: { alignSelf: "stretch" },
});
