import { Stack } from "expo-router";

import { SheetDetentProvider } from "@/features/details/sheetDetent";
import { useIsAccessibilitySize } from "@/ui/accessibility";
import { useStackScreenOptions } from "@/ui/navigation";

/**
 * The sheet's own stack. The content is transparent so the sheet shows its
 * glass at the 0.7 detent.
 */
export default function MemberLayout() {
  const screenOptions = useStackScreenOptions();
  const accessibilitySize = useIsAccessibilitySize();
  return (
    <SheetDetentProvider initiallyPartial={!accessibilitySize}>
      <Stack
        screenOptions={{
          ...screenOptions,
          headerLargeTitleEnabled: false,
          contentStyle: { backgroundColor: "transparent" },
        }}
      />
    </SheetDetentProvider>
  );
}
