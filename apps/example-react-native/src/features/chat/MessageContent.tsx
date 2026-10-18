import { StyleSheet, View } from "react-native";

import { type ChatMessage, contentText } from "@/core/chatMessages.ts";
import { AttachmentPlaceholder } from "@/features/chat/AttachmentPlaceholder";
import { BubbleText } from "@/ui/Bubble";
import { useColors } from "@/ui/colors";
import { Text } from "@/ui/Text";

export interface MessageContentProps {
  readonly row: ChatMessage;
  readonly isOutgoing: boolean;
  /** The conversation's consent is allowed (attachments download only then). */
  readonly allowed: boolean;
}

/**
 * What a bubble holds, by content kind (`ChatMessages.swift` `messageContent`).
 * A new content kind adds its case to `ChatContent` (`core/chatMessages.ts`),
 * the SDK mapping (`sdkChatQueries.ts` `chatContent`) and a renderer here.
 */
export function MessageContent({
  row,
  isOutgoing,
  allowed,
}: MessageContentProps) {
  const { content } = row;
  switch (content.kind) {
    case "reply":
      return (
        <ReplyContent
          quote={content.quote}
          body={content.body}
          isOutgoing={isOutgoing}
        />
      );
    case "deleted":
      return (
        <BubbleText isOutgoing={isOutgoing} italic>
          {contentText(content)}
        </BubbleText>
      );
    case "attachment":
      return (
        <AttachmentPlaceholder
          row={row}
          file={content.file}
          isOutgoing={isOutgoing}
          allowed={allowed}
        />
      );
    case "reaction":
      return null;
    case "text":
    case "markdown":
    case "unsupported":
      return (
        <BubbleText isOutgoing={isOutgoing}>{contentText(content)}</BubbleText>
      );
  }
}

/** A reply: the quoted parent (caption, 3 lines, 8 pt padding) above the body. */
function ReplyContent({
  quote,
  body,
  isOutgoing,
}: {
  readonly quote: string;
  readonly body: string;
  readonly isOutgoing: boolean;
}) {
  const palette = useColors();
  return (
    <View style={styles.reply}>
      <View style={[styles.quote, { backgroundColor: palette.quoteFill }]}>
        <Text
          variant="caption"
          color={isOutgoing ? "onTint" : "label"}
          numberOfLines={3}>
          {quote}
        </Text>
      </View>
      <BubbleText isOutgoing={isOutgoing}>{body}</BubbleText>
    </View>
  );
}

const styles = StyleSheet.create({
  reply: { alignItems: "flex-start", gap: 6 },
  quote: { borderCurve: "continuous", borderRadius: 8, padding: 8 },
});
