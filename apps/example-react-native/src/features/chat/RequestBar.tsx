import { router } from "expo-router";
import { useEffect } from "react";
import { StyleSheet, View } from "react-native";

import { useChatContext } from "@/features/chat/ChatContext";
import { chatCopy } from "@/features/chat/copy";
import { useColors } from "@/ui/colors";
import { showConfirmation } from "@/ui/dialogs";
import { GlassButton } from "@/ui/GlassButton";
import { Symbol } from "@/ui/Symbol";
import { Text } from "@/ui/Text";
import { useStore } from "@/ui/useStore";

/**
 * The message request card that replaces the composer while consent is
 * unknown (`ChatRequestBar.swift`): opaque `IncomingBubble`, radius 24,
 * Sender details, Accept, Delete request and Block. Delete and Block ask
 * "Confirm request action" first.
 */
export function RequestBar() {
  const { runtime, conversationId } = useChatContext();
  const palette = useColors();
  const state = useStore(runtime.requests);
  const confirmation = state.confirmation;

  useEffect(() => {
    if (confirmation === undefined) return;
    showConfirmation({
      confirm: chatCopy.confirm,
      destructive: true,
      onConfirm: () => {
        runtime.requests.confirm();
      },
      onCancel: () => {
        runtime.requests.cancel();
      },
    });
  }, [confirmation, runtime]);

  return (
    <View
      testID="chat.request-bar"
      style={[styles.card, { backgroundColor: palette.incomingBubble }]}>
      <Symbol
        name="person.crop.circle.badge.questionmark"
        textStyle="title2"
        color="tint"
      />
      <Text variant="headline" align="center">
        {chatCopy.messageRequest}
      </Text>
      <Text variant="subheadline" color="themeSecondaryLabel" align="center">
        {chatCopy.requestDetail}
      </Text>
      <GlassButton
        title={chatCopy.senderDetails}
        trailingSystemImage="chevron.right"
        fullWidth
        testID="chat.sender-details"
        onPress={() => {
          router.push({
            pathname: "/details/[conversationId]",
            params: { conversationId },
          });
        }}
      />
      <GlassButton
        title={chatCopy.accept}
        variant="glassProminent"
        fullWidth
        labelFills
        disabled={state.busy}
        testID="chat.accept"
        onPress={() => {
          runtime.requests.accept();
        }}
      />
      <View style={styles.decisions}>
        <View style={styles.decision}>
          <GlassButton
            title={chatCopy.deleteRequest}
            role="destructive"
            fullWidth
            labelFills
            disabled={state.busy}
            testID="chat.delete-request"
            onPress={() => {
              runtime.requests.requestDelete();
            }}
          />
        </View>
        <View style={styles.decision}>
          <GlassButton
            title={chatCopy.block}
            role="destructive"
            fullWidth
            labelFills
            disabled={state.busy}
            testID="chat.block"
            onPress={() => {
              runtime.requests.requestBlock();
            }}
          />
        </View>
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  card: {
    alignItems: "center",
    borderCurve: "continuous",
    borderRadius: 24,
    gap: 12,
    marginBottom: 12,
    marginHorizontal: 16,
    padding: 16,
  },
  decisions: { alignSelf: "stretch", flexDirection: "row", gap: 12 },
  decision: { flex: 1 },
});
