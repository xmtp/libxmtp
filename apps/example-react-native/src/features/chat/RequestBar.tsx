import { router } from "expo-router";
import { useMemo } from "react";
import { StyleSheet, View } from "react-native";

import type { RequestDecisionsStore } from "@/core/requestDecisions.ts";
import { useChatContext } from "@/features/chat/ChatContext";
import { chatCopy } from "@/features/chat/copy";
import { useColors } from "@/ui/colors";
import { GlassButton } from "@/ui/GlassButton";
import { PopoverConfirmation } from "@/ui/PopoverConfirmation";
import { Symbol } from "@/ui/Symbol";
import { Text } from "@/ui/Text";
import { useStore } from "@/ui/useStore";

/**
 * The message request card that replaces the composer while consent is
 * unknown (`ChatRequestBar.swift`): opaque `IncomingBubble`, radius 24,
 * Sender details, Accept, Delete request and Block. The decisions are the
 * Requests decisions (`graph.requests`); Delete and Block ask "Confirm
 * request action" first, in a popover that points at the card.
 */
export function RequestBar({
  decisions,
}: {
  readonly decisions: RequestDecisionsStore;
}) {
  const { conversationId } = useChatContext();
  const palette = useColors();
  const state = useStore(decisions);
  const confirmation = state.confirmation;
  const request = useMemo(
    () =>
      confirmation === undefined
        ? undefined
        : {
            // iOS hides the title in the popover; the Android alert shows it.
            title: chatCopy.confirmRequestAction,
            confirm: chatCopy.confirm,
            destructive: true,
            onConfirm: () => {
              decisions.confirm(confirmation);
            },
          },
    [confirmation, decisions],
  );

  return (
    // `.padding(.horizontal, 16).padding(.bottom, 12)` comes before the
    // dialog modifier, so the popover points at the padded frame.
    <View style={styles.frame}>
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
            decisions.accept(conversationId);
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
                decisions.requestDelete(conversationId);
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
                decisions.requestBlock(conversationId);
              }}
            />
          </View>
        </View>
      </View>
      <PopoverConfirmation
        request={request}
        onDismiss={() => {
          decisions.cancel();
        }}
      />
    </View>
  );
}

const styles = StyleSheet.create({
  frame: { paddingBottom: 12, paddingHorizontal: 16 },
  card: {
    alignItems: "center",
    borderCurve: "continuous",
    borderRadius: 24,
    gap: 12,
    padding: 16,
  },
  decisions: { alignSelf: "stretch", flexDirection: "row", gap: 12 },
  decision: { flex: 1 },
});
