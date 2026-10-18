import { Stack } from "expo-router";

import { useStackScreenOptions } from "@/ui/navigation";

/**
 * The sheet's own stack (SwiftUI `.sheet { NavigationStack { } }`). The
 * picker draws its inline title bar itself; the content is transparent so
 * the sheet shows its glass.
 */
export default function EmojiPickerLayout() {
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
