import { Stack } from "expo-router";

import { NewConversationSheet } from "@/features/new-conversation/NewConversationSheet";
import { useStackScreenOptions } from "@/ui/navigation";

/** The sheet's own stack (SwiftUI `.sheet { NavigationStack { } }`). */
export default function NewConversationLayout() {
  const screenOptions = useStackScreenOptions();
  return (
    <NewConversationSheet>
      <Stack
        screenOptions={{ ...screenOptions, headerLargeTitleEnabled: false }}
      />
    </NewConversationSheet>
  );
}
