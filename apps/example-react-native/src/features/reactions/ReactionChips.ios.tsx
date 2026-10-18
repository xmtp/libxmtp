import { Button, HStack, Host, ScrollView } from "@expo/ui/swift-ui";
import {
  accessibilityAddTraits,
  buttonStyle,
  padding,
} from "@expo/ui/swift-ui/modifiers";
import { StyleSheet } from "react-native";

import type { ReactionChipsProps } from "@/features/reactions/ReactionChips.types";
import { useColors } from "@/ui/colors";

/**
 * The filter chips (`ScrollView(.horizontal) { HStack { Button … } }`): glass
 * buttons with no visible selected style. Like iOS, only the emoji chips
 * carry the selected trait; "All" never does.
 */
export function ReactionChips({
  chips,
  selected,
  onSelect,
}: ReactionChipsProps) {
  const palette = useColors();
  return (
    <Host
      matchContents={{ vertical: true }}
      seedColor={palette.tint}
      style={styles.fill}>
      <ScrollView axes="horizontal">
        <HStack modifiers={[padding({ horizontal: 16 })]}>
          {chips.map((chip) => (
            <Button
              key={chip.key}
              label={chip.title}
              modifiers={[
                buttonStyle("glass"),
                ...(chip.key !== "" && chip.key === selected
                  ? [accessibilityAddTraits(["isSelected"])]
                  : []),
              ]}
              onPress={() => {
                onSelect(chip.key);
              }}
            />
          ))}
        </HStack>
      </ScrollView>
    </Host>
  );
}

const styles = StyleSheet.create({
  fill: { width: "100%" },
});
