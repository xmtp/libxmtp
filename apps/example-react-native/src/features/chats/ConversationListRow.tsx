import { router } from "expo-router";
import { memo, useRef, useState } from "react";
import { Alert, Platform, Pressable, StyleSheet, View } from "react-native";
import ReanimatedSwipeable, {
  type SwipeableMethods,
} from "react-native-gesture-handler/ReanimatedSwipeable";

import { commonCopy } from "@/copy/common";
import type { ConversationRow } from "@/core/conversationList.ts";
import {
  ConversationRowView,
  conversationRowLabel,
  useInitialsInset,
} from "@/features/chats/ConversationRowView";
import { useColors } from "@/ui/colors";
import { ContextMenu } from "@/ui/ContextMenu";
import { PlainListRow } from "@/ui/PlainListRow";
import { Text } from "@/ui/Text";

export interface RowAction {
  readonly label: string;
  readonly destructive?: boolean;
  readonly onPress: () => void;
}

/** Keeps at most one row's swipe actions open, like a SwiftUI `List`. */
export class SwipeGroup {
  #open: SwipeableMethods | undefined;

  opened(row: SwipeableMethods): void {
    if (this.#open !== row) this.#open?.close();
    this.#open = row;
  }

  closed(row: SwipeableMethods): void {
    if (this.#open === row) this.#open = undefined;
  }

  closeAll(): void {
    this.#open?.close();
    this.#open = undefined;
  }
}

export interface ConversationListRowProps {
  row: ConversationRow;
  /** Context menu items, top to bottom. */
  menu: readonly RowAction[];
  /** Trailing swipe actions, outermost first (SwiftUI `swipeActions` order). */
  swipe: readonly RowAction[];
  swipeGroup: SwipeGroup;
}

function showAndroidMenu(title: string, actions: readonly RowAction[]): void {
  Alert.alert(title, undefined, [
    ...actions.map((action) => ({
      text: action.label,
      style: action.destructive === true ? ("destructive" as const) : undefined,
      onPress: action.onPress,
    })),
    { text: commonCopy.cancel, style: "cancel" as const },
  ]);
}

/**
 * A conversation row that opens its chat, with trailing swipe actions and a
 * long-press context menu (a native UIKit menu on iOS, an alert on Android).
 */
export const ConversationListRow = memo(function ConversationListRow({
  row,
  menu,
  swipe,
  swipeGroup,
}: ConversationListRowProps) {
  const palette = useColors();
  const swipeable = useRef<SwipeableMethods>(null);
  // Open: actions reachable. Revealed: drawn (only while a swipe shows them,
  // so a lifted context-menu row never shows them behind it).
  const [open, setOpen] = useState(false);
  const [revealed, setRevealed] = useState(false);
  // iOS 26 swipe buttons: as wide as the widest label plus 12 pt a side,
  // 8 pt shorter than the row up to 80 pt, centered.
  const [labelWidth, setLabelWidth] = useState(0);
  const [rowHeight, setRowHeight] = useState(0);
  const [inset, onInitialsLayout] = useInitialsInset();

  const content = (
    <PlainListRow
      testID={`chats.row.${row.id}`}
      accessibilityLabel={conversationRowLabel(row)}
      showsSeparator={false}
      backgroundColor={palette.systemBackground}
      onPress={() => {
        router.push({
          pathname: "/chat/[conversationId]",
          params: { conversationId: row.id },
        });
      }}
      onLongPress={
        Platform.OS === "ios"
          ? undefined
          : () => {
              showAndroidMenu(row.title, menu);
            }
      }
      style={styles.row}>
      <ConversationRowView row={row} onInitialsLayout={onInitialsLayout} />
    </PlainListRow>
  );

  return (
    <View
      onLayout={(event) => {
        setRowHeight(event.nativeEvent.layout.height);
      }}>
      <ReanimatedSwipeable
        ref={swipeable}
        friction={1}
        overshootRight={false}
        rightThreshold={40}
        onSwipeableOpenStartDrag={() => {
          setRevealed(true);
        }}
        onSwipeableWillOpen={() => {
          setRevealed(true);
          setOpen(true);
          if (swipeable.current !== null) swipeGroup.opened(swipeable.current);
        }}
        onSwipeableClose={() => {
          setOpen(false);
          setRevealed(false);
          if (swipeable.current !== null) swipeGroup.closed(swipeable.current);
        }}
        renderRightActions={() => (
          <View
            style={[styles.actions, revealed ? undefined : styles.hidden]}
            accessibilityElementsHidden={!open}
            importantForAccessibility={open ? "auto" : "no-hide-descendants"}>
            {[...swipe].reverse().map((action) => (
              <Pressable
                key={action.label}
                accessibilityRole="button"
                accessibilityLabel={action.label}
                onPress={() => {
                  swipeable.current?.close();
                  action.onPress();
                }}
                style={({ pressed }) => [
                  styles.action,
                  {
                    backgroundColor: palette.tint,
                    height: Math.min(80, Math.max(0, rowHeight - 8)),
                    minWidth: labelWidth + 24,
                  },
                  pressed ? styles.pressed : undefined,
                ]}>
                <Text
                  variant="footnote"
                  color="onTint"
                  onLayout={(event) => {
                    const width = Math.ceil(event.nativeEvent.layout.width);
                    setLabelWidth((current) => Math.max(current, width));
                  }}>
                  {action.label}
                </Text>
              </Pressable>
            ))}
          </View>
        )}>
        <ContextMenu
          items={menu.map((action) => ({
            id: action.label,
            title: action.label,
            ...(action.destructive === undefined
              ? {}
              : { destructive: action.destructive }),
          }))}
          onSelect={(id) => {
            menu.find((action) => action.label === id)?.onPress();
          }}>
          {content}
        </ContextMenu>
      </ReanimatedSwipeable>
      {/* A List separator stays in place while its row slides. */}
      <View
        pointerEvents="none"
        style={[
          styles.separator,
          { backgroundColor: palette.separator, left: inset },
        ]}
      />
    </View>
  );
});

const styles = StyleSheet.create({
  // `.padding(.vertical, 5)` inside the 15 pt list row inset.
  row: { paddingVertical: 20 },
  actions: {
    alignItems: "center",
    flexDirection: "row",
    gap: 10.5,
    paddingHorizontal: 10.5,
  },
  action: {
    alignItems: "center",
    borderRadius: 18,
    justifyContent: "center",
    paddingHorizontal: 12,
  },
  pressed: { opacity: 0.7 },
  hidden: { opacity: 0 },
  separator: { bottom: 0, height: 1, position: "absolute", right: 16 },
});
