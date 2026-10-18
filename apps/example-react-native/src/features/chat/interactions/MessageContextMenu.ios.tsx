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
  buttonStyle,
  font,
  glassEffect,
  labelStyle,
  padding,
} from "@expo/ui/swift-ui/modifiers";

import {
  MessageMenuCopy,
  type MessageMenuItem,
  QUICK_REACTIONS,
} from "@/core/messageInteractions.ts";
import type { MessageContextMenuProps } from "@/features/chat/interactions/MessageContextMenu.types";
import { useColors } from "@/ui/colors";
import type { SymbolName } from "@/ui/Symbol";

export type { MessageContextMenuProps };

const itemSymbols: Partial<Record<MessageMenuItem, SymbolName>> = {
  copy: "doc.on.doc",
  reply: "arrowshape.turn.up.left",
  react: "face.smiling",
  delete: "trash",
};

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
  return (
    <Host matchContents seedColor={palette.tint}>
      <ContextMenu>
        <ContextMenu.Trigger>
          <RNHostView matchContents>{children}</RNHostView>
        </ContextMenu.Trigger>
        <ContextMenu.Preview>
          <GlassEffectContainer>
            <VStack
              modifiers={[
                padding({ all: 8 }),
                glassEffect({
                  glass: { variant: "regular" },
                  shape: "roundedRectangle",
                  cornerRadius: 20,
                }),
              ]}>
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
