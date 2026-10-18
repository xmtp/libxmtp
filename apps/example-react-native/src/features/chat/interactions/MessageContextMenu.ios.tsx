import {
  Button,
  ContextMenu,
  GlassEffectContainer,
  HStack,
  Host,
  RNHostView,
  Text,
  VStack,
} from "@expo/ui/swift-ui";
import {
  background,
  buttonStyle,
  font,
  glassEffect,
  labelStyle,
  padding,
  shapes,
  strokeBorder,
} from "@expo/ui/swift-ui/modifiers";
import { DynamicColorIOS } from "react-native";

import {
  MessageMenuCopy,
  type MessageMenuItem,
  QUICK_REACTIONS,
} from "@/core/messageInteractions.ts";
import type { MessageContextMenuProps } from "@/features/chat/interactions/MessageContextMenu.types";
import {
  useIncreasedContrast,
  useReduceTransparency,
} from "@/ui/accessibility";
import { type Palette, useColors } from "@/ui/colors";
import type { SymbolName } from "@/ui/Symbol";

export type { MessageContextMenuProps };

const itemSymbols: Partial<Record<MessageMenuItem, SymbolName>> = {
  copy: "doc.on.doc",
  reply: "arrowshape.turn.up.left",
  react: "face.smiling",
  delete: "trash",
};

/** The preview's corner radius (`RoundedRectangle(cornerRadius: 20)`). */
const PREVIEW_RADIUS = 20;

/** `.primary.opacity(…)`: the label color at the stroke's opacity. */
const strokeColors = {
  regular: DynamicColorIOS({
    light: "rgba(0,0,0,0.2)",
    dark: "rgba(255,255,255,0.2)",
  }),
  increased: DynamicColorIOS({
    light: "rgba(0,0,0,0.6)",
    dark: "rgba(255,255,255,0.6)",
  }),
};

/**
 * `messengerGlass(in:)` (`DesignSystem/Glass.swift`) for the preview: Liquid
 * Glass, or with Reduce Transparency the system background and a 1 pt stroke
 * in the label color at 20 percent (60 percent with Increase Contrast).
 */
function useMessengerGlass(palette: Palette) {
  const reduceTransparency = useReduceTransparency();
  const increasedContrast = useIncreasedContrast();
  if (!reduceTransparency) {
    return [
      glassEffect({
        glass: { variant: "regular" },
        shape: "roundedRectangle",
        cornerRadius: PREVIEW_RADIUS,
      }),
    ];
  }
  return [
    background(
      palette.systemBackground,
      shapes.roundedRectangle({ cornerRadius: PREVIEW_RADIUS }),
    ),
    strokeBorder({
      content: increasedContrast
        ? strokeColors.increased
        : strokeColors.regular,
      shape: "roundedRectangle",
      cornerRadius: PREVIEW_RADIUS,
    }),
  ];
}

/**
 * The bubble's long-press menu (`ChatMessages.swift`
 * `.contextMenu { menu(row) } preview: { messagePreview(row) }`): SwiftUI's
 * own context menu around the React Native bubble, with the glass preview
 * of quick reactions over the message text.
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
  const glass = useMessengerGlass(palette);
  return (
    <Host matchContents seedColor={palette.tint}>
      <ContextMenu>
        <ContextMenu.Trigger>
          <RNHostView matchContents>{children}</RNHostView>
        </ContextMenu.Trigger>
        <ContextMenu.Preview>
          <GlassEffectContainer>
            <VStack modifiers={[padding({ all: 8 }), ...glass]}>
              {quickReactions ? (
                <HStack>
                  {QUICK_REACTIONS.map((emoji) => (
                    <Button
                      key={emoji}
                      label={emoji}
                      modifiers={[buttonStyle("glass")]}
                      onPress={() => {
                        onReact(emoji);
                      }}
                    />
                  ))}
                  <Button
                    label={MessageMenuCopy.more}
                    systemImage="plus"
                    modifiers={[labelStyle("iconOnly"), buttonStyle("glass")]}
                    onPress={onMore}
                  />
                </HStack>
              ) : null}
              <Text
                modifiers={[
                  font({ textStyle: "body" }),
                  padding({ all: "default" }),
                ]}>
                {previewText}
              </Text>
            </VStack>
          </GlassEffectContainer>
        </ContextMenu.Preview>
        <ContextMenu.Items>
          {items.map((item) => (
            <Button
              key={item}
              label={MessageMenuCopy[item]}
              {...(itemSymbols[item] === undefined
                ? {}
                : { systemImage: itemSymbols[item] })}
              {...(item === "delete" ? { role: "destructive" as const } : {})}
              onPress={() => {
                onItem(item);
              }}
            />
          ))}
        </ContextMenu.Items>
      </ContextMenu>
    </Host>
  );
}
