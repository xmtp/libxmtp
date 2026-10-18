import { router } from "expo-router";
import { StyleSheet, TextInput, View } from "react-native";

import { composerEnabled } from "@/core/chatComposer.ts";
import { contentText } from "@/core/chatMessages.ts";
import { useChatContext } from "@/features/chat/ChatContext";
import {
  ComposerAttachMenu,
  StagedAttachments,
} from "@/features/chat/ComposerSlots";
import { chatCopy } from "@/features/chat/copy";
import { useBodyScale } from "@/ui/accessibility";
import { useColors } from "@/ui/colors";
import { GlassButton, GlassIconButton } from "@/ui/GlassButton";
import { GlassSurface } from "@/ui/GlassSurface";
import { Text } from "@/ui/Text";
import { useStore } from "@/ui/useStore";

/** The field's 11 pt vertical padding around 1 to 5 body lines. */
const FIELD_PADDING = 11;

/**
 * The field's text metrics at a body scale: SwiftUI's body size (17 pt, 53 pt
 * at AX5) with one line 5 pt taller than the font (22 pt at the default
 * size). React Native would scale a text input by its own, larger factor.
 */
function fieldMetrics(bodyScale: number) {
  const fontSize = Math.round(17 * bodyScale);
  const lineHeight = fontSize + 5;
  return {
    fontSize,
    lineHeight,
    minHeight: lineHeight + FIELD_PADDING * 2,
    maxHeight: lineHeight * 5 + FIELD_PADDING * 2,
  };
}

/**
 * The composer (`ChatComposer.swift`): staged attachments, the error line,
 * "Review send" when a send outcome is unknown, the reply preview, then the
 * input row: attach menu, the glass "Message" field (1 to 5 lines) and the
 * prominent send arrow, shown only while the field has text.
 */
export function Composer() {
  const { runtime, conversationId, composerRef } = useChatContext();
  const palette = useColors();
  const bodyScale = useBodyScale();
  const state = useStore(runtime.composer);
  const timeline = useStore(runtime.timeline);
  const allowed = timeline.conversation?.consent === "allowed";
  const enabled = composerEnabled(state);
  const { draft } = state;
  const replyRow =
    draft.replyId === undefined
      ? undefined
      : timeline.messages.find((row) => row.id === draft.replyId);
  return (
    <View ref={composerRef} style={styles.composer} testID="chat.composer">
      <StagedAttachments conversationId={conversationId} allowed={allowed} />
      {state.error === undefined ? null : (
        <Text
          variant="caption"
          color="secondaryLabel"
          align="center"
          testID="composer.error">
          {state.error}
        </Text>
      )}
      {state.unknown ? (
        <GlassButton
          title={chatCopy.reviewSend}
          testID="composer.review-send"
          style={styles.centered}
          onPress={() => {
            router.push("/settings");
          }}
        />
      ) : null}
      {draft.replyId === undefined ? null : (
        <View style={styles.reply}>
          <Text
            variant="caption"
            numberOfLines={2}
            style={styles.replyText}
            testID="composer.reply-preview">
            {chatCopy.replyTo(
              replyRow === undefined
                ? undefined
                : contentText(replyRow.content),
            )}
          </Text>
          <GlassIconButton
            systemImage="xmark"
            shape="capsule"
            accessibilityLabel={chatCopy.cancelReply}
            testID="composer.cancel-reply"
            onPress={() => {
              runtime.cancelReply();
            }}
          />
        </View>
      )}
      <View style={styles.input}>
        <ComposerAttachMenu conversationId={conversationId} allowed={allowed} />
        <GlassSurface cornerRadius={22} style={styles.field}>
          <TextInput
            testID="message-composer"
            accessibilityLabel={chatCopy.messagePlaceholder}
            value={draft.text}
            onChangeText={(text) => {
              runtime.composer.edit(text);
            }}
            editable={enabled}
            multiline
            placeholder={chatCopy.messagePlaceholder}
            placeholderTextColor={palette.placeholderText}
            selectionColor={palette.tint}
            allowFontScaling={false}
            style={[
              styles.text,
              fieldMetrics(bodyScale),
              { color: enabled ? palette.label : palette.tertiaryLabel },
            ]}
          />
        </GlassSurface>
        {draft.text === "" ? null : (
          <GlassIconButton
            systemImage="arrow.up"
            variant="glassProminent"
            shape="capsule"
            accessibilityLabel={chatCopy.sendMessage}
            testID="send-message"
            disabled={!enabled}
            onPress={() => {
              runtime.submit();
            }}
          />
        )}
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  composer: { gap: 8, padding: 12 },
  centered: { alignSelf: "center" },
  reply: { alignItems: "center", flexDirection: "row", gap: 8 },
  replyText: { flex: 1 },
  input: { alignItems: "flex-end", flexDirection: "row", gap: 10 },
  field: { flex: 1 },
  text: {
    paddingBottom: FIELD_PADDING,
    paddingHorizontal: 14,
    paddingTop: FIELD_PADDING,
  },
});
