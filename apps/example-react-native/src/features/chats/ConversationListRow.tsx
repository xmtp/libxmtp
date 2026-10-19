import { router, useFocusEffect } from "expo-router";
import { useCallback, useRef, useState } from "react";
import { Alert, Platform, Pressable, StyleSheet, View } from "react-native";
import ReanimatedSwipeable, {
  type SwipeableMethods,
} from "react-native-gesture-handler/ReanimatedSwipeable";

import { commonCopy } from "@/copy/common";
import {
  type ConversationRow,
  conversationRowLabel,
} from "@/core/conversationList.ts";
import {
  type RowAction,
  SwipeGroup,
  menuAlertButtons,
  rowAccessibilityActions,
} from "@/core/rowActions.ts";
import {
  ConversationRowView,
  useInitialsInset,
} from "@/features/chats/ConversationRowView";
import { useColors } from "@/ui/colors";
import { ContextMenu } from "@/ui/ContextMenu";
import { PlainListRow } from "@/ui/PlainListRow";
import { Text } from "@/ui/Text";

export type RowSwipeGroup = SwipeGroup<SwipeableMethods>;

/**
 * The list's swipe group: one open row at a time, closed when the screen
 * loses focus so no open row survives a navigation.
 */
export function useSwipeGroup(): RowSwipeGroup {
  const [group] = useState<RowSwipeGroup>(() => new SwipeGroup());
  useFocusEffect(
    useCallback(
      () => () => {
        group.closeAll();
      },
      [group],
    ),
  );
  return group;
}

export interface ConversationListRowProps {
  row: ConversationRow;
  /** `chats.row` or `requests.row`; the row ID follows. */
  testIDPrefix: string;
  /** Context menu items, top to bottom. */
  menu: readonly RowAction[];
  /** Trailing swipe actions, outermost first (SwiftUI `swipeActions` order). */
  swipe: readonly RowAction[];
  swipeGroup: RowSwipeGroup;
  /**
   * `clear` (default): Chats' `.listRowBackground(Color.clear)`. `system`:
   * SwiftUI's default row background (Message requests), which lifts on the
   * elevated color in a context menu and turns gray while swiped open.
   */
  rowBackground?: "clear" | "system";
}

function showAndroidMenu(title: string, actions: readonly RowAction[]): void {
  Alert.alert(
    title,
    undefined,
    [...menuAlertButtons(actions, commonCopy.cancel)],
    { cancelable: true },
  );
}

/**
 * A conversation row that opens its chat, with trailing swipe actions and a
 * long-press context menu (a native UIKit menu on iOS, an alert on Android).
 */
export function ConversationListRow({
  row,
  testIDPrefix,
  menu,
  swipe,
  swipeGroup,
  rowBackground = "clear",
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
  const accessibilityActions = rowAccessibilityActions(menu, swipe);
  const swiped = revealed && rowBackground === "system";

  const content = (
    <PlainListRow
      testID={`${testIDPrefix}.${row.id}`}
      accessibilityLabel={conversationRowLabel(row)}
      accessibilityActions={accessibilityActions.map((action) => ({
        name: action.label,
        label: action.label,
      }))}
      onAccessibilityAction={(event) => {
        accessibilityActions
          .find((action) => action.label === event.nativeEvent.actionName)
          ?.onPress();
      }}
      showsSeparator={false}
      backgroundColor={swiped ? palette.systemGray5 : palette.systemBackground}
      onPress={() => {
        // While a row's actions are open, a tap only closes them.
        if (swipeGroup.closeAll()) return;
        router.push({
          pathname: "/chat/[conversationId]",
          params: { conversationId: row.id },
        });
      }}
      onLongPress={
        Platform.OS === "ios"
          ? undefined
          : () => {
              if (swipeGroup.closeAll()) return;
              showAndroidMenu(row.title, menu);
            }
      }
      style={[styles.row, swiped ? styles.swiped : undefined]}>
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
                  swipeGroup.closeAll();
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
          liftedBackground={rowBackground === "system" ? "elevated" : "page"}
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
}

const styles = StyleSheet.create({
  // `.padding(.vertical, 5)` inside the 15 pt list row inset.
  row: { paddingVertical: 20 },
  // iOS 26 rounds a swiped row's gray background (measured 28 pt).
  swiped: { borderCurve: "continuous", borderRadius: 28 },
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
