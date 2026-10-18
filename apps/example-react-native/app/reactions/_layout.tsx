import { Stack } from "expo-router";

import { useStackScreenOptions } from "@/ui/navigation";

/**
 * The sheet's own stack (SwiftUI `.sheet { NavigationStack { } }`). The
 * content is transparent so the sheet shows its own glass background.
 */
export default function ReactionsLayout() {
  const screenOptions = useStackScreenOptions();
  return (
    <Stack
      screenOptions={{
        ...screenOptions,
        headerLargeTitleEnabled: false,
        contentStyle: { backgroundColor: "transparent" },
      }}
    />
  );
}
