import { Pressable, StyleSheet, View } from "react-native";

import { useColors } from "@/ui/colors";
import type { SegmentedPickerProps } from "@/ui/SegmentedPicker.types";
import { Text } from "@/ui/Text";

export type {
  SegmentedOption,
  SegmentedPickerProps,
} from "@/ui/SegmentedPicker.types";

/** Plain React Native segmented control (Android), drawn like iOS 26's. */
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
  return (
    <View
      testID={testID}
      accessibilityRole="radiogroup"
      accessibilityLabel={accessibilityLabel}
      style={[
        styles.track,
        { backgroundColor: palette.tertiarySystemFill },
        disabled ? styles.disabled : undefined,
        style,
      ]}>
      {options.map((option) => {
        const selected = option.value === selection;
        return (
          <Pressable
            key={option.value}
            disabled={disabled}
            accessibilityRole="radio"
            accessibilityState={{ selected, disabled }}
            onPress={() => {
              onSelectionChange(option.value);
            }}
            style={[
              styles.segment,
              selected
                ? { backgroundColor: palette.secondarySystemGroupedBackground }
                : undefined,
            ]}>
            <Text
              variant="subheadline"
              weight={selected ? "semibold" : "medium"}>
              {option.label}
            </Text>
          </Pressable>
        );
      })}
    </View>
  );
}

const styles = StyleSheet.create({
  track: {
    width: "100%",
    borderRadius: 999,
    flexDirection: "row",
    padding: 3,
  },
  segment: {
    alignItems: "center",
    borderRadius: 999,
    flex: 1,
    justifyContent: "center",
    minHeight: 32,
  },
  disabled: { opacity: 0.4 },
});
