import { useEffect, useRef, useState } from "react";
import {
  Modal,
  Platform,
  Pressable,
  ScrollView,
  StyleSheet,
  View,
} from "react-native";
import Animated, {
  useAnimatedStyle,
  useSharedValue,
  withSpring,
  withTiming,
} from "react-native-reanimated";
import { scheduleOnRN } from "react-native-worklets";

import { commonCopy } from "@/copy/common";
import {
  MessageMenuCopy,
  type MessageMenuItem,
  QUICK_REACTIONS,
} from "@/core/messageInteractions.ts";
import type { MessageContextMenuProps } from "@/features/chat/interactions/MessageContextMenu.types";
import { useColors } from "@/ui/colors";
import { haptics } from "@/ui/haptics";
import {
  useMotion,
  transitionScale,
  reducedTiming,
  springConfig,
} from "@/ui/motion";
import { Symbol, type SymbolName } from "@/ui/Symbol";
import { Text } from "@/ui/Text";

export type { MessageContextMenuProps };

const itemSymbols: Partial<Record<MessageMenuItem, SymbolName>> = {
  copy: "doc.on.doc",
  reply: "arrowshape.turn.up.left",
  react: "face.smiling",
  delete: "trash",
};

/** One actionable reaction bar and message menu, animated with the app's motion policy. */
export function MessageContextMenu({
  items,
  attachmentActions = [],
  openRequest = 0,
  previewText,
  quickReactions,
  onItem,
  onReact,
  onMore,
  children,
}: MessageContextMenuProps) {
  const palette = useColors();
  const motion = useMotion();
  const [open, setOpen] = useState(false);
  const latestRequest = useRef(openRequest);
  const closing = useRef(false);
  const afterDismiss = useRef<(() => void) | undefined>(undefined);
  const progress = useSharedValue(0);
  useEffect(() => {
    if (latestRequest.current === openRequest) return;
    latestRequest.current = openRequest;
    closing.current = false;
    haptics.selection();
    setOpen(true);
  }, [openRequest]);
  useEffect(() => {
    if (open) progress.value = motion.animate(1);
  }, [open, motion, progress]);
  const animation = useAnimatedStyle(() => ({
    opacity: progress.value,
    transform: [
      {
        scale: motion.reduceMotion
          ? 1
          : transitionScale + (1 - transitionScale) * progress.value,
      },
    ],
  }));
  const dismissed = () => {
    const action = afterDismiss.current;
    afterDismiss.current = undefined;
    action?.();
  };
  const close = (then?: () => void) => {
    if (closing.current) return;
    closing.current = true;
    const finish = () => {
      afterDismiss.current = then;
      setOpen(false);
      // iOS cannot present the emoji sheet or file viewer while the menu's
      // native modal is still presented. Its onDismiss runs after teardown.
      if (Platform.OS !== "ios") dismissed();
    };
    const finished = (done?: boolean) => {
      "worklet";
      if (done) scheduleOnRN(finish);
    };
    progress.value = motion.reduceMotion
      ? withTiming(0, reducedTiming, finished)
      : withSpring(0, springConfig, finished);
  };
  return (
    <>
      <Pressable
        accessible={false}
        delayLongPress={400}
        onLongPress={() => {
          haptics.selection();
          closing.current = false;
          setOpen(true);
        }}>
        {children}
      </Pressable>
      <Modal
        visible={open}
        transparent
        animationType="none"
        onRequestClose={() => close()}
        onDismiss={dismissed}>
        <View style={[styles.backdrop, { backgroundColor: palette.menuScrim }]}>
          <Pressable
            style={StyleSheet.absoluteFill}
            accessibilityRole="button"
            accessibilityLabel={commonCopy.close}
            onPress={() => close()}
          />
          <Animated.View
            style={[styles.column, animation]}
            accessibilityViewIsModal>
            <ScrollView contentContainerStyle={styles.content} bounces={false}>
              {quickReactions ? (
                <View
                  style={[
                    styles.reactions,
                    {
                      backgroundColor: palette.secondarySystemGroupedBackground,
                    },
                  ]}>
                  {QUICK_REACTIONS.map((emoji) => (
                    <Pressable
                      key={emoji}
                      accessibilityRole="button"
                      accessibilityLabel={emoji}
                      style={({ pressed }) => [
                        styles.reaction,
                        pressed
                          ? {
                              backgroundColor: palette.tertiarySystemFill,
                              transform: [
                                { scale: motion.reduceMotion ? 1 : 0.9 },
                              ],
                            }
                          : undefined,
                      ]}
                      onPress={() => close(() => onReact(emoji))}>
                      <Text variant="title2">{emoji}</Text>
                    </Pressable>
                  ))}
                  <Pressable
                    accessibilityRole="button"
                    accessibilityLabel={MessageMenuCopy.more}
                    style={({ pressed }) => [
                      styles.reaction,
                      pressed
                        ? { backgroundColor: palette.tertiarySystemFill }
                        : undefined,
                    ]}
                    onPress={() => close(onMore)}>
                    <Symbol name="plus" />
                  </Pressable>
                </View>
              ) : null}
              <View
                style={[
                  styles.preview,
                  { backgroundColor: palette.secondarySystemGroupedBackground },
                ]}>
                <Text numberOfLines={4}>{previewText}</Text>
              </View>
              <View
                style={[
                  styles.menu,
                  { backgroundColor: palette.secondarySystemGroupedBackground },
                ]}>
                {attachmentActions.map((action) => (
                  <Pressable
                    key={action.id}
                    testID={action.id}
                    accessibilityRole="menuitem"
                    disabled={action.disabled}
                    accessibilityState={{ disabled: action.disabled ?? false }}
                    style={({ pressed }) => [
                      styles.item,
                      action.disabled ? styles.disabled : undefined,
                      pressed
                        ? { backgroundColor: palette.tertiarySystemFill }
                        : undefined,
                    ]}
                    onPress={() => close(action.run)}>
                    <Symbol
                      name={action.symbol}
                      color="tint"
                      style={styles.icon}
                    />
                    <Text style={styles.itemLabel}>{action.title}</Text>
                  </Pressable>
                ))}
                {items.map((item) => {
                  const symbol = itemSymbols[item];
                  return (
                    <Pressable
                      key={item}
                      accessibilityRole="menuitem"
                      style={({ pressed }) => [
                        styles.item,
                        pressed
                          ? { backgroundColor: palette.tertiarySystemFill }
                          : undefined,
                      ]}
                      onPress={() => close(() => onItem(item))}>
                      {symbol === undefined ? (
                        <View style={styles.icon} />
                      ) : (
                        <Symbol
                          name={symbol}
                          color={item === "delete" ? "systemRed" : "tint"}
                          style={styles.icon}
                        />
                      )}
                      <Text
                        color={item === "delete" ? "systemRed" : "label"}
                        style={styles.itemLabel}>
                        {MessageMenuCopy[item]}
                      </Text>
                    </Pressable>
                  );
                })}
              </View>
              <Pressable
                accessibilityRole="button"
                onPress={() => close()}
                style={[
                  styles.close,
                  { backgroundColor: palette.secondarySystemGroupedBackground },
                ]}>
                <Text>{commonCopy.close}</Text>
              </Pressable>
            </ScrollView>
          </Animated.View>
        </View>
      </Modal>
    </>
  );
}

const styles = StyleSheet.create({
  backdrop: {
    flex: 1,
    alignItems: "center",
    justifyContent: "center",
    padding: 16,
  },
  column: { maxHeight: "90%", width: 340, maxWidth: "100%" },
  content: { gap: 12 },
  preview: { borderRadius: 20, padding: 16 },
  reactions: {
    flexDirection: "row",
    flexWrap: "wrap",
    justifyContent: "center",
    borderRadius: 24,
    padding: 4,
  },
  reaction: {
    alignItems: "center",
    borderRadius: 16,
    minHeight: 44,
    minWidth: 44,
    justifyContent: "center",
    padding: 6,
  },
  menu: { borderRadius: 20, overflow: "hidden" },
  item: {
    alignItems: "center",
    flexDirection: "row",
    gap: 16,
    minHeight: 48,
    paddingHorizontal: 20,
    paddingVertical: 10,
  },
  itemLabel: { flexShrink: 1 },
  icon: { width: 24 },
  disabled: { opacity: 0.5 },
  close: {
    borderRadius: 20,
    alignItems: "center",
    minHeight: 48,
    justifyContent: "center",
    padding: 10,
  },
});
