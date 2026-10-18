import { Stack, router } from "expo-router";
import { memo, useEffect, useMemo, useState } from "react";
import { FlatList, Pressable, StyleSheet, TextInput, View } from "react-native";

import {
  EmojiCopy,
  type EmojiEntry,
  addRecent,
  emojiSections,
  parseEmojiCatalog,
  searchEmoji,
} from "@/core/emojiCatalog.ts";
import { emojiSearchTsv } from "@/core/emojiData.ts";
import type { MessageActions } from "@/core/messageActions.ts";
import { createMessageActions } from "@/features/chat/chatRuntime";
import { type ActiveSession, useActiveSession } from "@/session";
import { useColors } from "@/ui/colors";
import { GlassIconButton } from "@/ui/GlassButton";
import { GlassSurface } from "@/ui/GlassSurface";
import { Symbol } from "@/ui/Symbol";
import { Text } from "@/ui/Text";
import { fontOf } from "@/ui/typography";
import { useStore } from "@/ui/useStore";

/** The grid's columns (`LazyVGrid` with 8 flexible columns). */
const COLUMNS = 8;

/** "Recently used": in memory for the process, like the iOS static array. */
let recent: readonly string[] = [];

/** The catalog, parsed once (`emoji-search.tsv`). */
let catalog: readonly EmojiEntry[] | undefined;
function loadCatalog(): readonly EmojiEntry[] {
  catalog ??= parseEmojiCatalog(emojiSearchTsv);
  return catalog;
}

type Item =
  | {
      readonly kind: "title";
      readonly key: string;
      readonly title: string;
      readonly recent: boolean;
    }
  | {
      readonly kind: "row";
      readonly key: string;
      readonly values: readonly EmojiEntry[];
    }
  | { readonly kind: "failure"; readonly key: string; readonly text: string };

function rows(key: string, values: readonly EmojiEntry[]): Item[] {
  const items: Item[] = [];
  for (let start = 0; start < values.length; start += COLUMNS) {
    items.push({
      kind: "row",
      key: `${key}:${String(start)}`,
      values: values.slice(start, start + COLUMNS),
    });
  }
  return items;
}

/** The scroll content (`EmojiPickerFeatureView` body) as list items. */
function pickerItems(
  entries: readonly EmojiEntry[],
  search: string,
  recents: readonly string[],
): Item[] {
  if (search !== "") return rows("search", searchEmoji(entries, search));
  const items: Item[] = [];
  if (recents.length > 0) {
    items.push({
      kind: "title",
      key: "recent",
      title: EmojiCopy.recentlyUsed,
      recent: true,
    });
    items.push(
      ...rows(
        "recent",
        recents.map((emoji) => ({ emoji, name: emoji })),
      ),
    );
  }
  for (const section of emojiSections(entries)) {
    items.push({
      kind: "title",
      key: section.title,
      title: section.title,
      recent: false,
    });
    items.push(...rows(section.title, section.values));
  }
  return items;
}

export interface EmojiPickerScreenProps {
  readonly conversationId: string;
  readonly messageId: string;
}

/**
 * The emoji picker sheet (`EmojiPickerFeatureView`): "Choose a reaction", a
 * Close button, an emoji search field, then "Recently used" and the catalog
 * sections in an 8-column grid. Choosing an emoji toggles that reaction on
 * the message and closes the sheet.
 */
export function EmojiPickerScreen({
  conversationId,
  messageId,
}: EmojiPickerScreenProps) {
  const session = useActiveSession();
  return (
    <View style={styles.screen}>
      <Stack.Screen options={{ title: EmojiCopy.title, headerShown: false }} />
      <View style={styles.header}>
        {/* A toolbar item: the bar's glass circle around a glass capsule. */}
        <GlassSurface cornerRadius={25} style={styles.toolbarItem}>
          <GlassIconButton
            systemImage="xmark"
            shape="capsule"
            accessibilityLabel={EmojiCopy.close}
            testID="emoji.close"
            onPress={() => {
              router.back();
            }}
          />
        </GlassSurface>
        <Text
          variant="headline"
          accessibilityRole="header"
          numberOfLines={1}
          style={styles.title}>
          {EmojiCopy.title}
        </Text>
        <View style={styles.balance} />
      </View>
      {session === undefined ? null : (
        <EmojiPickerContent
          key={`${String(session.generation)}:${messageId}`}
          session={session}
          conversationId={conversationId}
          messageId={messageId}
        />
      )}
    </View>
  );
}

function useMessageActions(
  session: ActiveSession,
  conversationId: string,
  messageId: string,
): MessageActions {
  const [mounted] = useState(() => ({ current: true }));
  const [actions] = useState(() =>
    createMessageActions(session, conversationId, () => mounted.current),
  );
  useEffect(() => {
    mounted.current = true;
    actions.load(messageId);
    return () => {
      mounted.current = false;
    };
  }, [actions, messageId, mounted]);
  return actions;
}

