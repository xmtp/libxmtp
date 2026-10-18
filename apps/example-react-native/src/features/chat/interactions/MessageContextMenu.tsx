import { useState } from "react";
import { Modal, Pressable, StyleSheet, View } from "react-native";

import {
  MessageMenuCopy,
  type MessageMenuItem,
  QUICK_REACTIONS,
} from "@/core/messageInteractions.ts";
import type { MessageContextMenuProps } from "@/features/chat/interactions/MessageContextMenu.types";
import { useColors } from "@/ui/colors";
import { haptics } from "@/ui/haptics";
import { Symbol, type SymbolName } from "@/ui/Symbol";
import { Text } from "@/ui/Text";

export type { MessageContextMenuProps };

const itemSymbols: Partial<Record<MessageMenuItem, SymbolName>> = {
  copy: "doc.on.doc",
  reply: "arrowshape.turn.up.left",
  react: "face.smiling",
  delete: "trash",
};

/**
 * The bubble's long-press menu without SwiftUI (Android): a long press opens
 * a dimmed overlay with the quick reactions, the message text and the menu
 * items. A tap outside closes it.
 */
export function MessageContextMenu({
  items,
  previewText,
  quickReactions,
  onItem,
  onReact,
  onMore,
  children,
}: MessageContextMenuProps) {
  const palette = useColors();
  const [open, setOpen] = useState(false);
  const close = (then: () => void) => {
    setOpen(false);
    then();
  };
  return (
    <>
      <Pressable
        delayLongPress={400}
        onLongPress={() => {
          haptics.selection();
          setOpen(true);
        }}>
        {children}
      </Pressable>
      <Modal
        visible={open}
        transparent
        animationType="fade"
        onRequestClose={() => {
          setOpen(false);
        }}>
        <Pressable
          style={styles.backdrop}
          onPress={() => {
            setOpen(false);
          }}>
          <View style={styles.column}>
            <View
              style={[
                styles.preview,
                { backgroundColor: palette.secondarySystemGroupedBackground },
              ]}>
              {quickReactions ? (
                <View style={styles.reactions}>
                  {QUICK_REACTIONS.map((emoji) => (
                    <Pressable
                      key={emoji}
                      accessibilityRole="button"
                      accessibilityLabel={emoji}
                      style={[
                        styles.reaction,
                        { backgroundColor: palette.tertiarySystemFill },
                      ]}
                      onPress={() => {
                        close(() => {
                          onReact(emoji);
                        });
                      }}>
                      <Text>{emoji}</Text>
                    </Pressable>
                  ))}
                  <Pressable
                    accessibilityRole="button"
                    accessibilityLabel={MessageMenuCopy.more}
                    style={[
                      styles.reaction,
                      { backgroundColor: palette.tertiarySystemFill },
                    ]}
                    onPress={() => {
                      close(onMore);
                    }}>
                    <Symbol name="plus" />
                  </Pressable>
                </View>
              ) : null}
              <Text style={styles.previewText}>{previewText}</Text>
            </View>
            <View
              style={[
                styles.menu,
                { backgroundColor: palette.secondarySystemGroupedBackground },
              ]}>
              {items.map((item) => {
                const symbol = itemSymbols[item];
                const color = item === "delete" ? "systemRed" : "label";
                return (
                  <Pressable
                    key={item}
                    accessibilityRole="menuitem"
                    style={styles.item}
                    onPress={() => {
                      close(() => {
                        onItem(item);
                      });
                    }}>
                    {symbol === undefined ? (
                      <View style={styles.icon} />
                    ) : (
                      <Symbol
                        name={symbol}
                        color={item === "delete" ? "systemRed" : "tint"}
                        style={styles.icon}
                      />
                    )}
                    <Text color={color}>{MessageMenuCopy[item]}</Text>
                  </Pressable>
                );
              })}
            </View>
          </View>
        </Pressable>
      </Modal>
    </>
  );
}

const styles = StyleSheet.create({
  backdrop: {
    backgroundColor: "rgba(0,0,0,0.35)",
    flex: 1,
    justifyContent: "center",
    padding: 16,
  },
  column: { gap: 12 },
  preview: { borderRadius: 20, padding: 8 },
  reactions: { flexDirection: "row", gap: 8, justifyContent: "center" },
  reaction: {
    alignItems: "center",
    borderRadius: 22,
    height: 44,
    justifyContent: "center",
    width: 44,
  },
  previewText: { padding: 16 },
  menu: { alignSelf: "flex-start", borderRadius: 20, minWidth: 250 },
  item: {
    alignItems: "center",
    flexDirection: "row",
    gap: 16,
    minHeight: 48,
    paddingHorizontal: 20,
  },
  icon: { width: 24 },
});
