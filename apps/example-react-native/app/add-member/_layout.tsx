import { Stack } from "expo-router";

import { SheetDetentProvider } from "@/features/details/sheetDetent";
import { useStackScreenOptions } from "@/ui/navigation";

/**
 * The sheet's own stack (SwiftUI `.sheet { NavigationStack { } }`). The
 * content is transparent so the sheet shows its glass at the medium detent.
 */
export default function AddMemberLayout() {
  const screenOptions = useStackScreenOptions();
  return (
    <SheetDetentProvider initiallyPartial>
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