function EmojiPickerContent({
  session,
  conversationId,
  messageId,
}: EmojiPickerScreenProps & { readonly session: ActiveSession }) {
  const palette = useColors();
  const actions = useMessageActions(session, conversationId, messageId);
  const state = useStore(actions);
  const [search, setSearch] = useState("");
  const [failure, setFailure] = useState<string>();
  const entries = loadCatalog();
  const items = useMemo(() => {
    const list = pickerItems(entries, search, recent);
    return failure === undefined
      ? list
      : [...list, { kind: "failure" as const, key: "failure", text: failure }];
  }, [entries, search, failure]);

  const choose = (emoji: string) => {
    const row = state.message;
    if (row === undefined || !actions.allowed) return;
    actions.react(row, emoji);
    void actions.wait().then(() => {
      const error = actions.getState().error;
      if (error !== undefined) {
        setFailure(error);
        return;
      }
      recent = addRecent(recent, emoji);
      router.back();
    });
  };

  return (
    <View style={styles.screen}>
      <View
        style={[styles.search, { backgroundColor: palette.quaternaryLabel }]}>
        <Symbol name="magnifyingglass" color="secondaryLabel" />
        <TextInput
          testID="emoji.search"
          accessibilityLabel={EmojiCopy.search}
          value={search}
          onChangeText={setSearch}
          placeholder={EmojiCopy.search}
          placeholderTextColor={palette.placeholderText}
          selectionColor={palette.tint}
          autoCapitalize="none"
          autoCorrect={false}
          clearButtonMode="never"
          style={[styles.field, fontOf("body"), { color: palette.label }]}
        />
      </View>
      <View style={styles.list}>
        <FlatList
          contentInsetAdjustmentBehavior="never"
          data={items}
          keyExtractor={(item) => item.key}
          initialNumToRender={16}
          windowSize={7}
          keyboardShouldPersistTaps="handled"
          keyboardDismissMode="on-drag"
          contentContainerStyle={styles.content}
          renderItem={({ item, index }) => {
            switch (item.kind) {
              case "title":
                return (
                  <Text
                    variant={item.recent ? "headline" : "subheadline"}
                    weight={item.recent ? undefined : "semibold"}
                    accessibilityRole="header"
                    style={[
                      item.recent ? styles.recentTitle : styles.sectionTitle,
                      index === 0 ? styles.first : undefined,
                    ]}>
                    {item.title}
                  </Text>
                );
              case "row":
                return <EmojiRow values={item.values} onChoose={choose} />;
              case "failure":
                return (
                  <Text color="systemRed" style={styles.failure}>
                    {item.text}
                  </Text>
                );
            }
          }}
        />
      </View>
    </View>
  );
}

/** One grid row: up to 8 cells of 28 pt glyphs with 44 pt targets. */
const EmojiRow = memo(function EmojiRow({
  values,
  onChoose,
}: {
  readonly values: readonly EmojiEntry[];
  readonly onChoose: (emoji: string) => void;
}) {
  return (
    <View style={styles.row}>
      {Array.from({ length: COLUMNS }, (_, column) => {
        const entry = values.at(column);
        if (entry === undefined) {
          return <View key={column} style={styles.cell} />;
        }
        return (
          <Pressable
            key={column}
            accessibilityRole="button"
            accessibilityLabel={entry.name}
            testID={`emoji.option.${entry.emoji}`}
            style={styles.cell}
            onPress={() => {
              onChoose(entry.emoji);
            }}>
            <Text allowFontScaling={false} style={styles.glyph}>
              {entry.emoji}
            </Text>
          </Pressable>
        );
      })}
    </View>
  );
});

const styles = StyleSheet.create({
  screen: { flex: 1 },
  header: {
    alignItems: "center",
    flexDirection: "row",
    justifyContent: "space-between",
    paddingBottom: 4.8,
    paddingHorizontal: 16,
    paddingTop: 15.5,
  },
  toolbarItem: {
    alignItems: "center",
    height: 50,
    justifyContent: "center",
    width: 50,
  },
  balance: { height: 50, width: 50 },
  // The inline title sits 2 pt above the toolbar item's center.
  title: { marginBottom: 4 },
  search: {
    alignItems: "center",
    borderCurve: "continuous",
    borderRadius: 10,
    flexDirection: "row",
    gap: 8,
    marginHorizontal: 16,
    marginVertical: 12,
    padding: 10,
  },
  // SwiftUI's one-line body field is 21 pt tall (41 pt with the padding).
  field: { flex: 1, height: 21, lineHeight: 21, padding: 0 },
  list: { flex: 1, overflow: "hidden" },
  content: { paddingBottom: 16, paddingHorizontal: 16 },
  first: { paddingTop: 0 },
  recentTitle: { paddingBottom: 20 },
  sectionTitle: { paddingBottom: 12, paddingTop: 20 },
  row: { flexDirection: "row" },
  cell: {
    alignItems: "center",
    flex: 1,
    justifyContent: "center",
    minHeight: 44,
    minWidth: 44,
  },
  glyph: { fontSize: 28 },
  failure: { paddingTop: 20 },
});
