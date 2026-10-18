import type { ReactNode } from "react";
import { StyleSheet, View } from "react-native";
import { KeyboardStickyView } from "react-native-keyboard-controller";
import { useSafeAreaInsets } from "react-native-safe-area-context";

import { sizes } from "@/ui/tokens";

export interface ActionFooterProps {
  children?: ReactNode;
  testID?: string;
}

/**
 * Bottom actions above the home indicator and the keyboard
 * (`DesignSystem/ActionFooter.swift` placed with `.safeAreaInset(edge:
 * .bottom)`): a 10 pt stack, 20 pt sides, 8 pt above, at most 560 pt wide.
 * Render it after the screen's scroll view.
 */
export function ActionFooter({ children, testID }: ActionFooterProps) {
  const insets = useSafeAreaInsets();
  return (
    <KeyboardStickyView offset={{ closed: 0, opened: insets.bottom }}>
      <View
        testID={testID}
        style={[styles.outer, { paddingBottom: insets.bottom }]}>
        <View style={styles.stack}>{children}</View>
      </View>
    </KeyboardStickyView>
  );
}

const styles = StyleSheet.create({
  outer: { alignItems: "center", paddingHorizontal: 20, paddingTop: 8 },
  stack: { gap: 10, maxWidth: sizes.readableWidth, width: "100%" },
});
