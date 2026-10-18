import type { RemoteAttachment } from "@xmtp/react-native-sdk";
import { StyleSheet, View } from "react-native";

import type { ChatMessage } from "@/core/chatMessages.ts";
import { chatCopy } from "@/features/chat/copy";
import { Symbol } from "@/ui/Symbol";
import { Text } from "@/ui/Text";

export interface AttachmentContentProps {
  readonly row: ChatMessage;
  readonly file: RemoteAttachment;
  readonly isOutgoing: boolean;
  /** Downloads need an allowed conversation ("Closed until you accept"). */
  readonly allowed: boolean;
}

/**
 * A file message's card frame (`AttachmentCardView`: padding 12, max width
 * 260, `doc.fill` label). The attachments lane replaces this with the full
 * card: size, status, Download, Open and Save.
 */
export function AttachmentPlaceholder({
  file,
  isOutgoing,
}: AttachmentContentProps) {
  const color = isOutgoing ? "onTint" : "label";
  return (
    <View style={styles.card}>
      <View style={styles.label}>
        <Symbol name="doc.fill" textStyle="subheadline" color={color} />
        <Text variant="subheadline" weight="medium" color={color}>
          {file.filename ?? chatCopy.attachmentFallback}
        </Text>
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  card: { gap: 8, maxWidth: 260, padding: 12 },
  label: { alignItems: "center", flexDirection: "row", gap: 8 },
});
