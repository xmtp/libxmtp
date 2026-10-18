import { Stack } from "expo-router";
import { ScrollView, StyleSheet, View } from "react-native";

import { galleryCopy } from "@/features/dev/copy";
import { ActionFooter } from "@/ui/ActionFooter";
import { useColors } from "@/ui/colors";
import { GlassButton } from "@/ui/GlassButton";
import { Text } from "@/ui/Text";

/** The Connect footer (reference 01): a large glassProminent button. */
export function ConnectReplica() {
  const palette = useColors();
  return (
    <View
      style={[
        styles.screen,
        { backgroundColor: palette.systemGroupedBackground },
      ]}>
      <Stack.Screen options={{ headerShown: false }} />
      <ScrollView contentContainerStyle={styles.content}>
        <Text variant="footnote" color="themeSecondaryLabel">
          {galleryCopy.connect.footer}
        </Text>
      </ScrollView>
      <ActionFooter>
        <GlassButton
          variant="glassProminent"
          size="large"
          fullWidth
          title={galleryCopy.joinServer}
          testID="join-server"
          onPress={() => {}}
        />
      </ActionFooter>
    </View>
  );
}

const styles = StyleSheet.create({
  screen: { flex: 1 },
  content: { paddingHorizontal: 32, paddingTop: 600 },
});
