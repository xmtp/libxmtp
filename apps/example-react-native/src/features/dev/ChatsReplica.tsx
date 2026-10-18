import { Stack } from "expo-router";
import { ScrollView, StyleSheet, View } from "react-native";

import { galleryCopy } from "@/features/dev/copy";
import { useColors } from "@/ui/colors";
import { ContentUnavailable } from "@/ui/ContentUnavailable";
import { Chevron } from "@/ui/FormRows";
import { GlassButton } from "@/ui/GlassButton";
import { NavigationSubtitle } from "@/ui/NavigationSubtitle";
import { PlainListRow } from "@/ui/PlainListRow";
import { Symbol } from "@/ui/Symbol";
import { Text } from "@/ui/Text";
import { useToolbarIcon } from "@/ui/toolbarIcon";

const copy = galleryCopy.chats;

/** Chats in its empty state (reference 09): toolbar, subtitle, bottom search. */
export function ChatsReplica() {
  const palette = useColors();
  const settingsIcon = useToolbarIcon("gearshape");
  const composeIcon = useToolbarIcon("square.and.pencil");
  return (
    <ScrollView
      contentInsetAdjustmentBehavior="automatic"
      style={{ backgroundColor: palette.systemBackground }}
      contentContainerStyle={styles.content}>
      <Stack.Screen
        options={{ title: copy.title, headerLargeTitleEnabled: true }}
      />
      <NavigationSubtitle subtitle={galleryCopy.settings.serverUrl} />
      <Stack.Toolbar placement="left">
        <Stack.Toolbar.Button
          icon={settingsIcon}
          tintColor={palette.tint}
          accessibilityLabel={galleryCopy.settings.title}
          onPress={() => {}}
        />
      </Stack.Toolbar>
      <Stack.Toolbar placement="right">
        <Stack.Toolbar.Button
          icon={composeIcon}
          tintColor={palette.tint}
          accessibilityLabel={galleryCopy.newMessage}
          onPress={() => {}}
        />
      </Stack.Toolbar>
      <Stack.SearchBar
        placeholder={galleryCopy.search}
        onChangeText={() => {}}
      />
      <Stack.Toolbar placement="bottom">
        <Stack.Toolbar.SearchBarSlot />
      </Stack.Toolbar>
      <PlainListRow
        showsTopSeparator
        showsSeparator={false}
        separatorInset={56}
        onPress={() => {}}
        accessibilityLabel={`${copy.requests} ${copy.requestsCount}`}
        style={styles.requests}>
        <Symbol name="tray" textStyle="title3" color="tint" />
        <View style={styles.requestsText}>
          <Text variant="headline">
            {copy.requests} {copy.requestsCount}
          </Text>
          <Text variant="subheadline" color="secondaryLabel">
            {copy.requestsDetail}
          </Text>
        </View>
        <Chevron />
      </PlainListRow>
      <ContentUnavailable
        fill={false}
        systemImage="bubble.left.and.bubble.right"
        title={copy.emptyTitle}
        description={copy.emptyDetail}>
        <GlassButton
          variant="glassProminent"
          size="large"
          font="headline"
          title={copy.emptyAction}
          onPress={() => {}}
        />
      </ContentUnavailable>
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  content: { paddingBottom: 24 },
  requests: { paddingVertical: 13 },
  requestsText: { flex: 1, gap: 4 },
});
