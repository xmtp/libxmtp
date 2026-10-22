import { ScrollView, StyleSheet } from "react-native";

import type { ReactionChipsProps } from "@/features/reactions/ReactionChips.types";
import { GlassButton } from "@/ui/GlassButton";

/** The filter chips without SwiftUI (Android). */
export function ReactionChips({ chips, onSelect }: ReactionChipsProps) {
  return (
    <ScrollView
      horizontal
      showsHorizontalScrollIndicator={false}
      style={styles.scroller}
      contentContainerStyle={styles.chips}>
      {chips.map((chip) => (
        <GlassButton
          key={chip.key}
          title={chip.title}
          onPress={() => {
            onSelect(chip.key);
          }}
        />
      ))}
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  scroller: { flexGrow: 0 },
  chips: { gap: 8, paddingHorizontal: 16 },
});
