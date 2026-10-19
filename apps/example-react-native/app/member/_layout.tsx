import { Stack } from "expo-router";

import { useStackScreenOptions } from "@/ui/navigation";

/** The sheet's own stack (SwiftUI `.sheet { NavigationStack { } }`). */
export default function MemberLayout() {
  const screenOptions = useStackScreenOptions();
  return (
    <Stack
      screenOptions={{ ...screenOptions, headerLargeTitleEnabled: false }}
    />
  );
}
