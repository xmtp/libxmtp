import { Stack } from "expo-router";

import { NewConversationSheet } from "@/features/new-conversation/NewConversationSheet";
import { useStackScreenOptions } from "@/ui/navigation";

/** The sheet's own stack (SwiftUI `.sheet { NavigationStack { } }`). */
export default function NewConversationLayout() {
  const screenOptions = useStackScreenOptions();
  return (
    <NewConversationSheet>
      <Stack
        screenOptions={{
          ...screenOptions,
          headerLargeTitleEnabled: false,
          // A SwiftUI Form in a sheet shows the hard edge under its bar.
          scrollEdgeEffects: { top: "hard" },
        }}
      />
    </NewConversationSheet>
  );
}
