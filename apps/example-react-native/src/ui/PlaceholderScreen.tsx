import { Stack } from "expo-router";
import { ScrollView } from "react-native";

import { foundationCopy } from "@/copy/foundation";
import { useColors } from "@/ui/colors";
import { ContentUnavailable } from "@/ui/ContentUnavailable";
import { Text } from "@/ui/Text";

export interface PlaceholderScreenProps {
  /** The navigation title. */
  title: string;
  /** The route file, shown for orientation (for example `details/[conversationId]`). */
  route: string;
}

/** A route that a later change fills in. */
export function PlaceholderScreen({ title, route }: PlaceholderScreenProps) {
  const palette = useColors();
  return (
    <ScrollView
      contentInsetAdjustmentBehavior="automatic"
      style={{ backgroundColor: palette.systemGroupedBackground }}
      contentContainerStyle={{ flexGrow: 1 }}>
      <Stack.Screen options={{ title }} />
      <ContentUnavailable
        testID="placeholder"
        title={title}
        systemImage="hourglass"
        description={foundationCopy.placeholder}>
        <Text variant="footnote" color="secondaryLabel" monospaced>
          {route}
        </Text>
      </ContentUnavailable>
    </ScrollView>
  );
}
